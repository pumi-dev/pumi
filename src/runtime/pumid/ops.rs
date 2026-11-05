use super::wire::{self, num};
use crate::runtime::{Destination, Error, Made, Op, Result};
use serde_json::{Value, json};
pub trait Calls {
    fn call(&self, method: &str, params: Value) -> Result<Value>;
    fn state(&self) -> Result<Value>;
}
/// An id as the API spells it (`%3`), from the one the app's model has.
fn id(what: &str, s: &str) -> Result<String> {
    num(s).map(|n| format!("{}{n}", sigil(what))).ok_or_else(|| Error::Refused(format!("not a {what} id: {s}")))
}
fn sigil(what: &str) -> char {
    match what {
        "project" => '$',
        "view" => '@',
        _ => '%',
    }
}
fn terminal_args(cwd: Option<String>, env: Option<Value>) -> Value {
    let mut args = json!({});
    if let Some(cwd) = cwd {
        args["cwd"] = json!(cwd);
    }
    if let Some(env) = env.filter(Value::is_object) {
        args["env"] = env;
    }
    args
}
fn cmd(calls: &dyn Calls, name: &str, flags: Value, args: Vec<String>) -> Result<Value> {
    calls.call("cmd.run", json!([{ "cmd": name, "flags": flags, "args": args, "long": {} }, {}]))
}
pub fn made(calls: &dyn Calls, tab: &str) -> Made {
    let state = calls.state().unwrap_or_default();
    let read = wire::read(&state, &Default::default());
    let found = read.snapshot.tabs.iter().find(|t| t.tab_id == tab);
    Made {
        project: found.map(|t| t.project_id.clone()).filter(|s| !s.is_empty()),
        view: found.map(|t| t.view_id.clone()).filter(|s| !s.is_empty()),
        tabs: vec![tab.to_string()],
    }
}
pub fn apply(calls: &dyn Calls, op: Op) -> Result<Made> {
    let none = || Ok(Made::default());
    match op {
        Op::Split { target, axis, cwd, env } => {
            let place = json!({ "kind": "tiled", "split": wire::direction(axis), "beside": id("tab", &target)?, "focus": false });
            let tab = calls.call("tab.open", json!([null, terminal_args(cwd, env), place]))?;
            Ok(made(calls, tab.as_str().unwrap_or_default()))
        }
        Op::NewView { project, cwd, env } => {
            let place = json!({ "kind": "view", "project": id("project", &project)?, "focus": false });
            let tab = calls.call("tab.open", json!([null, terminal_args(cwd, env), place]))?;
            Ok(made(calls, tab.as_str().unwrap_or_default()))
        }
        Op::NewProject { path, env, .. } => {
            let place = json!({ "kind": "project", "path": path, "focus": false });
            let tab = calls.call("tab.open", json!([null, terminal_args(path, env), place]))?;
            Ok(made(calls, tab.as_str().unwrap_or_default()))
        }
        Op::OpenWorktree { path, .. } => apply(calls, Op::NewProject { path: Some(path), env: None, server: None }),
        Op::NewWorktree { .. } => Err(Error::Refused("worktrees are not in the server yet".into())),
        Op::MoveTab { tab, to } => {
            let place = match to {
                Destination::Beside { target, axis, .. } => {
                    json!({ "kind": "tiled", "split": wire::direction(axis), "beside": id("tab", &target)?, "focus": false })
                }
                Destination::NewView { project } => {
                    json!({ "kind": "view", "project": id("project", &project)?, "focus": false })
                }
                Destination::NewProject => json!({ "kind": "project", "focus": false }),
            };
            let tab = id("tab", &tab)?;
            calls.call("tab.update", json!([&tab, { "place": place }]))?;
            Ok(made(calls, &tab))
        }
        Op::CloseTab { tab } => {
            calls.call("tab.close", json!([id("tab", &tab)?]))?;
            none()
        }
        Op::CloseView { view } => {
            calls.call("view.close", json!([id("view", &view)?]))?;
            none()
        }
        Op::SwapTabs { a, b } => {
            cmd(calls, "swap-pane", json!({ "s": a, "t": b }), vec![])?;
            none()
        }
        Op::ClearTab { tab } => {
            cmd(calls, "send-keys", json!({ "R": true, "t": tab }), vec![])?;
            cmd(calls, "clear-history", json!({ "t": tab }), vec![])?;
            none()
        }
        Op::SendText { tab, text } => {
            calls.call("tab.call", json!([id("tab", &tab)?, "input", { "text": text }]))?;
            none()
        }
        Op::Relaunch { tab, line } => {
            let tab = id("tab", &tab)?;
            calls.call("option.set", json!(["restore_command", line, { "id": &tab }]))?;
            calls.call("tab.call", json!([&tab, "respawn", {}]))?;
            none()
        }
        Op::SendKeys { tab, keys } => {
            cmd(calls, "send-keys", json!({ "t": tab }), keys)?;
            none()
        }
        Op::RenameView { view, label } => {
            calls.call("view.update", json!([id("view", &view)?, { "name": label }]))?;
            none()
        }
        Op::RenameTab { tab, label } => {
            let tab = id("tab", &tab)?;
            calls.call("option.set", json!(["@label", label, { "id": &tab }]))?;
            none()
        }
        Op::SetRatio { view, path, ratio } => {
            let view = id("view", &view)?;
            let number = num(&view).unwrap_or(0);
            let state = calls.state()?;
            let found = state["views"]
                .as_array()
                .and_then(|views| views.iter().find(|v| v["id"] == number))
                .ok_or_else(|| Error::Refused(format!("no view {view}")))?;
            let layout = &found["layout"];
            let mut tree =
                wire::tree(layout, &found["panes"]).ok_or_else(|| Error::Refused(format!("a layout: {layout}")))?;
            if wire::set_ratio(&mut tree, &path, ratio) {
                calls.call("view.update", json!([&view, { "layout": wire::layout(&tree) }]))?;
            }
            none()
        }
    }
}
