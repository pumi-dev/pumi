use crate::kernel::{Kernel, SERVER};
use pumi_api::{Caller, Cx, Registry, Result};
use serde_json::{Map, Value, json};
#[derive(Clone, Debug, Default)]
pub struct Startup {
    pub clean: bool,
    pub none: bool,
}
pub const SOURCE: &str = "pumi";
pub fn start(registry: &Registry, kernel: &mut Kernel, startup: &Startup) {
    let me = Caller { channel: SERVER, source: Some(SOURCE.into()), ..Caller::default() };
    let result = registry.with_cx(kernel, &me, |cx| -> Result<()> {
        if !startup.none {
            keys(cx);
            menus(cx);
        }
        if !startup.clean && !startup.none {
            crate::overlay::apply(cx);
            crate::contributions::follow(cx);
        }
        Ok(())
    });
    if let Err(err) = result {
        crate::server::log(&format!("defaults: {}", err.message));
    }
}
/// A key's or a menu item's: the function it calls and its arguments.
fn call(name: &str, args: Value) -> Value {
    json!({ "call": name, "args": args })
}
fn split(side: &str) -> Value {
    call("tab.open", json!(["terminal", {}, { "kind": "tiled", "split": side }]))
}
fn layout(name: &str) -> Value {
    call("view.update", json!([null, { "layout": name }]))
}
/// The `prefix` key table: the server's own bindings, added to what the option holds already.
fn keys(cx: &mut Cx) {
    let mut prefix: Vec<(String, Value)> = vec![
        ("\"".into(), split("down")),
        ("%".into(), split("right")),
        ("c".into(), call("tab.open", json!(["terminal", {}, { "kind": "view" }]))),
        ("!".into(), call("tab.update", json!([null, { "place": { "kind": "view" } }]))),
        ("n".into(), call("view.focus", json!([null, { "offset": 1 }]))),
        ("p".into(), call("view.focus", json!([null, { "offset": -1 }]))),
        ("l".into(), call("view.focus", json!([null, { "last": true }]))),
        (")".into(), call("session.focus", json!([null, { "offset": 1 }]))),
        ("(".into(), call("session.focus", json!([null, { "offset": -1 }]))),
        ("L".into(), call("session.focus", json!([null, { "last": true }]))),
        (";".into(), call("pane.focus", json!([null, { "last": true }]))),
        ("o".into(), call("pane.focus", json!([null, { "offset": 1 }]))),
        ("Up".into(), call("pane.focus", json!([null, { "direction": "up" }]))),
        ("Down".into(), call("pane.focus", json!([null, { "direction": "down" }]))),
        ("Left".into(), call("pane.focus", json!([null, { "direction": "left" }]))),
        ("Right".into(), call("pane.focus", json!([null, { "direction": "right" }]))),
        ("z".into(), call("pane.update", json!([null, { "zoom": "toggle" }]))),
        ("x".into(), call("tab.close", json!([]))),
        ("Space".into(), call("view.update", json!([null, { "layout": { "offset": 1 } }]))),
        ("M-1".into(), layout("even-horizontal")),
        ("M-2".into(), layout("even-vertical")),
        ("M-3".into(), layout("main-horizontal")),
        ("M-4".into(), layout("main-vertical")),
        ("M-5".into(), layout("tiled")),
    ];
    for i in 0..10 {
        prefix.push((i.to_string(), call("view.focus", json!([null, { "index": i }]))));
    }
    for (key, dir) in [("Up", "up"), ("Down", "down"), ("Left", "left"), ("Right", "right")] {
        for (m, cells) in [("C", 1), ("M", 5)] {
            prefix.push((
                format!("{m}-{key}"),
                call("tab.update", json!([null, { "size": { "direction": dir, "amount": cells } }])),
            ));
        }
    }
    // The key tables are the `keys` option: its `prefix` table, as it is now, with these added.
    let mut tables = match cx.call("option.get", json!(["keys", { "scope": "global" }])) {
        Ok(Value::Object(tables)) => tables,
        _ => Map::new(),
    };
    let mut table = match tables.remove("prefix") {
        Some(Value::Object(table)) => table,
        _ => Map::new(),
    };
    for (key, rhs) in prefix {
        table.insert(key, rhs);
    }
    tables.insert("prefix".into(), Value::Object(table));
    if let Err(err) = cx.call("option.set", json!(["keys", Value::Object(tables), { "scope": "global" }])) {
        crate::server::log(&format!("default keys: {}", err.message));
    }
}
/// The menus' items, each the function choosing it calls: `menu.set` takes a path and one.
fn menus(cx: &mut Cx) {
    let items: Vec<(&str, Value)> = vec![
        ("Session.New Session", call("tab.open", json!(["terminal", {}, { "kind": "session", "focus": false }]))),
        ("Session.Next Session", call("session.focus", json!([null, { "offset": 1 }]))),
        ("Session.Previous Session", call("session.focus", json!([null, { "offset": -1 }]))),
        ("Session.New View", call("tab.open", json!(["terminal", {}, { "kind": "view" }]))),
        ("Session.New Tab", call("tab.open", json!(["terminal", {}, { "kind": "tiled", "pane": "current" }]))),
        ("Session.Split Right", split("right")),
        ("Session.Split Down", split("down")),
        ("Session.Zoom Pane", call("pane.update", json!([null, { "zoom": "toggle" }]))),
        ("Session.Layout.Even Horizontal", layout("even-horizontal")),
        ("Session.Layout.Even Vertical", layout("even-vertical")),
        ("Session.Layout.Main Horizontal", layout("main-horizontal")),
        ("Session.Layout.Main Vertical", layout("main-vertical")),
        ("Session.Layout.Tiled", layout("tiled")),
        ("Session.Close Tab", call("tab.close", json!([]))),
    ];
    for (path, rhs) in items {
        let item = json!({ "fn": rhs["call"], "args": rhs["args"] });
        if let Err(err) = cx.call("menu.set", json!([path, item])) {
            crate::server::log(&format!("default menu {path}: {}", err.message));
        }
    }
}
