use crate::runtime::tree::{Axis, Leaf, Node, Pane};
use crate::runtime::{Read, model};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::collections::BTreeMap;
pub fn session_id(id: u64) -> String {
    format!("${id}")
}
pub fn view_id(id: u64) -> String {
    format!("@{id}")
}
pub fn tab_id(id: u64) -> String {
    format!("%{id}")
}
pub fn pane_id(id: u64) -> String {
    format!(":{id}")
}
#[cfg(test)]
pub fn num(id: &str) -> Option<u64> {
    id.trim_start_matches(['%', '@', '$', ':']).parse().ok()
}
#[cfg(test)]
pub fn layout(node: &Node) -> Value {
    // `view.update`'s shape: `{pane}` (its tabs stay as they are) or `{axis, ratio, first, second}`.
    match node {
        Node::Leaf(leaf) => json!({ "pane": leaf.pane().map(|p| &p.id) }),
        Node::Split { axis, ratio, first, second } => json!({
            "axis": if *axis == Axis::Row { "row" } else { "column" },
            "ratio": ratio,
            "first": layout(first),
            "second": layout(second),
        }),
    }
}
#[cfg(test)]
pub fn set_ratio(node: &mut Node, path: &[bool], to: f32) -> bool {
    match (node, path.split_first()) {
        (Node::Split { ratio, .. }, None) => {
            *ratio = to;
            true
        }
        (Node::Split { first, second, .. }, Some((side, rest))) => {
            set_ratio(if *side { second } else { first }, rest, to)
        }
        _ => false,
    }
}
#[cfg(test)]
pub fn direction(axis: Axis) -> &'static str {
    match axis {
        Axis::Row => "right",
        Axis::Column => "down",
    }
}
fn program_status(value: Option<&Value>) -> Vec<pumi_proto::status::Record> {
    value.and_then(|value| serde_json::from_value(value.clone()).ok()).unwrap_or_default()
}
fn u(v: &Value) -> Option<u64> {
    v.as_u64()
}
pub fn read(state: &Value, signatures: &BTreeMap<String, String>) -> Read {
    let empty = Vec::new();
    let list = |key: &str| state.get(key).and_then(Value::as_array).unwrap_or(&empty);
    let mut snapshot = model::Snapshot { version: Some(pumi_proto::PUMI_VERSION.into()), ..Default::default() };
    let mut view_session = BTreeMap::new();
    for s in list("sessions") {
        let Some(id) = s.get("id").and_then(u) else { continue };
        let views: Vec<(u64, u64)> = s
            .get("views")
            .and_then(Value::as_array)
            .map(|t| t.iter().filter_map(|p| Some((p.get(0)?.as_u64()?, p.get(1)?.as_u64()?))).collect())
            .unwrap_or_default();
        for (index, view) in &views {
            view_session.entry(*view).or_insert((id, *index));
        }
        snapshot.workspaces.push(model::Workspace {
            workspace_id: session_id(id),
            label: s.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            active_view_id: s.get("current").and_then(u).map(view_id),
            cwd: s.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_string),
            ..Default::default()
        });
    }
    let mut trees = BTreeMap::new();
    for v in list("views") {
        let Some(id) = v.get("id").and_then(u) else { continue };
        let Some((session, index)) = view_session.get(&id).copied() else { continue };
        snapshot.views.push(model::View {
            view_id: view_id(id),
            workspace_id: session_id(session),
            number: index as usize + 1,
            label: v.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()).map(str::to_string),
            worktree: state["options"][format!("view:{id}")]["worktree"]
                .as_str()
                .filter(|dir| !dir.is_empty())
                .map(str::to_string),
            ..Default::default()
        });
        let layout = v.get("layout").cloned().unwrap_or(Value::Null);
        let panes = v.get("panes").cloned().unwrap_or(Value::Null);
        // A tree is sent again when its panes split otherwise, or their tabs change.
        let signature = format!("{layout}{panes}");
        snapshot.layouts.push(model::ViewLayout {
            view_id: view_id(id),
            zoomed: v.get("zoomed").and_then(Value::as_bool).unwrap_or(false),
            focused_tab_id: v.get("active").and_then(u).map(tab_id),
            ..Default::default()
        });
        if signatures.get(&view_id(id)) != Some(&signature)
            && let Some(tree) = tree(&layout, &panes)
        {
            trees.insert(view_id(id), (signature, tree));
        }
    }
    for t in list("tabs") {
        let Some(id) = t.get("id").and_then(u) else { continue };
        let options = t.get("options").and_then(Value::as_object).cloned().unwrap_or_default();
        let place = t.get("place").cloned().unwrap_or(Value::Null);
        // A tiled tab is in its view's layout; a float is drawn over its view (the app's
        // `useFloats`). Either is one of the view's tabs; a hidden one is in no view.
        let tiled = place["kind"] == "tiled";
        let view = (tiled || place["kind"] == "float").then(|| place["view"].as_u64()).flatten();
        let text = |key: &str| options.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        let title = text("title").filter(|t| !t.trim().is_empty());
        let tab = model::Tab {
            tab_id: tab_id(id),
            block: t.get("block").and_then(Value::as_str).unwrap_or("terminal").to_string(),
            traits: t
                .get("traits")
                .and_then(Value::as_array)
                .map(|t| t.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            view_id: view.map(view_id).unwrap_or_default(),
            workspace_id: view.and_then(|t| view_session.get(&t)).map(|(s, _)| session_id(*s)).unwrap_or_default(),
            cwd: text("tab_current_path"),
            terminal_title: title.clone(),
            terminal_title_stripped: title.as_deref().map(strip_title).filter(|t| !t.is_empty()),
            label: text("@label"),
            agent: text("@agent"),
            status: text("@status").unwrap_or_default(),
            agent_session: text("@agent.session"),
            agent_model: text("@agent.model"),
            agent_transcript: text("@agent.transcript"),
            agent_conversation: text("@agent.conversation"),
            program_status: program_status(options.get("@program_status")),
            place,
            options,
            ..Default::default()
        };
        if view.is_some() {
            if tiled && let Some(layout) = snapshot.layouts.iter_mut().find(|l| l.view_id == tab.view_id) {
                let n = |k: &str| tab.options.get(k).and_then(Value::as_i64).unwrap_or(0);
                layout.tabs.push(model::LayoutTab {
                    tab_id: tab.tab_id.clone(),
                    rect: model::Rect {
                        x: n("pane_left") as i32,
                        y: n("pane_top") as i32,
                        width: n("pane_width") as u32,
                        height: n("pane_height") as u32,
                    },
                });
            }
            snapshot.tabs.push(tab);
        } else {
            snapshot.others.push(tab);
        }
    }
    let options = state["options"]["global"].as_object().cloned();
    Read { snapshot, trees, menus: None, keys: None, bindings: None, options }
}
fn strip_title(title: &str) -> String {
    title
        .trim_start_matches(|c: char| !c.is_alphanumeric() && !c.is_ascii_punctuation() || c.is_whitespace())
        .trim()
        .to_string()
}
/// A view's layout in the core's shape (`["row", [["leaf", 4, 0.5], …]]`), its leaves the panes
/// of `panes` (the view's, by number) with their tabs.
pub fn tree(v: &Value, panes: &Value) -> Option<Node> {
    let list = v.as_array()?;
    match list.first()?.as_str()? {
        "leaf" => {
            let id = list.get(1)?.as_u64()?;
            let pane = &panes[id.to_string()];
            let tabs =
                pane["tabs"].as_array().map(|t| t.iter().filter_map(u).map(tab_id).collect()).unwrap_or_default();
            Some(Node::Leaf(Leaf::Pane(Pane { id: pane_id(id), tabs, active: pane["active"].as_u64().map(tab_id) })))
        }
        kind @ ("row" | "col") => {
            let axis = if kind == "row" { Axis::Row } else { Axis::Column };
            let children = list.get(1)?.as_array()?;
            let sized: Vec<(f32, Node)> = children
                .iter()
                .map(|c| Some((c.get(2).and_then(Value::as_f64).unwrap_or(1.0) as f32, tree(c, panes)?)))
                .collect::<Option<_>>()?;
            fold(axis, sized)
        }
        _ => None,
    }
}
fn fold(axis: Axis, mut sized: Vec<(f32, Node)>) -> Option<Node> {
    if sized.len() <= 1 {
        return sized.pop().map(|(_, n)| n);
    }
    let total: f32 = sized.iter().map(|(s, _)| s.max(0.0)).sum::<f32>().max(f32::EPSILON);
    let (size, first) = sized.remove(0);
    let ratio = (size.max(0.0) / total).clamp(0.05, 0.95);
    Some(Node::split(axis, ratio, first, fold(axis, sized)?))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn a_state_reads_into_the_model() {
        let state = json!({
            "sessions": [{ "id": 1, "name": "main", "cwd": "/src/pumi", "views": [[0, 1]], "current": 1 }],
            "views": [{
                "id": 1,
                "layout": ["row", [["leaf", 1, 0.5], ["leaf", 2, 0.5]]],
                "panes": { "1": { "id": 1, "tabs": [1], "active": 1 }, "2": { "id": 2, "tabs": [2], "active": 2 } },
                "active": 2,
                "zoomed": false,
            }],
            "options": { "view:1": { "worktree": "/src/pumi.worktrees/avid-storm" } },
            "tabs": [
                { "id": 1, "block": "terminal", "place": { "kind": "tiled", "view": 1 }, "options": { "tab_current_path": "/tmp", "title": "⠋ π - pumi" } },
                { "id": 2, "block": "pumi_dev.hello.counter", "place": { "kind": "tiled", "view": 1 }, "options": { "@agent": "codex", "@agent.model": "gpt-5", "@agent.transcript": "/tmp/t.jsonl", "@agent.conversation": "pumi_dev/acp" } },
                { "id": 3, "block": "pumi_dev.hello.counter", "place": { "kind": "float", "view": 1, "row": 0, "col": 0, "width": 20, "height": 5 }, "options": {} },
            ],
        });
        let read = read(&state, &BTreeMap::new());
        assert_eq!(read.snapshot.workspaces[0].workspace_id, "$1");
        assert_eq!(read.snapshot.workspaces[0].cwd.as_deref(), Some("/src/pumi"), "the folder it was opened at");
        assert_eq!(read.snapshot.views[0].workspace_id, "$1");
        assert_eq!(read.snapshot.tabs.len(), 3, "a float is one of its view's tabs");
        assert_eq!(read.snapshot.tabs[0].terminal_title.as_deref(), Some("⠋ π - pumi"));
        assert_eq!(read.snapshot.tabs[0].terminal_title_stripped.as_deref(), Some("π - pumi"));
        assert_eq!(read.snapshot.views[0].worktree.as_deref(), Some("/src/pumi.worktrees/avid-storm"));
        assert_eq!(read.snapshot.tabs[1].block, "pumi_dev.hello.counter");
        let agent = &read.snapshot.tabs[1];
        assert_eq!(agent.agent.as_deref(), Some("codex"), "a plugin's tab is an agent as a terminal is");
        assert_eq!(
            (agent.agent_model.as_deref(), agent.agent_transcript.as_deref()),
            (Some("gpt-5"), Some("/tmp/t.jsonl"))
        );
        assert_eq!(agent.agent_conversation.as_deref(), Some("pumi_dev/acp"));
        assert_eq!(read.snapshot.tabs[0].agent_session, None);
        let float = &read.snapshot.tabs[2];
        assert_eq!((float.tab_id.as_str(), float.view_id.as_str(), float.workspace_id.as_str()), ("%3", "@1", "$1"));
        assert_eq!(float.place["kind"], "float");
        assert!(read.snapshot.others.is_empty());
        let rects: Vec<&str> = read.snapshot.layouts[0].tabs.iter().map(|t| t.tab_id.as_str()).collect();
        assert_eq!(rects, ["%1", "%2"], "a float has no place in the layout");
        assert!(read.trees.contains_key("@1"));
        let Node::Split { second, .. } = &read.trees["@1"].1 else { panic!("a split") };
        let Node::Leaf(Leaf::Pane(pane)) = second.as_ref() else { panic!("a pane") };
        assert_eq!((pane.id.as_str(), pane.tabs.clone()), (":2", vec!["%2".to_string()]));
    }
}
