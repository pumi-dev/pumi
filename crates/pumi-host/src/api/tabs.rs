use super::kernel;
use super::select::Zoom;
use crate::kernel::layout::{PlaceSpec, Side};
use crate::kernel::objects::{TabPlace, patch_field};
use crate::kernel::options::Key;
use pumi_api::{Cx, Error, Result, TabRef, api};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
/// Open a tab holding `block` (`null`: what a new tab holds, what the `new_tab` option names, a
/// block or a plugin's `tab.new` action; while it names nothing installed, the Launcher's list, a
/// shell or the first thing installed that starts one; refused while nothing is, as pumi has no
/// block of its own for one), placed as `place` says:
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
        None => match new_tab(k, &caller)? {
            NewTab::Block(block) => block,
            NewTab::Action(function) => return start_action(k, &caller, &function, args, place),
        },
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
/// What a refusal to open a new tab says while nothing installed can start one: the app knows it by this.
pub const NO_NEW_TAB: &str = "no plugin opens new tabs";
/// The blocks a new tab holds, in this order, while `new_tab` names nothing installed: the
/// Launcher's list, then a shell.
const PREFERRED: [&str; 2] = ["pumi_dev.launcher.new", "pumi_dev.terminal.shell"];
/// What a new tab starts: a block, or a plugin's `tab.new` action, by the function it calls.
#[derive(Debug, PartialEq)]
pub enum NewTab {
    Block(String),
    Action(String),
}
/// One thing a new tab can start, as the `+` menu lists them (the app's `startable.ts`): each
/// enabled plugin's `tab.new` actions, or, for a plugin with none, each block it declares.
fn startables() -> Vec<NewTab> {
    let mut out = Vec::new();
    for p in crate::contributions::plugins() {
        let c = &p.manifest.contributes;
        let actions = c.slots.get("tab.new").map(|s| s.actions.as_slice()).unwrap_or_default();
        if actions.is_empty() {
            out.extend(c.blocks.iter().map(|b| NewTab::Block(p.prefixed(&b.id))));
        } else {
            // One that asks for files first can't start on its own.
            out.extend(actions.iter().filter(|a| a.pick.is_none()).map(|a| NewTab::Action(p.prefixed(&a.command))));
        }
    }
    out
}
/// What `named` (the `new_tab` option) starts, of `startables` and the blocks the core has: an
/// action it names, else a block it names, else the first of `PREFERRED` there is, else the
/// first startable. `None` while nothing is installed that starts one.
pub fn resolve_new_tab(named: &str, startables: Vec<NewTab>, blocks: &[String]) -> Option<NewTab> {
    let has = |id: &str| blocks.iter().any(|b| b == id);
    if !named.is_empty() {
        if startables.iter().any(|s| matches!(s, NewTab::Action(f) if f == named)) {
            return Some(NewTab::Action(named.to_string()));
        }
        if has(named) {
            return Some(NewTab::Block(named.to_string()));
        }
    }
    if let Some(block) = PREFERRED.iter().find(|b| has(b)) {
        return Some(NewTab::Block(block.to_string()));
    }
    startables.into_iter().find(|s| match s {
        NewTab::Block(b) => has(b),
        NewTab::Action(_) => true,
    })
}
/// What a new tab starts: what the `new_tab` option names, while it is installed; else the
/// Launcher's list, a shell, or the first thing installed that starts one. Refused while nothing does.
pub fn new_tab(k: &mut crate::kernel::Kernel, caller: &pumi_api::Caller) -> Result<NewTab> {
    let named = k.global_option("new_tab");
    let listed = k.call_core(caller, "core.blocks", json!([]))?;
    let blocks: Vec<String> =
        listed.as_array().into_iter().flatten().filter_map(|b| b.get("id")?.as_str().map(str::to_string)).collect();
    resolve_new_tab(named.as_str().unwrap_or(""), startables(), &blocks).ok_or_else(|| {
        Error::invalid_params(format!(
            "{NO_NEW_TAB}: install one (Terminal, Launcher, Browser, Agents: `pumi plugin add pumi-dev/plugins/terminal`); Settings ▸ General ▸ New tabs open (the `new_tab` option) picks among them"
        ))
    })
}
/// Starts a new tab with a plugin's `tab.new` action: its `function`, given what the `+` menu
/// gives it (`{tab, place}`, the tab it goes beside and where) and `args`. The place is made
/// concrete first (`current` is the caller's pane, none is below the caller's tab), as the plugin
/// opens the tab as itself. Answers the tab the function opened.
fn start_action(
    k: &mut crate::kernel::Kernel,
    caller: &pumi_api::Caller,
    function: &str,
    args: Option<Value>,
    place: Option<TabPlace>,
) -> Result<TabRef> {
    let tab = k.model.current(caller).map(|c| TabRef(c.tab));
    let place = match place {
        Some(place) => {
            let mut place = serde_json::to_value(&place).map_err(|e| Error::exception(e.to_string()))?;
            if place.get("pane").and_then(Value::as_str) == Some("current") {
                let (_, pane) = k.model.pane_id(caller, 0)?;
                place["pane"] = json!(pumi_api::PaneRef(pane));
            }
            place
        }
        None => match tab {
            Some(tab) => json!({ "kind": "tiled", "beside": tab }),
            None => json!({ "kind": "tiled" }),
        },
    };
    let mut given = match args {
        Some(Value::Object(args)) => args,
        _ => Map::new(),
    };
    if let Some(tab) = tab {
        given.insert("tab".into(), json!(tab));
    }
    given.insert("place".into(), place);
    let opened = k.call_core(caller, function, json!([given]))?;
    match &opened {
        Value::Number(n) => n.as_u64().map(TabRef),
        _ => serde_json::from_value::<TabRef>(opened.clone()).ok(),
    }
    .ok_or_else(|| Error::exception(format!("{function} opened {opened}, not a tab")))
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

#[cfg(test)]
mod tests {
    use super::*;
    fn block(id: &str) -> NewTab {
        NewTab::Block(id.into())
    }
    fn action(f: &str) -> NewTab {
        NewTab::Action(f.into())
    }
    fn list() -> Vec<NewTab> {
        vec![
            action("pumi_dev.agents.new_pi"),
            action("pumi_dev.agents.new_claude"),
            block("amresyd.film.editor"),
            action("pumi_dev.terminal.open"),
        ]
    }
    fn blocks(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn a_new_tab_starts_what_new_tab_names_while_it_is_installed() {
        let all = blocks(&["amresyd.film.editor", "pumi_dev.terminal.shell", "pumi_dev.agents.term"]);
        assert_eq!(
            resolve_new_tab("pumi_dev.agents.new_claude", list(), &all),
            Some(action("pumi_dev.agents.new_claude"))
        );
        assert_eq!(resolve_new_tab("pumi_dev.agents.term", list(), &all), Some(block("pumi_dev.agents.term")));
        assert_eq!(resolve_new_tab("amresyd.film.editor", list(), &all), Some(block("amresyd.film.editor")));
    }
    #[test]
    fn with_nothing_named_a_new_tab_is_the_launcher_then_a_shell_then_the_first() {
        let with_launcher = blocks(&["pumi_dev.launcher.new", "pumi_dev.terminal.shell"]);
        assert_eq!(resolve_new_tab("", list(), &with_launcher), Some(block("pumi_dev.launcher.new")));
        let shell = blocks(&["pumi_dev.terminal.shell"]);
        assert_eq!(resolve_new_tab("gone.plugin.block", list(), &shell), Some(block("pumi_dev.terminal.shell")));
        assert_eq!(resolve_new_tab("", list(), &[]), Some(action("pumi_dev.agents.new_pi")));
        assert_eq!(
            resolve_new_tab("", vec![block("amresyd.film.editor")], &blocks(&["amresyd.film.editor"])),
            Some(block("amresyd.film.editor"))
        );
        assert_eq!(resolve_new_tab("", vec![], &blocks(&["test.term.shell"])), None);
    }
}
