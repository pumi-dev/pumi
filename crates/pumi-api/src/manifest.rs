//! A plugin's manifest: the `pumi` key of its `package.json`, read without running it
//! (api/plugin-manifest.schema.json). What it contributes (options, commands, keys, blocks, renderers, slots) is
//! registered from here, so Settings, the palette and keys work before its worker starts; its
//! `activation` says when the worker does.
//!
//! A plugin's id is `owner/name` and its prefix `owner.name.` (`-` as `_`, so a prefixed name is
//! a function name): the owner is the GitHub owner of its source, lowercased with `-` as `_`, and a
//! linked local plugin's is `pumi.owner`, or `local`.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
/// The owner of a plugin that is neither from GitHub nor names one.
pub const LOCAL: &str = "local";
/// The keychain service a plugin's `secret` options are kept under (`secret.get`,
/// `secret.set`); each is the account named as the option is (`owner.name.key`). The SDK reads
/// and writes them through `option.get` and `option.set`, and `plugin.del` with `data` forgets
/// them.
pub const SECRET_SERVICE: &str = "pumi";
/// A plugin's own directory under one of pumi's (`stdpath('config')`, `state`, `cache`):
/// `<root>/plugins/<owner.name>`. The SDK's `paths` are these, and the server keeps a plugin's
/// `storage` in its `state` one.
pub fn data_dir(root: &Path, id: &str) -> PathBuf {
    root.join("plugins").join(id.replace('/', "."))
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub api: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_pumi: Option<String>,
    #[serde(default)]
    pub isolate: bool,
    #[serde(default)]
    pub uses: Vec<Use>,
    #[serde(default)]
    pub themes: Vec<String>,
    #[serde(default)]
    pub images: Vec<Image>,
    /// Its icon in the store: a square image in the plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub contributes: Contributes,
    #[serde(default)]
    pub activation: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Use {
    Network,
    Processes,
    Files,
    Addons,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub path: String,
    pub alt: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    #[serde(default)]
    pub options: BTreeMap<String, OptionSpec>,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub keys: Vec<Key>,
    /// The blocks it defines (`diagram` is `owner.name.diagram`), each drawn by the `src/ui.tsx`
    /// export named for it (`Diagram`).
    #[serde(default)]
    pub blocks: Vec<BlockSpec>,
    /// The blocks it draws but did not define, by block (`terminal`, `pumi_dev.marmaide.diagram`):
    /// the `src/ui.tsx` export that draws them. The core's `file` block is drawn by the plugin
    /// whose renderer's `match` fits its file best (`{"export": "Editor", "match": ["*"]}`).
    #[serde(default)]
    pub renderers: BTreeMap<String, Renderer>,
    /// What it adds to the window's slots (`panel`, `status`, …), by slot.
    #[serde(default)]
    pub slots: BTreeMap<String, Slot>,
    /// The traits it defines (`file` is `owner.name.file`): what a tab is beside its block.
    #[serde(default)]
    pub traits: Vec<TraitSpec>,
    /// Its pages in Settings, and its sections on a page (a built-in one or its own), each an
    /// export of src/ui.tsx. Without any, its options are a page Settings makes of them.
    #[serde(default, skip_serializing_if = "SettingsSpec::is_empty")]
    pub settings: SettingsSpec,
}
/// What a plugin draws in Settings (`contributes.settings`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<SettingsPage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SettingsSection>,
}
impl SettingsSpec {
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty() && self.sections.is_empty()
    }
}
/// A page of its own in Settings' nav, after the built-in ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsPage {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The src/ui.tsx export that draws it, loaded when the page is shown.
    pub component: String,
    /// Words Settings' search finds it by, beside its title.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}
/// A section on a page: a built-in one (`general`, `appearance`, `keys`, `plugins`), after its
/// own rows, or one of the plugin's pages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSection {
    pub id: String,
    pub page: String,
    pub title: String,
    pub component: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}
