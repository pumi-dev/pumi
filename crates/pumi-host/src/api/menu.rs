use super::kernel;
use crate::callback::{Call, ChanRef};
use crate::kernel::Kernel;
use crate::kernel::events::About;
use pumi_api::{Cx, Error, Nullable, Optional, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub name: String,
    pub priority: i64,
    /// A command line, run through `cmd.run` by whoever provides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rhs: Option<String>,
    /// Or a function the host calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<Call>,
    /// Or a channel's reference, sent `{path}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<ChanRef>,
    /// The shortcut it shows, as a key (`cmd+t`); the keymap decides what the key does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Drawn at the beginning of its row (a bar point's `start`): before the app's own items.
    #[serde(default)]
    pub start: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<Open>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Menu>,
}
/// What choosing an item opens instead of running anything: a component, as a tab of its own
/// for as long as it shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Open {
    /// Its type, `<plugin>.<component>`.
    pub component: String,
    /// What it is opened with (`open.args` in the component).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
    /// In pixels; 320 × 240 when left out. An overlay fills the window and ignores them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// `popover` (the default): under the item, gone at a click outside or `escape`;
    /// `overlay`: over the whole window, gone the same ways.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}
/// The tree's root.
#[derive(Default)]
pub struct Menus(pub Vec<Menu>);
/// `File.New\. View` as `["File", "New. View"]`.
pub fn parse_path(path: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => parts.last_mut().expect("one").extend(chars.next()),
            '.' => parts.push(String::new()),
            c => parts.last_mut().expect("one").push(c),
        }
    }
    parts.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}
fn menus(k: &mut Kernel) -> &mut Vec<Menu> {
    if k.ext.get::<Menus>().is_none() {
        k.ext.insert(Menus::default());
    }
    &mut k.ext.get_mut::<Menus>().expect("just inserted").0
}
fn find<'a>(mut list: &'a [Menu], path: &[String]) -> Option<&'a Menu> {
    let (last, parents) = path.split_last()?;
    for name in parents {
        list = &list.iter().find(|m| &m.name == name)?.children;
    }
    list.iter().find(|m| &m.name == last)
}
/// A menu item as `menu.set` takes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MenuItem {
    /// The function choosing it calls.
    #[serde(rename = "fn")]
    pub function: String,
    /// Its arguments, positional or by name; none by default.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub args: Value,
    /// What it says in the menu; made of the function's name by default.
    #[serde(default)]
    pub title: Optional<String>,
    /// When it shows; always without it.
    #[serde(default)]
    pub when: Optional<String>,
}
/// `registry.changed`: the app draws its menus again.
fn registry_changed(k: &mut Kernel, path: &str) {
    k.emit("registry.changed", "menu", About::default(), json!({ "kind": "menu", "names": [path] }));
}
/// Set a menu item: `path` names it and every menu above it, which are made as needed, and the
/// item is the function choosing it calls, with its title and when it shows. Setting a path
/// again replaces the item; `null` removes it, and a menu with everything in it.
#[api(
    name = "menu.set",
    example(args = r#"["File.Build", {"fn": "owner.name.build", "title": "Build"}]"#, result = "null"),
    example(args = r#"["File.Build", null]"#, result = "null", desc = "Removed.")
)]
pub fn pumi_menu_set(cx: &mut Cx, path: String, item: Nullable<MenuItem>) -> Result<()> {
    let parts = parse_path(&path);
    let (_, k) = kernel(cx)?;
    let Some((last, parents)) = parts.split_last() else {
        return Err(Error::invalid_params("a menu path names at least one menu"));
    };
    let Some(item) = item.into_option() else {
        let mut list = menus(k);
        for name in parents {
            list = &mut list
                .iter_mut()
                .find(|m| &m.name == name)
                .ok_or_else(|| Error::not_found(format!("no menu `{path}`")))?
                .children;
        }
        let before = list.len();
        list.retain(|m| m.name != *last);
        if list.len() == before {
            return Err(Error::not_found(format!("no menu `{path}`")));
        }
        registry_changed(k, &path);
        return Ok(());
    };
    let args = (!item.args.is_null()).then(|| item.args.clone());
    let menu = Menu {
        name: last.clone(),
        priority: 500,
        call: Some(Call { call: item.function, args }),
        desc: item.when.0,
        label: item.title.0,
        enabled: true,
        ..Default::default()
    };
    let mut list = menus(k);
    for name in parents {
        let at = match list.iter().position(|m| &m.name == name) {
            Some(at) => at,
            None => {
                list.push(Menu { name: name.clone(), enabled: true, priority: 500, ..Default::default() });
                list.sort_by_key(|m| m.priority);
                list.iter().position(|m| &m.name == name).expect("just added")
            }
        };
        list = &mut list[at].children;
    }
    match list.iter_mut().find(|m| m.name == *last) {
        Some(old) => {
            let children = std::mem::take(&mut old.children);
            *old = Menu { children, ..menu };
        }
        None => list.push(menu),
    }
    list.sort_by_key(|m| m.priority);
    registry_changed(k, &path);
    Ok(())
}
/// The menu tree under `path` (all of it for `None`), as `registry.list("menu")` lists it: each
/// item's name, what it calls, its keys, title and when it shows, and its children.
pub(crate) fn tree(k: &mut Kernel, path: Option<&str>) -> Result<Vec<Value>> {
    let parts = parse_path(path.unwrap_or(""));
    let list = menus(k);
    let found: Vec<&Menu> = if parts.is_empty() {
        list.iter().collect()
    } else {
        vec![find(list, &parts).ok_or_else(|| Error::not_found(format!("no menu `{}`", parts.join("."))))?]
    };
    Ok(found.into_iter().map(|m| serde_json::to_value(m).expect("plain data")).collect())
}
#[cfg(test)]
mod tests {
    use super::parse_path;
    #[test]
    fn paths_split_on_unescaped_dots() {
        assert_eq!(parse_path("File.New View"), ["File", "New View"]);
        assert_eq!(parse_path(r"Edit.Go to v1\.2"), ["Edit", "Go to v1.2"]);
        assert!(parse_path("").is_empty());
    }
}
