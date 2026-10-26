use crate::kernel::layout::{Axis, Dim, PlaceKind, PlaceSpec, Side, Size};
use crate::kernel::{Kernel, Place, TabId};
use pumi_api::{Caller, Error, Result};
use serde_json::json;
/// Whether panes hold several tabs (`pane_tabs`, on by default). Off, a tab put into a pane
/// splits it instead, as tmux has one program to a pane.
pub fn pane_tabs(k: &Kernel) -> bool {
    k.global_option("pane_tabs").as_bool().unwrap_or(true)
}
/// The share of `rect` along `side`'s axis a tiled `size` is.
fn share(size: &Option<serde_json::Value>, side: Side, rect: Option<crate::kernel::layout::Rect>) -> Result<Option<serde_json::Value>> {
    let Some(size) = size else { return Ok(None) };
    let span = rect.map_or(1, |r| if side.axis() == Axis::Row { r.width } else { r.height });
    Ok(Some(json!(Size::parse(size)?.share_of(span))))
}
/// Where a tab goes, with everything the core needs chosen: the pane it joins or splits, the
/// view, the session. A pane that is gone falls back to `beside` and `view`, as a closed tab
/// reopening does.
pub fn resolve(k: &Kernel, caller: &Caller, spec: &PlaceSpec, moving: Option<TabId>) -> Result<PlaceSpec> {
    let current = k.model.current(caller);
    if spec.kind.unwrap_or(PlaceKind::Tiled) == PlaceKind::Tiled
        && let Some(pane) = spec.pane
    {
        match k.model.pane_id(caller, pane) {
            Ok((view, pane)) => {
                let full = spec.full.unwrap_or(false);
                let split = match spec.split {
                    None if !full && pane_tabs(k) => None,
                    split => Some(split.unwrap_or(Side::Down)),
                };
                let size = match split {
                    Some(side) => {
                        let area = k.view_area(view);
                        let rect = if full { Some(area) } else { k.model.views[&view].layout.rect_of(pane, area) };
                        share(&spec.size, side, rect)?
                    }
                    None => None,
                };
                return Ok(PlaceSpec {
                    kind: Some(PlaceKind::Tiled),
                    pane: Some(pane),
                    split,
                    size,
                    full: spec.full,
                    index: spec.index.filter(|_| split.is_none()),
                    focus: spec.focus,
                    ..PlaceSpec::default()
                });
            }
            Err(err) if spec.beside.is_none() && spec.view.is_none() => return Err(err),
            Err(_) => {}
        }
    }
    let session = |name: Option<String>| PlaceSpec {
        kind: Some(PlaceKind::Session),
        name,
        cwd: spec.cwd.clone(),
        focus: spec.focus,
        ..PlaceSpec::default()
    };
    match spec.kind.unwrap_or(PlaceKind::Tiled) {
        PlaceKind::Tiled => {
            let beside = match (spec.beside, spec.view) {
                (Some(beside), Some(view)) if k.model.tab_id(caller, beside).is_err() => {
                    match k.model.view_id(caller, view) {
                        Ok(view) => Some(k.model.views[&view].active).filter(|t| Some(*t) != moving),
                        Err(_) => {
                            let Some(current) = current else { return Ok(session(None)) };
                            let into = PlaceSpec {
                                kind: Some(PlaceKind::View),
                                session: Some(current.session),
                                ..Default::default()
                            };
                            return Ok(PlaceSpec { focus: spec.focus, ..into });
                        }
                    }
                }
                (Some(beside), _) => Some(k.model.tab_id(caller, beside)?),
                (None, Some(view)) => {
                    let view = &k.model.views[&k.model.view_id(caller, view)?];
                    Some(if Some(view.active) == moving {
                        view.tabs()
                            .into_iter()
                            .find(|t| Some(*t) != moving)
                            .ok_or_else(|| Error::invalid_params("the tab is alone in that view"))?
                    } else {
                        view.active
                    })
                }
                (None, None) => current.and_then(|c| {
                    if Some(c.tab) != moving {
                        return Some(c.tab);
                    }
                    let view = &k.model.views[&c.view];
                    view.last
                        .filter(|l| Some(*l) != moving)
                        .or_else(|| view.tabs().into_iter().find(|t| Some(*t) != moving))
                }),
            };
            let Some(beside) = beside else { return Ok(session(spec.name.clone())) };
            let size = match k.model.tabs.get(&beside).map(|t| &t.place) {
                Some(Place::Tiled { view }) => {
                    let area = k.view_area(*view);
                    let rect = match spec.full.unwrap_or(false) {
                        true => Some(area),
                        false => k.model.views[view].rect_of(beside, area),
                    };
                    share(&spec.size, spec.split.unwrap_or(Side::Down), rect)?
                }
                _ => spec.size.clone(),
            };
            Ok(PlaceSpec {
                kind: Some(PlaceKind::Tiled),
                beside: Some(beside),
                split: spec.split,
                size,
                full: spec.full,
                focus: spec.focus,
                ..PlaceSpec::default()
            })
        }
        PlaceKind::View => {
            let Some(target) =
                spec.session.map(|s| k.model.session_id(caller, s)).transpose()?.or(current.map(|c| c.session))
            else {
                return Ok(session(None));
            };
            Ok(PlaceSpec { kind: Some(PlaceKind::View), session: Some(target), ..spec.clone() })
        }
        PlaceKind::Session => Ok(session(spec.name.clone())),
        PlaceKind::Float => {
            let view = match spec.view {
                Some(view) => k.model.view_id(caller, view)?,
                None => current
                    .map(|c| c.view)
                    .ok_or_else(|| Error::invalid_params("a float needs a view, and there is none"))?,
            };
            for value in [&spec.row, &spec.col, &spec.width, &spec.height].into_iter().flatten() {
                Dim::parse(value)?;
            }
            let anchor = spec.anchor.clone().unwrap_or_else(|| {
                crate::kernel::layout::float::default_anchor(spec.row.is_some(), spec.col.is_some()).into()
            });
            match spec.relative.as_deref() {
                None | Some("view" | "client" | "pane" | "cursor" | "mouse") => {}
                Some(other) => {
                    return Err(Error::invalid_params(format!(
                        "a float is relative to `view`, `client`, `pane`, `cursor` or `mouse`: got {other:?}"
                    )));
                }
            }
            if spec.backdrop.is_some_and(|b| b > 100) {
                return Err(Error::invalid_params("a float's backdrop is 0 to 100"));
            }
            Ok(PlaceSpec {
                view: Some(view),
                anchor: Some(anchor),
                width: Some(spec.width.clone().unwrap_or_else(|| json!("50%"))),
                height: Some(spec.height.clone().unwrap_or_else(|| json!("50%"))),
                row: Some(spec.row.clone().unwrap_or_else(|| json!(0))),
                col: Some(spec.col.clone().unwrap_or_else(|| json!(0))),
                ..spec.clone()
            })
        }
    }
}
