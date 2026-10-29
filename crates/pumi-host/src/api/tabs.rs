use super::kernel;
use super::select::Zoom;
use crate::kernel::layout::{PlaceSpec, Side};
use crate::kernel::objects::{TabPlace, patch_field};
use crate::kernel::options::Key;
use pumi_api::{Cx, Error, Result, TabRef, api};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
/// Open a tab holding `block` (`null`: what a new tab holds, the block the `new_tab` option names;
/// refused while it names none, as pumi has no block of its own for one), placed as `place` says:
/// in a pane split from a tab's, into a pane
/// as one of its tabs (`{"pane": ":2"}`), in a new view or session, floating; or in an open tab
/// instead of its block (`{"kind": "tiled", "replace": "%1"}`), which
/// keeps its id, place, focus and options (`tab.changed {block}`). It is the one way anything
/// appears. `args` are the block's (a terminal block's `command`, `cwd` and `env`; a file's `path`;
/// a page's `url`), or `{"state": …}` to restore a saved state into the tab. Returns the tab's id.
#[api(
    name = "tab.open",
    example(
        args = r#"["pumi_dev.terminal.shell", {"command": ["make", "test"]}, {"kind": "tiled", "split": "right"}]"#,
        result = r#""%3""#,
        desc = "A terminal running the tests, to the right of the caller's tab."
    ),
    example(
        args = r#"["file", {"path": "README.md"}, {"kind": "tiled", "pane": ":2"}]"#,
        result = r#""%7""#,
        desc = "A file as a tab of pane :2, in front of it."
    ),
    example(
        args = r#"["pumi_dev.browser.page", {"url": "https://example.com"}, {"kind": "view"}]"#,
        result = r#""%4""#,
        desc = "A page in a new view."
    ),
    example(
        args = r#"[null, {}, {"kind": "tiled", "session": "new", "name": "work", "cwd": "/src"}]"#,
        result = r#""%5""#,
        desc = "A new session in /src, holding what a new tab does."
    ),
    example(
        args = r#"["pumi_dev.terminal.shell", {"command": ["vim"]}, {"kind": "float", "width": "80%", "height": "80%"}]"#,
        result = r#""%6""#,
        desc = "A floating terminal."
    ),
    example(
        args = r#"["pumi_dev.diagram.view", {"path": "flow.mmd"}, {"kind": "tiled", "replace": "%1"}]"#,
        result = r#""%1""#,
        desc = "A diagram in tab %1, instead of what it held: the same tab, in the same place."
    ),
    note = "Without a place, the tab is tiled in a pane below the caller's tab's.",
    note = "Into a pane, it goes after the tab in front (or at `index` of its strip) and comes to the front, unless `focus` is `false`. With `pane_tabs` off, it splits the pane instead.",
    note = "A float's `width` and `height` are cells, `\"480px\"` or a share of what it floats over (`\"50%\"`, the default), kept as given so a share stays one as the window changes size.",
    note = "A float is pinned by `anchor` (`NW` … `SE`, `center`), `row` and `col` in from it; without an anchor it is centred on each axis it was given no offset for."
)]
pub fn pumi_tab_open(
    cx: &mut Cx,
    block: Option<String>,
    args: Option<Value>,
    place: Option<TabPlace>,
) -> Result<TabRef> {
    let (caller, k) = kernel(cx)?;
    let block = match block {
        Some(block) => block,
        None => new_tab(k)?,
    };
    let place = match place {
        Some(place) => place.spec()?,
        None => PlaceSpec::default(),
    };
    let place = match place.replace {
        Some(tab) => PlaceSpec { replace: Some(k.model.tab_id(&caller, tab)?), ..PlaceSpec::default() },
        None => crate::place::resolve(k, &caller, &place, None)?,
    };
    let tab = k.call_core(&caller, "core.openTab", json!([block, args, place]))?;
    tab.as_u64().map(TabRef).ok_or_else(|| Error::exception(format!("the core opened {tab}, not a tab")))
}
/// What a refusal to open a new tab says while `new_tab` names no block: the app knows it by this.
pub const NO_NEW_TAB: &str = "no plugin opens new tabs";
/// The block a new tab holds: the `new_tab` option's. Nothing is opened while it names none.
pub fn new_tab(k: &crate::kernel::Kernel) -> Result<String> {
    let named = k.global_option("new_tab");
    match named.as_str().filter(|b| !b.is_empty()) {
        Some(block) => Ok(block.to_string()),
        None => Err(Error::invalid_params(format!(
            "{NO_NEW_TAB}: install one (Terminal, Launcher, Browser, Agents: `pumi plugin add pumi-dev/plugins/terminal`) and pick it in Settings ▸ General ▸ New tabs open (the `new_tab` option)"
        ))),
    }
}
/// How far a tab's border moves (`tab.update`'s `size`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabSize {
    /// The side whose border moves.
    pub direction: Side,
    /// By how many cells: its neighbour there gives or takes them.
    pub amount: i64,
}
/// A patch to a tab: what `tab.update` changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabPatch {
    /// Its title; `null` leaves it to what its program says next.
    #[serde(default, deserialize_with = "patch_field", skip_serializing_if = "Option::is_none")]
    pub title: Option<Option<String>>,
    /// Where it goes: into a pane (`{"kind": "tiled", "pane": ":2"}`, or along its own pane's
    /// strip), in a pane split from another tab's, into a new view or session, floating, or into
    /// another tab's place (`{"kind": "tiled", "swap": "%2"}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<TabPlace>,
    /// Grow or shrink its pane from a side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<TabSize>,
    /// Zoom its pane to fill its view (`true`), unzoom its view (`false`), or flip it (`"toggle"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Zoom>,
    /// The user has seen it: what its program left for them (`done`, `error`, OSC 7501) goes, so
    /// its `@status` stops asking. The app says so when the tab is in front of the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<bool>,
    /// Options to set on it; `null` unsets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Map<String, Value>>,
}
/// Change a tab: its title, place, size, zoom, whether it was seen, and its options. Each change
/// fires `tab.changed` (or its view's `view.changed`) with what changed. No id is the caller's tab.
#[api(
    name = "tab.update",
    example(args = r#"["%3", {"title": "tests"}]"#, result = "null"),
    example(
        args = r#"["%3", {"place": {"kind": "view", "name": "logs"}}]"#,
        result = "null",
        desc = "Into a view of its own, as `break-pane`."
    ),
    example(
        args = r#"["%3", {"place": {"kind": "tiled", "pane": ":2", "index": 0}}]"#,
        result = "null",
        desc = "To the start of pane :2's strip."
    ),
    example(args = r#"["%3", {"size": {"direction": "left", "amount": 5}}]"#, result = "null"),
    example(args = r#"["%3", {"zoom": "toggle"}]"#, result = "null", desc = "Zoom it, or unzoom its view.")
)]
pub fn pumi_tab_update(cx: &mut Cx, tab: Option<TabRef>, patch: TabPatch) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_ref(&caller, tab)?;
    if let Some(title) = patch.title {
        k.call_core(&caller, "core.tabSetTitle", json!([tab, title]))?;
    }
    if let Some(options) = patch.options {
        let source = k.source(&caller);
        for (name, value) in options {
            k.set_option(&name, Key::Tab(tab), value, &source, false)?;
        }
    }
    if patch.seen == Some(true) {
        crate::program::seen(k, tab);
    }
    if let Some(place) = patch.place {
        let place = place.spec()?;
        let place = match place.swap {
            Some(other) => PlaceSpec { swap: Some(k.model.tab_id(&caller, other)?), ..PlaceSpec::default() },
            None => crate::place::resolve(k, &caller, &place, Some(tab))?,
        };
        k.call_core(&caller, "core.tabSetPlace", json!([tab, place]))?;
    }
    if let Some(size) = patch.size {
        super::select::resize(cx, tab, size.direction, size.amount)?;
    }
    if let Some(zoom) = patch.zoom {
        super::select::set_zoom(cx, tab, zoom)?;
    }
    Ok(())
}