/// The built-in pages of Settings a plugin's section can be on.
pub const SETTINGS_PAGES: &[&str] = &["general", "appearance", "keys", "plugins"];
/// What draws a block a plugin did not define: an export of src/ui.tsx (`"Terminal"`), or for
/// the core's `file` block, one with the files it is for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Renderer {
    Export(String),
    Files(FileRenderer),
}
/// A renderer of the `file` block: its export, and the files it draws (`*.md`, `*`, `src/**/*.rs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRenderer {
    pub export: String,
    /// Globs: one without a `/` is matched against the file's name, one with a `/` against its
    /// whole path. `*` is any run of characters but `/`, `**` any at all, `?` one. Case is ignored.
    #[serde(rename = "match")]
    pub matches: Vec<String>,
}
impl Renderer {
    /// The src/ui.tsx export that draws the block.
    pub fn export(&self) -> &str {
        match self {
            Renderer::Export(name) => name,
            Renderer::Files(f) => &f.export,
        }
    }
    /// The files it draws: every one (`*`) unless it says.
    pub fn matches(&self) -> Vec<String> {
        match self {
            Renderer::Export(_) => vec!["*".into()],
            Renderer::Files(f) => f.matches.clone(),
        }
    }
    /// How well it fits `path`: the most literal characters of a glob of its that matches, or
    /// `None` when none does. `*.md` fits `README.md` better than `*`, which fits every file.
    pub fn fits(&self, path: &str) -> Option<usize> {
        self.matches().iter().filter_map(|glob| glob_fit(glob, path)).max()
    }
}
/// How well `glob` matches `path` (see `FileRenderer::matches`): its count of literal
/// characters, or `None`. A relative glob with a `/` in it is matched from any folder of the path.
/// A Windows path (`C:\docs\a.md`) is matched as `C:/docs/a.md`.
pub fn glob_fit(glob: &str, path: &str) -> Option<usize> {
    let path = path.replace('\\', "/");
    let path = path.as_str();
    let subject = if glob.contains('/') { path } else { path.rsplit('/').next().unwrap_or(path) };
    // A relative glob with a folder in it (`docs/**`) is matched from any folder in the path.
    let anchored = if glob.contains('/') && !glob.starts_with('/') && !glob.starts_with("**") {
        format!("**/{glob}")
    } else {
        glob.to_string()
    };
    let pattern: Vec<char> = anchored.to_lowercase().chars().collect();
    let subject: Vec<char> = subject.to_lowercase().chars().collect();
    fn walk(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') if p.get(1) == Some(&'*') => {
                let rest = if p.get(2) == Some(&'/') { &p[3..] } else { &p[2..] };
                (0..=s.len()).any(|i| walk(rest, &s[i..])) || walk(&p[2..], s)
            }
            Some('*') => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != '/').any(|i| walk(&p[1..], &s[i..])),
            Some('?') => s.first().is_some_and(|c| *c != '/') && walk(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && walk(&p[1..], &s[1..]),
        }
    }
    walk(&pattern, &subject).then(|| glob.chars().filter(|c| !matches!(c, '*' | '?')).count())
}
/// A trait a plugin defines. A tab has it while its option `@<id>` is set, and its `fields` are
/// the options `@<id>.<field>`, both with the plugin's prefix (`@pumi_dev.files.file.path`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitSpec {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
}
/// The window's slots a plugin contributes to: a fixed set, which the window's parts draw (the
/// default theme's, or a UI theme's own). `sidebar.footer` is the bar at the side bar's foot: an
/// action there is a button beside the panel buttons, for a component that opens elsewhere (a
/// float, a view) rather than filling the side bar. `tab.new` is what a new tab can be (a pane's
/// `+` menu, a split's list): each action is run with `{tab, place}`, the tab it goes beside and
/// where it opens, as `tab.open` takes it.
pub const SLOTS: &[&str] = &[
    "window.overlay",
    "session.header",
    "session.menu",
    "view.actions",
    "view.badge",
    "view.menu",
    "tab.toolbar",
    "tab.strip",
    "tab.overlay",
    "tab.menu",
    "tab.new",
    "agent.controls",
    "agent.footer",
    "agent.menu",
    "panel",
    "sidebar.footer",
    "palette.actions",
];
/// The slots that hold actions and nothing else: the palette lists them.
pub const ACTION_SLOTS: &[&str] = &["palette.actions"];
/// A plugin's contribution to one slot: actions (data, each running one of its commands) and
/// a component (an export of its `src/ui.tsx`, loaded when the slot is on screen).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SlotAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotAction {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// One of its `commands`, run with what the slot is of (`{tab}`).
    pub command: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub title: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub key: String,
    pub command: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSpec {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The version of its tabs' saved `data`; a state of another version is refused (1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    /// How its tabs come back: `exact`, `relaunch` or `reference` (`exact`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// Its tabs' own methods: `tab.call` of one fires `tab.called` for its renderer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
}
/// An option's declaration: `option.declare`'s fields, and `secret`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionSpec {
    #[serde(rename = "type")]
    pub ty: OptionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    pub desc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Control>,
    /// The blocks it belongs to: one of its own by id (`page`), or another's (`terminal`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionType {
    Boolean,
    Number,
    String,
    Table,
    Secret,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Session,
    View,
    Tab,
    Client,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Control {
    Toggle,
    Slider,
    Json,
    /// A `#rrggbb` string: a few swatches, and a field for any other.
    Color,
}
/// When a plugin's worker starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activation {
    Startup,
    /// A tab of its block `<id>` opens.
    Block(String),
    /// Its command `<id>` runs (a key runs its command).
    Command(String),
    /// The event `<name>` fires.
    Event(String),
}
impl Activation {
    pub fn parse(text: &str) -> Option<Activation> {
        let named = |prefix: &str| text.strip_prefix(prefix).filter(|rest| !rest.is_empty()).map(str::to_string);
        if text == "onStartup" {
            return Some(Activation::Startup);
        }
        named("onBlock:")
            .map(Activation::Block)
            .or_else(|| named("onCommand:").map(Activation::Command))
            .or_else(|| named("onEvent:").map(Activation::Event))
    }
}
/// An installed plugin as its manifest says: its id, prefix and contributions.
#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    /// `owner/name`.
    pub id: String,
    pub owner: String,
    pub dir: PathBuf,
    /// `package.json`'s `main`: what its worker runs. A theme-only plugin has none.
    pub main: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// `package.json`'s `displayName`: its name as people read it (`Chat UI`), shown where its
    /// `name` (`chat-ui`) would be. Outside `pumi`, so a pumi that does not know it still loads it.
    pub display_name: Option<String>,
    pub manifest: Manifest,
}
impl Plugin {
    /// Its name as people read it: its `displayName`, else its `name`.
    pub fn title(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.manifest.name)
    }
    /// `owner.name.`, which its options, blocks, commands and events start with.
    pub fn prefix(&self) -> String {
        format!("{}.{}.", self.owner, self.manifest.name.replace('-', "_"))
    }
    /// A name of its own, prefixed: `bar` is `pumi_dev.queue.bar`.
    pub fn prefixed(&self, name: &str) -> String {
        format!("{}{name}", self.prefix())
    }
    pub fn activation(&self) -> Vec<Activation> {
        self.manifest.activation.iter().filter_map(|a| Activation::parse(a)).collect()
    }
    /// Its worker starts now: it says `onStartup`.
    pub fn on_startup(&self) -> bool {
        self.activation().contains(&Activation::Startup)
    }
    /// Its worker starts on `wanted`: it lists it, or lists no activation at all and `wanted` is the
    /// use of one of its contributions (a block, a command).
    pub fn activates_on(&self, wanted: &Activation) -> bool {
        let listed = self.activation();
        if !listed.is_empty() {
            return listed.contains(wanted);
        }
        let contributes = &self.manifest.contributes;
        match wanted {
            Activation::Block(id) => contributes.blocks.iter().any(|b| &b.id == id),
            Activation::Command(id) => contributes.commands.iter().any(|c| &c.id == id),
            Activation::Startup | Activation::Event(_) => false,
        }
    }
    /// Its `secret` options' keychain accounts: their prefixed names.
    pub fn secrets(&self) -> Vec<String> {
        let options = &self.manifest.contributes.options;
        options.iter().filter(|(_, spec)| spec.ty == OptionType::Secret).map(|(key, _)| self.prefixed(key)).collect()
    }
    /// Its block `<id>`, for the block `owner.name.<id>`.
    pub fn block_of(&self, block: &str) -> Option<&BlockSpec> {
        let id = block.strip_prefix(&self.prefix())?;
        self.manifest.contributes.blocks.iter().find(|b| b.id == id)
    }
    /// Its option `key`'s `option.declare` spec, `plugin` set to its id and a block of its own in
    /// `blocks` named in full (`page` is `pumi_dev.browser.page`). A `secret` has none: it
    /// is kept in the keychain (`SECRET_SERVICE`, its prefixed name), not by the server.
    pub fn declaration(&self, spec: &OptionSpec) -> Option<Value> {
        if spec.ty == OptionType::Secret {
            return None;
        }
        let own = &self.manifest.contributes.blocks;
        let blocks = spec.blocks.iter().map(|b| match own.iter().any(|o| &o.id == b) {
            true => self.prefixed(b),
            false => b.clone(),
        });
        let spec = OptionSpec { blocks: blocks.collect(), ..spec.clone() };
        let mut value = serde_json::to_value(spec).ok()?;
        let map = value.as_object_mut()?;
        map.insert("plugin".into(), Value::String(self.id.clone()));
        Some(value)
    }
    /// Its trait `t`, as `core.setTraits` takes it.
    pub fn trait_spec(&self, t: &TraitSpec) -> Value {
        serde_json::json!({ "id": self.prefixed(&t.id), "provider": self.id, "desc": t.desc, "fields": t.fields })
    }
    /// Its block `block`, as `core.setBlocks` takes it.
    pub fn block_spec(&self, block: &BlockSpec) -> Value {
        let mut spec =
            serde_json::json!({ "id": self.prefixed(&block.id), "provider": self.id, "methods": block.methods });
        if let Some(version) = block.version {
            spec["version"] = version.into();
        }
        if let Some(strategy) = &block.strategy {
            spec["strategy"] = strategy.clone().into();
        }
        spec
    }
}
/// The owner a plugin's source gives: a GitHub URL's (`https://github.com/O/r`,
/// `git@github.com:O/r`), lowercased with `-` as `_`; else the manifest's `owner`, or `local`.
pub fn owner(src: Option<&str>, manifest: &Manifest) -> String {
    let github = src.and_then(|src| {
        let rest = src
            .strip_prefix("https://github.com/")
            .or_else(|| src.strip_prefix("http://github.com/"))
            .or_else(|| src.strip_prefix("git@github.com:"))
            .or_else(|| src.strip_prefix("ssh://git@github.com/"))?;
        rest.split('/').next().filter(|o| !o.is_empty())
    });
    match github {
        Some(owner) => owner.to_lowercase().replace('-', "_"),
        None => manifest.owner.clone().unwrap_or_else(|| LOCAL.to_string()),
    }
}
/// The plugin in `dir`, from its `package.json`; `src` is where it was installed from.
pub fn read(dir: &Path, src: Option<&str>) -> Result<Plugin, String> {
    let path = dir.join("package.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text, dir, src)
}
/// A `package.json`'s text, as the plugin in `dir`.
pub fn parse(text: &str, dir: &Path, src: Option<&str>) -> Result<Plugin, String> {
    let json: Value = serde_json::from_str(text).map_err(|e| format!("package.json: {e}"))?;
    let Some(pumi) = json.get("pumi") else {
        return Err("package.json has no `pumi` key: it is not a pumi plugin".into());
    };
    if pumi.get("api").is_none() {
        return Err(
            "package.json's `pumi.api` is missing: without the API level it is written for, it is not run".into()
        );
    }
    let manifest: Manifest = serde_json::from_value(pumi.clone()).map_err(|e| format!("package.json's `pumi`: {e}"))?;
    validate(&manifest).map_err(|e| format!("package.json's `pumi`: {e}"))?;
    let string = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_string);
    let owner = owner(src, &manifest);
    Ok(Plugin {
        id: format!("{owner}/{}", manifest.name),
        owner,
        dir: dir.to_path_buf(),
        main: string("main"),
        version: string("version"),
        description: string("description"),
        display_name: string("displayName").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        manifest,
    })
}
fn word(text: &str, first: impl Fn(char) -> bool, rest: impl Fn(char) -> bool) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(first) && chars.all(rest)
}
fn lower(c: char) -> bool {
    c.is_ascii_lowercase()
}
/// The methods every plugin block answers itself, which a block cannot declare.
pub const BLOCK_PROTOCOL: [&str; 6] = ["read", "input", "selection", "describe", "args", "report"];
/// What the schema's patterns and sizes say, which serde does not.
pub fn validate(m: &Manifest) -> Result<(), String> {
    let digit_or_lower = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    if !word(&m.name, lower, |c| digit_or_lower(c) || c == '_' || c == '-') {
        return Err(format!("`name` {:?} is lowercase letters, digits, `_` and `-`, from a letter", m.name));
    }
    if let Some(owner) = &m.owner
        && !word(owner, digit_or_lower, |c| digit_or_lower(c) || c == '_')
    {
        return Err(format!("`owner` {owner:?} is lowercase letters, digits and `_`"));
    }
    if m.api < 1 {
        return Err("`api` is 1 or more".into());
    }
    if let Some(v) = &m.min_pumi
        && !(v.split('.').count() == 3 && v.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())))
    {
        return Err(format!("`minPumi` {v:?} is a version, `0.43.0`"));
    }
    for (i, a) in m.uses.iter().enumerate() {
        if m.uses[..i].contains(a) {
            return Err(format!("`uses` lists {a:?} twice"));
        }
    }
    if m.themes.iter().any(String::is_empty) {
        return Err("a `themes` entry is empty".into());
    }
    if let Some(icon) = &m.icon {
        let ext = icon.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if !["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
            return Err(format!("icon {icon:?} is not a PNG, JPEG, WebP or GIF"));
        }
    }
    for image in &m.images {
        let ext = image.path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if !["png", "jpg", "jpeg", "webp", "gif", "svg"].contains(&ext.as_str()) {
            return Err(format!("image {:?} is not a PNG, JPEG, WebP, GIF or SVG", image.path));
        }
        if image.alt.is_empty() {
            return Err(format!("image {:?} has no `alt`", image.path));
        }
    }
    for a in &m.activation {
        if Activation::parse(a).is_none() {
            return Err(format!(
                "activation {a:?} is `onStartup`, `onBlock:<id>`, `onCommand:<id>` or `onEvent:<name>`"
            ));
        }
    }
    let c = &m.contributes;
    for key in c.options.keys() {
        if !word(key, lower, |ch| digit_or_lower(ch) || ch == '_') {
            return Err(format!("option {key:?} is lowercase letters, digits and `_`, from a letter"));
        }
    }
    for command in &c.commands {
        if !word(&command.id, lower, |ch| ch.is_ascii_alphanumeric() || "_.-".contains(ch)) {
            return Err(format!(
                "command {:?} is letters, digits, `_`, `.` and `-`, from a lowercase letter",
                command.id
            ));
        }
        if command.title.is_empty() {
            return Err(format!("command {:?} has no `title`", command.id));
        }
    }
    for key in &c.keys {
        if key.key.is_empty() || key.command.is_empty() {
            return Err("a key needs a `key` and a `command`".into());
        }
    }
    let export = |v: &str| word(v, |c| c.is_ascii_alphabetic(), |c| c.is_ascii_alphanumeric() || c == '_');
    for (name, slot) in &c.slots {
        if !SLOTS.contains(&name.as_str()) {
            return Err(format!("slot {name:?} is not one of the window's: {}", SLOTS.join(", ")));
        }
        if slot.actions.is_empty() && slot.component.is_none() {
            return Err(format!("slot {name:?} gives no `actions` and no `component`"));
        }
        if slot.component.is_some() && ACTION_SLOTS.contains(&name.as_str()) {
            return Err(format!("slot {name:?} holds actions, not a component"));
        }
        if let Some(component) = slot.component.as_deref().filter(|v| !export(v)) {
            return Err(format!("slot {name:?}: `component` {component:?} is the name of an export of src/ui.tsx"));
        }
        for action in &slot.actions {
            if !c.commands.iter().any(|command| command.id == action.command) {
                return Err(format!(
                    "slot {name:?}: action {:?} runs {:?}, which is not one of its `commands`",
                    action.id, action.command
                ));
            }
        }
    }
    let settings_id = |id: &str| word(id, lower, |ch| digit_or_lower(ch) || ch == '_' || ch == '-');
    for (i, page) in c.settings.pages.iter().enumerate() {
        if !settings_id(&page.id) {
            return Err(format!(
                "settings page {:?} is lowercase letters, digits, `_` and `-`, from a letter",
                page.id
            ));
        }
        if SETTINGS_PAGES.contains(&page.id.as_str()) || c.settings.pages[..i].iter().any(|p| p.id == page.id) {
            return Err(format!("settings page {:?} is named twice, or as a built-in page", page.id));
        }
        if page.title.is_empty() {
            return Err(format!("settings page {:?} has no `title`", page.id));
        }
        if !export(&page.component) {
            return Err(format!(
                "settings page {:?}: `component` {:?} is the name of an export of src/ui.tsx",
                page.id, page.component
            ));
        }
    }
    for (i, section) in c.settings.sections.iter().enumerate() {
        if !settings_id(&section.id) || c.settings.sections[..i].iter().any(|s| s.id == section.id) {
            return Err(format!("settings section {:?} is lowercase letters, digits, `_` and `-`, once", section.id));
        }
        if !SETTINGS_PAGES.contains(&section.page.as_str()) && !c.settings.pages.iter().any(|p| p.id == section.page) {
            return Err(format!(
                "settings section {:?}: `page` {:?} is {} or one of its own `pages`",
                section.id,
                section.page,
                SETTINGS_PAGES.join(", ")
            ));
        }
        if section.title.is_empty() {
            return Err(format!("settings section {:?} has no `title`", section.id));
        }
        if !export(&section.component) {
            return Err(format!(
                "settings section {:?}: `component` {:?} is the name of an export of src/ui.tsx",
                section.id, section.component
            ));
        }
    }
    for block in &c.blocks {
        if !word(&block.id, lower, |ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!("block {:?} is letters, digits and `_`, from a lowercase letter", block.id));
        }
        if block.version == Some(0) {
            return Err(format!("block {:?}: `version` is 1 or more", block.id));
        }
        if let Some(icon) = &block.icon {
            crate::icon::check_icon_src(icon).map_err(|err| format!("block {:?}: `icon`: {err}", block.id))?;
        }
        if let Some(s) = block.strategy.as_deref().filter(|s| !["exact", "relaunch", "reference"].contains(s)) {
            return Err(format!("block {:?}: `strategy` {s:?} is `exact`, `relaunch` or `reference`", block.id));
        }
        for (i, method) in block.methods.iter().enumerate() {
            if !word(method, lower, |ch| ch.is_ascii_alphanumeric() || ch == '_') || block.methods[..i].contains(method)
            {
                return Err(format!("block {:?}: method {method:?} is letters, digits and `_`, once", block.id));
            }
            if BLOCK_PROTOCOL.contains(&method.as_str()) {
                return Err(format!(
                    "block {:?}: `{method}` is the block protocol's, not a method of its own",
                    block.id
                ));
            }
        }
    }
    for (i, t) in c.traits.iter().enumerate() {
        if !word(&t.id, lower, |ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_') {
            return Err(format!("trait {:?} is lowercase letters, digits and `_`, from a letter", t.id));
        }
        if c.traits[..i].iter().any(|o| o.id == t.id) {
            return Err(format!("trait {:?} is defined twice", t.id));
        }
        for (j, field) in t.fields.iter().enumerate() {
            if !word(field, lower, |ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                || t.fields[..j].contains(field)
            {
                return Err(format!("trait {:?}: field {field:?} is lowercase letters, digits and `_`, once", t.id));
            }
        }
    }
    for (block, renderer) in &c.renderers {
        let core = word(block, lower, |ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
        let plugin = block.split('.').count() == 3
            && block.split('.').all(|w| {
                word(
                    w,
                    |ch| lower(ch) || ch.is_ascii_digit(),
                    |ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-',
                )
            });
        if !core && !plugin {
            return Err(format!("renderers: {block:?} is a block's id: `terminal`, or `owner.name.id`"));
        }
        if !export(renderer.export()) {
            return Err(format!(
                "renderers: {block:?} is drawn by {:?}, which is not the name of an export of src/ui.tsx",
                renderer.export()
            ));
        }
        if let Renderer::Files(f) = renderer {
            if block != "file" {
                return Err(format!("renderers: {block:?} is drawn by an export's name; `match` is for `file`"));
            }
            if f.matches.is_empty() || f.matches.iter().any(|g| g.trim().is_empty()) {
                return Err("renderers: file's `match` is a list of globs (`*.md`, `*`)".into());
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    const QUEUE: &str = r#"{
      "name": "pumi-queue", "version": "0.3.0", "main": "src/main.ts",
      "pumi": {
        "name": "queue", "api": 1, "minPumi": "0.43.0", "uses": ["processes"],
        "contributes": {
          "options": { "side_by_side": { "type": "boolean", "default": false, "label": "Side by side", "desc": "Run two at once." } },
          "commands": [{ "id": "toggle", "title": "Toggle the queue" }],
          "keys": [{ "key": "<C-q>", "command": "toggle" }],
          "blocks": [{ "id": "bar", "title": "Queue", "icon": "list" }]
        },
        "activation": ["onStartup"]
      }
    }"#;
    fn queue(src: Option<&str>) -> Result<Plugin, String> {
        parse(QUEUE, Path::new("/p/queue"), src)
    }
    #[test]
    fn a_queue_manifest_reads() {
        let p = queue(Some("https://github.com/pumi-dev/pumi-plugins")).unwrap();
        assert_eq!((p.id.as_str(), p.prefix().as_str()), ("pumi_dev/queue", "pumi_dev.queue."));
        assert_eq!(p.main.as_deref(), Some("src/main.ts"));
        assert_eq!(p.prefixed("bar"), "pumi_dev.queue.bar");
        assert!(p.on_startup());
        assert_eq!(p.block_of("pumi_dev.queue.bar").map(|b| b.id.as_str()), Some("bar"));
        let spec = &p.manifest.contributes.options["side_by_side"];
        let declared = p.declaration(spec).unwrap();
        assert_eq!(declared["plugin"], "pumi_dev/queue");
        assert_eq!(declared["type"], "boolean");
        assert!(declared.get("scope").is_none());
        assert!(p.secrets().is_empty());
        assert_eq!(data_dir(Path::new("/s"), &p.id), Path::new("/s/plugins/pumi_dev.queue"));
    }
    #[test]
    fn its_title_is_its_display_name_else_its_name() {
        let title = |extra: &str| {
            let text = format!(r#"{{ {extra} "pumi": {{ "name": "chat-ui", "api": 1 }} }}"#);
            parse(&text, Path::new("/p"), None).map(|p| p.title().to_string()).unwrap()
        };
        assert_eq!(title(r#""displayName": " Chat UI ","#), "Chat UI");
        assert_eq!(title(r#""displayName": "","#), "chat-ui");
        assert_eq!(title(""), "chat-ui");
    }
    #[test]
    fn images_are_pictures_with_alt_text_and_may_be_svg() {
        let with = |images: &str| {
            parse(&format!(r#"{{ "pumi": {{ "name": "x", "api": 1, "images": {images} }} }}"#), Path::new("/p"), None)
                .map(|p| p.manifest.images.len())
        };
        assert_eq!(with(r#"[{ "path": "preview.svg", "alt": "A" }, { "path": "shot.PNG", "alt": "B" }]"#), Ok(2));
        assert!(with(r#"[{ "path": "preview.html", "alt": "A" }]"#).unwrap_err().contains("or SVG"));
        assert!(with(r#"[{ "path": "preview.svg", "alt": "" }]"#).unwrap_err().contains("no `alt`"));
    }
    #[test]
    fn slots_are_the_window_s_and_their_actions_run_its_commands() {
        let with = |slots: &str| {
            let text = format!(
                r#"{{ "pumi": {{ "name": "x", "api": 1, "contributes": {{
                "commands": [{{ "id": "stop", "title": "Stop" }}], "slots": {slots} }} }} }}"#
            );
            parse(&text, Path::new("/p"), None).map(|p| p.manifest.contributes.slots)
        };
        let slots = with(
            r#"{ "tab.toolbar": { "actions": [{ "id": "s", "title": "Stop", "command": "stop" }] },
            "panel": { "component": "Files", "title": "Files" } }"#,
        )
        .unwrap();
        assert_eq!(slots["panel"].component.as_deref(), Some("Files"));
        assert!(with(r#"{ "sidebar": { "component": "Files" } }"#).unwrap_err().contains("not one of the window's"));
        for gone in ["status", "window.titlebar"] {
            let err = with(&format!(r#"{{ "{gone}": {{ "component": "Bar" }} }}"#)).unwrap_err();
            assert!(err.contains("not one of the window's"), "{gone}: {err}");
        }
        assert!(with(r#"{ "agent.controls": { "component": "Stop" } }"#).is_ok(), "an agent's row");
        assert!(with(r#"{ "palette.actions": { "component": "Menu" } }"#).unwrap_err().contains("holds actions"));
        assert!(with(r#"{ "tab.toolbar": {} }"#).unwrap_err().contains("no `actions`"));
        assert!(
            with(r#"{ "tab.toolbar": { "actions": [{ "id": "a", "title": "A", "command": "go" }] } }"#)
                .unwrap_err()
                .contains("not one of its `commands`")
        );
        assert!(with(r#"{ "panel": { "component": "a-b" } }"#).unwrap_err().contains("an export"));
    }
    #[test]
    fn settings_pages_are_its_own_and_sections_go_on_a_built_in_page_or_one_of_them() {
        let with = |settings: &str| {
            let text = format!(
                r#"{{ "pumi": {{ "name": "remote", "api": 1, "contributes": {{ "settings": {settings} }} }} }}"#
            );
            parse(&text, Path::new("/p"), None).map(|p| p.manifest.contributes.settings)
        };
        let s = with(
            r#"{ "pages": [{ "id": "remote", "title": "Remote", "icon": "globe", "component": "RemotePage", "keywords": ["qr"] }],
               "sections": [{ "id": "tailscale", "page": "general", "title": "Tailscale", "component": "TailscaleSection" },
                            { "id": "devices", "page": "remote", "title": "Devices", "component": "Devices" }] }"#,
        )
        .unwrap();
        assert_eq!(s.pages[0].keywords, ["qr"]);
        assert_eq!(s.sections[1].page, "remote");
        let section = |page: &str| {
            format!(r#"{{ "sections": [{{ "id": "a", "page": "{page}", "title": "A", "component": "A" }}] }}"#)
        };
        assert!(with(&section("elsewhere")).unwrap_err().contains("one of its own `pages`"));
        assert!(with(&section("keys")).is_ok());
        let page = |id: &str, component: &str| {
            format!(r#"{{ "pages": [{{ "id": "{id}", "title": "P", "component": "{component}" }}] }}"#)
        };
        assert!(with(&page("general", "P")).unwrap_err().contains("built-in"));
        assert!(with(&page("mine", "a-b")).unwrap_err().contains("an export"));
        assert!(
            with(r#"{ "pages": [{ "id": "p", "title": "", "component": "P" }] }"#).unwrap_err().contains("no `title`")
        );
        assert!(with("{}").unwrap().is_empty());
    }
    #[test]
    fn a_secret_is_the_keychain_s_not_the_server_s() {
        let text = r#"{ "pumi": { "name": "x", "api": 1, "contributes": { "options": {
            "token": { "type": "secret", "desc": "The token." } } } } }"#;
        let p = parse(text, Path::new("/p"), None).unwrap();
        assert_eq!(p.declaration(&p.manifest.contributes.options["token"]), None);
        assert_eq!(p.secrets(), vec!["local.x.token".to_string()]);
    }
    #[test]
    fn the_owner_comes_from_github_or_the_manifest() {
        let m = queue(None).unwrap().manifest;
        assert_eq!(owner(Some("git@github.com:Some-One/repo.git"), &m), "some_one");
        assert_eq!(owner(Some("/Users/me/queue"), &m), "local");
        let owned = Manifest { owner: Some("me".into()), ..m };
        assert_eq!(owner(Some("/Users/me/queue"), &owned), "me");
        assert_eq!(owner(None, &owned), "me");
    }
    #[test]
    fn a_dash_in_the_name_is_an_underscore_in_the_prefix() {
        let text = r#"{ "pumi": { "name": "pi-session", "api": 1 } }"#;
        let p = parse(text, Path::new("/p"), None).unwrap();
        assert_eq!((p.id.as_str(), p.prefix().as_str()), ("local/pi-session", "local.pi_session."));
        assert_eq!(p.main, None);
    }
    #[test]
    fn unknown_keys_and_a_missing_level_are_refused() {
        let at = Path::new("/p");
        assert!(parse(r#"{ "pumi": { "name": "x" } }"#, at, None).unwrap_err().contains("pumi.api"));
        assert!(parse(r#"{ "name": "x" }"#, at, None).unwrap_err().contains("no `pumi` key"));
        assert!(
            parse(r#"{ "pumi": { "name": "x", "api": 1, "colour": 1 } }"#, at, None).unwrap_err().contains("colour")
        );
        let option = r#"{ "pumi": { "name": "x", "api": 1, "contributes": { "options": { "a": { "type": "string", "desc": "", "wat": 1 } } } } }"#;
        assert!(parse(option, at, None).unwrap_err().contains("wat"));
        let activation = r#"{ "pumi": { "name": "x", "api": 1, "activation": ["onLoad"] } }"#;
        assert!(parse(activation, at, None).unwrap_err().contains("onLoad"));
        assert!(parse(r#"{ "pumi": { "name": "X", "api": 1 } }"#, at, None).is_err());
        assert!(parse(r#"{ "pumi": { "name": "x", "api": 0 } }"#, at, None).is_err());
        assert!(parse(r#"{ "pumi": { "name": "x", "api": 1, "uses": ["files", "files"] } }"#, at, None).is_err());
    }
    #[test]
    fn a_block_has_its_version_strategy_methods_and_options() {
        let text = r#"{ "pumi": { "name": "browser", "owner": "pumi_dev", "api": 1, "contributes": {
            "options": { "url": { "type": "string", "scope": "tab", "blocks": ["page", "terminal"], "desc": "" } },
            "blocks": [{ "id": "page", "version": 2, "strategy": "reference", "methods": ["navigate", "back"] },
                      { "id": "plain" }] } } }"#;
        let p = parse(text, Path::new("/p"), None).unwrap();
        let blocks = &p.manifest.contributes.blocks;
        let page = serde_json::json!({ "id": "pumi_dev.browser.page", "provider": "pumi_dev/browser",
            "methods": ["navigate", "back"], "version": 2, "strategy": "reference" });
        assert_eq!(p.block_spec(&blocks[0]), page);
        assert_eq!(
            p.block_spec(&blocks[1]),
            serde_json::json!({ "id": "pumi_dev.browser.plain", "provider": "pumi_dev/browser", "methods": [] })
        );
        let declared = p.declaration(&p.manifest.contributes.options["url"]).unwrap();
        assert_eq!(declared["blocks"], serde_json::json!(["pumi_dev.browser.page", "terminal"]), "its own block by id");
        let component = |fields: &str| {
            let text = format!(
                r#"{{ "pumi": {{ "name": "x", "api": 1, "contributes": {{ "blocks": [{{ "id": "v", {fields} }}] }} }} }}"#
            );
            parse(&text, Path::new("/p"), None)
        };
        assert!(component(r#""version": 0"#).unwrap_err().contains("version"));
        assert!(component(r#""icon": "plugin:icons/v.svg""#).is_ok());
        assert!(component(r#""icon": "plugin:../v.svg""#).unwrap_err().contains("icon"));
        assert!(component(r#""strategy": "again""#).unwrap_err().contains("strategy"));
        assert!(component(r#""methods": ["read"]"#).unwrap_err().contains("protocol"));
        assert!(component(r#""methods": ["go", "go"]"#).unwrap_err().contains("once"));
        assert!(component(r#""methods": ["Go"]"#).is_err());
        assert!(component(r#""methods": ["goBack"], "strategy": "relaunch""#).is_ok());
    }
    #[test]
    fn a_trait_is_named_with_its_fields() {
        let with = |traits: &str| {
            let text = format!(
                r#"{{ "pumi": {{ "name": "files", "owner": "pumi_dev", "api": 1, "contributes": {{ "traits": {traits} }} }} }}"#
            );
            parse(&text, Path::new("/p"), None)
        };
        let p = with(r#"[{ "id": "file", "fields": ["path", "line"] }]"#).unwrap();
        let spec = p.trait_spec(&p.manifest.contributes.traits[0]);
        assert_eq!(
            (spec["id"].clone(), spec["provider"].clone()),
            (serde_json::json!("pumi_dev.files.file"), serde_json::json!("pumi_dev/files"))
        );
        assert!(with(r#"[{ "id": "File" }]"#).is_err());
        assert!(with(r#"[{ "id": "a" }, { "id": "a" }]"#).unwrap_err().contains("twice"));
        assert!(with(r#"[{ "id": "a", "fields": ["x", "x"] }]"#).unwrap_err().contains("once"));
    }
    #[test]
    fn renderers_draw_a_block_by_its_id_with_an_export() {
        let with = |renderers: &str| {
            let text =
                format!(r#"{{ "pumi": {{ "name": "x", "api": 1, "contributes": {{ "renderers": {renderers} }} }} }}"#);
            parse(&text, Path::new("/p"), None).map(|p| p.manifest.contributes.renderers)
        };
        let drawn = with(r#"{ "terminal": "Terminal", "pumi_dev.marmaide.diagram": "Dark" }"#).unwrap();
        assert_eq!(drawn["terminal"].export(), "Terminal");
        let files = with(r#"{ "file": { "export": "Editor", "match": ["*.md", "docs/**"] } }"#).unwrap();
        assert_eq!((files["file"].export(), files["file"].fits("/a/README.md")), ("Editor", Some(3)));
        assert_eq!(files["file"].fits("/a/docs/x/y.txt"), Some(5), "a glob with a `/` is matched against the path");
        assert_eq!(files["file"].fits("/a/b.rs"), None);
        assert_eq!(with(r#"{ "file": "Viewer" }"#).unwrap()["file"].fits("/x"), Some(0), "every file, least well");
        assert!(with(r#"{ "terminal": { "export": "T", "match": ["*"] } }"#).unwrap_err().contains("for `file`"));
        assert!(with(r#"{ "file": { "export": "E", "match": [] } }"#).unwrap_err().contains("globs"));
        assert!(with(r#"{ "file": { "export": "E", "match": ["*"], "x": 1 } }"#).is_err());
        assert!(with(r#"{ "hello.badge": "Badge" }"#).unwrap_err().contains("a block's id"));
        assert!(with(r#"{ "terminal": "a-b" }"#).unwrap_err().contains("an export"));
    }
    #[test]
    fn globs_fit_a_file_by_name_or_path() {
        assert_eq!(super::glob_fit("*", "/a/b"), Some(0));
        assert_eq!(super::glob_fit("*.RS", "/a/main.rs"), Some(3), "case is ignored");
        assert_eq!(super::glob_fit("Makefile", "/x/Makefile"), Some(8));
        assert_eq!(super::glob_fit("?.md", "/x/ab.md"), None);
        assert_eq!(super::glob_fit("src/*.rs", "/p/src/a/b.rs"), None, "`*` stays in its folder");
        assert_eq!(super::glob_fit("**/src/**/*.rs", "/p/src/a/b.rs"), Some(9));
        assert_eq!(super::glob_fit("src/*.rs", "/p/src/b.rs"), Some(7), "from any folder");
        assert_eq!(super::glob_fit("/p/**", "/p/q/r"), Some(3));
        assert_eq!(super::glob_fit("docs/*.md", r"C:\docs\a.md"), Some(8), "a Windows path's folders");
    }
    #[test]
    fn with_no_activation_it_starts_on_first_use() {
        let text = r#"{ "pumi": { "name": "x", "api": 1, "contributes": {
            "commands": [{ "id": "go", "title": "Go" }], "blocks": [{ "id": "v" }] } } }"#;
        let p = parse(text, Path::new("/p"), None).unwrap();
        assert!(!p.on_startup());
        assert!(p.activates_on(&Activation::Block("v".into())));
        assert!(p.activates_on(&Activation::Command("go".into())));
        assert!(!p.activates_on(&Activation::Command("other".into())));
        let listed = parse(QUEUE, Path::new("/p"), None).unwrap();
        assert!(!listed.activates_on(&Activation::Block("bar".into())));
        assert_eq!(Activation::parse("onEvent:tab.opened"), Some(Activation::Event("tab.opened".into())));
        assert_eq!(Activation::parse("onBlock:"), None);
        assert_eq!(Activation::parse("onView:x"), None);
    }
}
