use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Session,
    View,
    Tab,
    Client,
}
/// One object's options: the server's, a session's, a view's, a tab's or a client's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Key {
    Global,
    Session(u64),
    View(u64),
    Tab(u64),
    Client(u64),
}
impl Key {
    pub fn scope(self) -> Scope {
        match self {
            Key::Global => Scope::Global,
            Key::Session(_) => Scope::Session,
            Key::View(_) => Scope::View,
            Key::Tab(_) => Scope::Tab,
            Key::Client(_) => Scope::Client,
        }
    }
    pub fn id(self) -> Option<u64> {
        match self {
            Key::Global => None,
            Key::Session(id) | Key::View(id) | Key::Tab(id) | Key::Client(id) => Some(id),
        }
    }
}
/// An option's type: the scalar types, and `table` for structured values (`renderers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Boolean,
    Number,
    String,
    Table,
    Any,
}
/// A declared option: what `option.getInfo` describes and Settings shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Decl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub default: Value,
    /// The narrowest object it is set on. A tab option may also be set on a view, a session or
    /// globally, and a tab without its own value takes the nearest one.
    pub scope: Scope,
    pub desc: String,
    /// The values it accepts, when only some are allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Value>>,
    /// Set by the server, or by the type that owns it, never by a caller: `pane_current_path`,
    /// `pane_pid`, `pane_width`.
    #[serde(default)]
    pub readonly: bool,
    /// The blocks it belongs to; empty for every tab.
    #[serde(default, alias = "types", skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    /// Who declared it: `core`, a block, a plugin.
    pub source: String,
    /// The plugin that declared it, whose Settings page lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// Its title in Settings (`Font size`); without one, Settings makes one of its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What Settings calls each of `choices`, in their order (`Status bar` for `status_bar`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// A number's unit in Settings (`pt`, `%`, `s`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// What an empty text field shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// Its heading in Settings (`Terminal`, `Tabs`); without one, its plugin or `General`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// A number's range and step: a value past them is clamped, one between steps snapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// How Settings edits it when its type does not say: `toggle` (a number as on and off),
    /// `slider`, `json`, `color` (a `#rrggbb` string), or a control of the app's own (`theme`, `terminal_theme`, `font`,
    /// `keymap`, `agents`); a boolean is a switch, an option
    /// with choices a menu, a number a stepper, a string a text field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
    /// Declared by a plugin that is installed but not loaded: known from the cache of what it
    /// declared when it last loaded. Its values are kept for when it loads.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
    /// When it was declared, counting from 1: Settings lists a group in this order.
    #[serde(default)]
    pub order: u64,
}
impl Decl {
    pub fn new(name: &str, ty: Type, default: Value, scope: Scope, desc: &str) -> Decl {
        Decl {
            name: name.into(),
            ty,
            default,
            scope,
            desc: desc.into(),
            choices: None,
            readonly: false,
            blocks: Vec::new(),
            source: "core".into(),
            plugin: None,
            label: None,
            labels: None,
            unit: None,
            placeholder: None,
            group: None,
            min: None,
            max: None,
            step: None,
            control: None,
            pending: false,
            order: 0,
        }
    }
    pub fn group(mut self, group: &str) -> Decl {
        self.group = Some(group.into());
        self
    }
    /// A core plugin's, listed on its Settings page as a plugin's are (`context`).
    pub fn plugin(mut self, plugin: &str) -> Decl {
        self.plugin = Some(plugin.into());
        self
    }
    pub fn label(mut self, label: &str) -> Decl {
        self.label = Some(label.into());
        self
    }
    /// Its choices, each with what Settings calls it.
    pub fn labelled(mut self, choices: &[(&str, &str)]) -> Decl {
        self.choices = Some(choices.iter().map(|(c, _)| json!(c)).collect());
        self.labels = Some(choices.iter().map(|(_, l)| l.to_string()).collect());
        self
    }
    /// A number's unit in Settings (`pt`, `%`, `px`).
    pub fn unit(mut self, unit: &str) -> Decl {
        self.unit = Some(unit.into());
        self
    }
    pub fn placeholder(mut self, text: &str) -> Decl {
        self.placeholder = Some(text.into());
        self
    }
    pub fn range(mut self, min: f64, max: f64, step: Option<f64>) -> Decl {
        (self.min, self.max, self.step) = (Some(min), Some(max), step);
        self
    }
    pub fn control(mut self, control: &str) -> Decl {
        self.control = Some(control.into());
        self
    }
    pub fn readonly(mut self) -> Decl {
        self.readonly = true;
        self
    }
    pub fn choices(mut self, choices: &[&str]) -> Decl {
        self.choices = Some(choices.iter().map(|c| json!(c)).collect());
        self
    }
    pub fn of_block(mut self, block: &str, source: &str) -> Decl {
        self.blocks = vec![block.into()];
        self.source = source.into();
        self
    }
}
/// A value and who set it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Set {
    pub value: Value,
    pub source: String,
}
/// What a `set` changed, for `option.changed`.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub name: String,
    pub key: Key,
    pub old: Value,
    pub new: Value,
}
/// An option declared at `declared` may be set on objects at `at`.
pub fn reaches(declared: Scope, at: Scope) -> bool {
    match declared {
        Scope::Global => at == Scope::Global,
        Scope::Session => matches!(at, Scope::Global | Scope::Session),
        Scope::View => matches!(at, Scope::Global | Scope::Session | Scope::View),
        Scope::Tab => matches!(at, Scope::Global | Scope::Session | Scope::View | Scope::Tab),
        Scope::Client => matches!(at, Scope::Global | Scope::Client),
    }
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
}
pub fn check_type(decl: &Decl, value: &Value) -> Result<()> {
    let ok = match decl.ty {
        Type::Boolean => value.is_boolean(),
        Type::Number => value.is_number(),
        Type::String => value.is_string(),
        Type::Table => value.is_object() || value.is_array(),
        Type::Any => true,
    } || value.is_null();
    if !ok {
        return Err(Error::invalid_params(format!("`{}` is a {:?}, not {value}", decl.name, decl.ty).to_lowercase()));
    }
    if let Some(choices) = &decl.choices
        && !value.is_null()
        && !choices.contains(value)
    {
        return Err(Error::invalid_params(format!(
            "`{}` is one of {}, not {value}",
            decl.name,
            choices.iter().map(Value::to_string).collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(())
}
fn is_user(name: &str) -> bool {
    name.starts_with('@')
}
/// Every declaration, and every value set on an object.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Options {
    decls: BTreeMap<String, Decl>,
    /// No object has an empty map here, so two stores holding the same values are equal.
    #[serde(with = "by_object")]
    values: BTreeMap<Key, BTreeMap<String, Set>>,
}
/// The values as a list of `[key, {name: set}]`: a key is not a JSON object's key.
mod by_object {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        values: &BTreeMap<Key, BTreeMap<String, Set>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.collect_seq(values)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<Key, BTreeMap<String, Set>>, D::Error> {
        Ok(Vec::<(Key, BTreeMap<String, Set>)>::deserialize(d)?.into_iter().filter(|(_, v)| !v.is_empty()).collect())
    }
}
impl Options {
    /// A store that knows these declarations, in this order.
    pub fn new(decls: Vec<Decl>) -> Options {
        let mut options = Options::default();
        for (i, mut decl) in decls.into_iter().enumerate() {
            decl.order = i as u64 + 1;
            options.decls.insert(decl.name.clone(), decl);
        }
        options
    }
    /// The next declaration's `order`.
    fn next_order(&self) -> u64 {
        self.decls.values().map(|d| d.order).max().unwrap_or(0) + 1
    }
    /// Changes a declared option's default: one only known at start (`runtimepath`).
    pub fn set_default(&mut self, name: &str, value: Value) {
        if let Some(decl) = self.decls.get_mut(name) {
            decl.default = value;
        }
    }
    pub fn decl(&self, name: &str) -> Option<&Decl> {
        self.decls.get(name)
    }
    pub fn decls(&self) -> impl Iterator<Item = &Decl> {
        self.decls.values()
    }
    /// Declares an option. Declaring one that exists is allowed only as the same option: a second
    /// block may share `title`, but not change its type.
    pub fn declare(&mut self, mut decl: Decl) -> Result<()> {
        if is_user(&decl.name) || !valid_name(&decl.name) {
            return Err(Error::invalid_params(format!("`{}` is not an option name", decl.name)));
        }
        check_type(&decl, &decl.default)
            .map_err(|err| Error::invalid_params(format!("the default: {}", err.message)))?;
        decl.order = self.next_order();
        match self.decls.get_mut(&decl.name) {
            None => {
                self.decls.insert(decl.name.clone(), decl);
                Ok(())
            }
            Some(existing) if existing.pending || existing.source == decl.source => {
                *existing = decl;
                Ok(())
            }
            Some(existing) if existing.ty == decl.ty && existing.scope == decl.scope => {
                for block in decl.blocks {
                    if !existing.blocks.contains(&block) {
                        existing.blocks.push(block);
                    }
                }
                Ok(())
            }
            Some(existing) => Err(Error::invalid_params(format!(
                "`{}` is already declared by {} as a {:?} {:?} option",
                decl.name, existing.source, existing.scope, existing.ty
            ))),
        }
    }
    /// A declaration saved before a restart: kept until whoever made it declares it again,
    /// so the values set for it are readable meanwhile.
    pub fn restore_decl(&mut self, mut decl: Decl) {
        if self.decls.contains_key(&decl.name) {
            return;
        }
        decl.pending = true;
        decl.order = self.next_order();
        self.decls.insert(decl.name.clone(), decl);
    }
    /// What others declared (the core's own are in the binary), for persistence.
    pub fn foreign_decls(&self) -> Vec<Decl> {
        self.decls.values().filter(|d| d.source != "core").cloned().collect()
    }
    /// The value at the first of `chain` (narrowest first) that has one, else the default.
    /// The [`Set`] says who set it; `None` for a default.
    pub fn get(&self, name: &str, chain: &[Key]) -> Option<(Value, Option<&Set>)> {
        let decl = self.decls.get(name);
        if decl.is_none() && !is_user(name) {
            return None;
        }
        for key in chain {
            if let Some(decl) = decl
                && !reaches(decl.scope, key.scope())
            {
                continue;
            }
            if let Some(set) = self.values.get(key).and_then(|v| v.get(name)) {
                return Some((set.value.clone(), Some(set)));
            }
        }
        Some((decl.map_or(Value::Null, |decl| decl.default.clone()), None))
    }
    /// The value set on exactly this object, if any.
    pub fn get_local(&self, name: &str, key: Key) -> Option<&Set> {
        self.values.get(&key)?.get(name)
    }
    /// Sets a value; a caller cannot set a read-only one, the server (`internal`) can.
    /// Returns what changed, or `None` when the value was already this.
    pub fn set(&mut self, name: &str, key: Key, value: Value, source: &str, internal: bool) -> Result<Option<Change>> {
        match self.decls.get(name) {
            Some(decl) => {
                if decl.readonly && !internal {
                    return Err(Error::invalid_params(format!("`{name}` is read-only")));
                }
                if !reaches(decl.scope, key.scope()) {
                    return Err(Error::invalid_params(format!(
                        "`{name}` is a {:?} option and cannot be set on a {:?}",
                        decl.scope,
                        key.scope()
                    )));
                }
                check_type(decl, &value)?;
            }
            None if is_user(name) => {}
            None => return Err(Error::not_found(format!("no option `{name}`"))),
        }
        let old = self.get(name, &[key]).map(|(v, _)| v).unwrap_or(Value::Null);
        let values = self.values.entry(key).or_default();
        let unchanged = values.get(name).is_some_and(|set| set.value == value);
        values.insert(name.into(), Set { value: value.clone(), source: source.into() });
        Ok((!unchanged).then_some(Change { name: name.into(), key, old, new: value }))
    }
    /// Forgets the value set on this object, so it inherits again.
    pub fn unset(&mut self, name: &str, key: Key) -> Option<Change> {
        let old = self.forget(key, name)?;
        let new = self.get(name, &[key]).map(|(v, _)| v).unwrap_or(Value::Null);
        (old.value != new).then_some(Change { name: name.into(), key, old: old.value, new })
    }
    fn forget(&mut self, key: Key, name: &str) -> Option<Set> {
        let values = self.values.get_mut(&key)?;
        let old = values.remove(name);
        if values.is_empty() {
            self.values.remove(&key);
        }
        old
    }
    /// Everything set on one object.
    pub fn local(&self, key: Key) -> BTreeMap<String, Set> {
        self.values.get(&key).cloned().unwrap_or_default()
    }
    /// The object closed: its values go with it.
    pub fn drop_object(&mut self, key: Key) {
        self.values.remove(&key);
    }
    /// Every object that has values, in order.
    pub fn keys(&self) -> Vec<Key> {
        self.values.keys().copied().collect()
    }
    pub fn is_readonly(&self, name: &str) -> bool {
        self.decl(name).is_some_and(|d| d.readonly)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// A few of the core's options: one of each scope a test needs, and a read-only one.
    fn options() -> Options {
        Options::new(vec![
            Decl::new("default_size", Type::String, json!("80x24"), Scope::Global, ""),
            Decl::new("history_bytes", Type::Number, json!(4 << 20), Scope::Tab, ""),
            Decl::new("remain_on_exit", Type::Boolean, json!(false), Scope::Tab, ""),
            Decl::new("pane_pid", Type::Number, Value::Null, Scope::Tab, "").readonly(),
        ])
    }
    #[test]
    fn a_tab_takes_the_nearest_value() {
        let mut options = options();
        let chain = [Key::Tab(3), Key::View(2), Key::Session(1), Key::Global];
        assert_eq!(options.get("remain_on_exit", &chain).unwrap().0, json!(false));
        options.set("remain_on_exit", Key::Session(1), json!(true), "cli", false).unwrap();
        let (value, set) = options.get("remain_on_exit", &chain).unwrap();
        assert_eq!((value, set.unwrap().source.as_str()), (json!(true), "cli"));
        options.set("remain_on_exit", Key::Tab(3), json!(false), "init.lua:3", false).unwrap();
        assert_eq!(options.get("remain_on_exit", &chain).unwrap().0, json!(false));
    }
    #[test]
    fn user_options_need_no_declaration_and_others_do() {
        let mut options = options();
        let change = options.set("@build", Key::Global, json!("ok"), "cli", false).unwrap().unwrap();
        assert_eq!((change.old, change.new), (Value::Null, json!("ok")));
        assert!(options.set("@build", Key::Global, json!("ok"), "cli", false).unwrap().is_none(), "no change");
        assert_eq!(options.get("@build", &[Key::Tab(1), Key::Global]).unwrap().0, json!("ok"));
        assert_eq!(
            options.set("nonsense", Key::Global, json!(1), "cli", false).unwrap_err().kind,
            pumi_api::ErrorKind::NotFound
        );
        assert!(options.get("remain-on-exit", &[Key::Global]).is_none(), "names are stored as written");
    }
    #[test]
    fn types_choices_scopes_and_read_only_are_enforced() {
        let mut options = options();
        assert!(options.set("history_bytes", Key::Global, json!("lots"), "cli", false).is_err());
        assert!(options.set("history_bytes", Key::Global, json!("500"), "cli", false).is_err(), "the host coerces");
        assert_eq!(
            options.set("history_bytes", Key::Global, json!(500), "cli", false).unwrap().unwrap().new,
            json!(500)
        );
        assert!(options.set("pane_pid", Key::Tab(1), json!(4), "cli", false).is_err());
        assert!(options.set("pane_pid", Key::Tab(1), json!(4), "server", true).is_ok());
        assert!(options.set("default_size", Key::Tab(1), json!("10x10"), "cli", false).is_err(), "global only");
        options
            .declare(
                Decl::new("files.sort", Type::String, json!("name"), Scope::Global, "How files sort.")
                    .choices(&["name", "mtime"]),
            )
            .unwrap();
        assert!(options.set("files.sort", Key::Global, json!("size"), "cli", false).is_err());
        let other =
            Decl { source: "another".into(), ..Decl::new("files.sort", Type::Number, json!(1), Scope::Global, "") };
        assert!(options.declare(other).is_err(), "only its declarer changes it");
    }
    #[test]
    fn declarations_are_ordered_and_a_saved_one_gives_way() {
        let mut options = options();
        assert_eq!(options.decl("pane_pid").unwrap().order, 4);
        options.restore_decl(Decl {
            source: "pkg".into(),
            ..Decl::new("pkg.x", Type::Number, json!(1), Scope::Global, "")
        });
        assert!(options.decl("pkg.x").unwrap().pending);
        options
            .declare(Decl { source: "pkg".into(), ..Decl::new("pkg.x", Type::String, json!(""), Scope::Global, "") })
            .unwrap();
        let decl = options.decl("pkg.x").unwrap();
        assert_eq!((decl.pending, decl.ty, decl.order), (false, Type::String, 6));
        assert_eq!(options.foreign_decls().len(), 1);
    }
    #[test]
    fn an_object_whose_last_value_goes_is_not_kept() {
        let mut options = options();
        options.set("@x", Key::View(1), json!(1), "cli", false).unwrap();
        options.unset("@x", Key::View(1));
        assert_eq!(options, self::options(), "the same values, the same store");
        assert!(options.keys().is_empty());
    }
}
