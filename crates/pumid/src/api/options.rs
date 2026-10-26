use super::kernel;
use crate::kernel::Kernel;
use crate::kernel::events::About;
use crate::kernel::options::{Decl, Key, Scope, Type};
use pumi_api::{Caller, Cx, Error, Kind, Ref, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};
/// Which object an internal `core.*` option call reads or sets, and how the value lasts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoreOptionOpts {
    /// `global`, `session`, `view`, `tab` or `client`; `plugin` is pumi-host's.
    #[serde(default)]
    pub scope: Option<String>,
    /// The object's id (`$1`, `@2`, `%3`). Without one, the numeric fields name it, as the
    /// server's own calls do.
    #[serde(default)]
    pub id: Option<Ref>,
    #[serde(default)]
    pub session: Option<u64>,
    #[serde(default)]
    pub view: Option<u64>,
    #[serde(default)]
    pub tab: Option<u64>,
    #[serde(default)]
    pub client: Option<u64>,
    /// Seconds the value holds for; then the option is the one it inherits again.
    #[serde(default)]
    pub ttl: Option<f64>,
    /// Who set it (`seq` orders one source's reports); the caller's source by default.
    #[serde(default)]
    pub source: Option<String>,
    /// With `source`: a report older than the last one from it is dropped.
    #[serde(default)]
    pub seq: Option<i64>,
}
/// The scope `opts` asks for, from its `scope` or else the object it names.
fn requested(opts: &CoreOptionOpts) -> Result<Option<Scope>> {
    let named = match &opts.id {
        Some(id) => Some(match id.kind {
            Kind::Session => Scope::Session,
            Kind::View => Scope::View,
            Kind::Tab => Scope::Tab,
            Kind::Pane => return Err(Error::invalid_params("a pane has no options: its tabs do")),
            Kind::Request => return Err(Error::invalid_params("a request has no options")),
        }),
        None if opts.tab.is_some() => Some(Scope::Tab),
        None if opts.view.is_some() => Some(Scope::View),
        None if opts.session.is_some() => Some(Scope::Session),
        None if opts.client.is_some() => Some(Scope::Client),
        None => None,
    };
    let Some(scope) = opts.scope.as_deref() else { return Ok(named) };
    let scope = match scope {
        "global" => Scope::Global,
        "session" => Scope::Session,
        "view" => Scope::View,
        "tab" => Scope::Tab,
        "client" => Scope::Client,
        "plugin" => {
            return Err(Error::invalid_params("a plugin's options are pumi-host's: `option.get` and `option.set`"));
        }
        other => return Err(Error::invalid_params(format!("`{other}` is not an option's scope"))),
    };
    Ok(Some(scope))
}
/// The object `opts` names, and the chain of objects a read falls back along.
fn target(k: &Kernel, caller: &Caller, name: &str, opts: &CoreOptionOpts) -> Result<(Key, Vec<Key>)> {
    let declared = k.model.options.decl(name).map(|d| d.scope);
    let scope = requested(opts)?.or(declared).unwrap_or(Scope::Global);
    let number = |kind: Kind| -> Result<u64> {
        match &opts.id {
            Some(id) => id.of(kind),
            None => Ok(match kind {
                Kind::Session => opts.session,
                Kind::View => opts.view,
                Kind::Tab => opts.tab,
                Kind::Pane | Kind::Request => None,
            }
            .unwrap_or(0)),
        }
    };
    let key = match scope {
        Scope::Global => Key::Global,
        Scope::Session => Key::Session(k.model.session_id(caller, number(Kind::Session)?)?),
        Scope::View => Key::View(k.model.view_id(caller, number(Kind::View)?)?),
        Scope::Tab => Key::Tab(k.model.tab_id(caller, number(Kind::Tab)?)?),
        Scope::Client => {
            let client = opts.client.unwrap_or(caller.channel);
            if k.model.chans.get(&client).is_none_or(|c| c.ui.is_none()) {
                return Err(Error::not_found(format!("no client {client}")));
            }
            Key::Client(client)
        }
    };
    let chain = match key {
        Key::Tab(p) => k.model.chain(Some(p), None, None),
        Key::View(t) => k.model.chain(None, Some(t), None),
        Key::Session(s) => k.model.chain(None, None, Some(s)),
        Key::Client(c) => vec![Key::Client(c), Key::Global],
        Key::Global => vec![Key::Global],
    };
    Ok((key, chain))
}
/// When a value was set to last until, and the last report each reporter made of it.
#[derive(Default)]
struct Reports(BTreeMap<(Key, String), Report>);
#[derive(Default)]
struct Report {
    expires: Option<SystemTime>,
    /// By who set it, its last `seq`.
    seq: BTreeMap<String, i64>,
}
fn reports(k: &mut Kernel) -> &mut Reports {
    if k.ext.get::<Reports>().is_none() {
        k.ext.insert(Reports::default());
    }
    k.ext.get_mut::<Reports>().expect("just inserted")
}
fn now() -> SystemTime {
    SystemTime::now()
}
/// An option's value as an object sees it: its own, else the nearest one it inherits (a tab's
/// from its view, session, then the server), else the default. `null` for a `@user` option that
/// is not set.
#[api(
    name = "core.getOptionValue",
    internal,
    effect = "read",
    example(args = r#"["history_bytes", {}]"#, result = "4194304"),
    example(args = r#"["@build", {"scope": "global"}]"#, result = "null", desc = "A user option nobody set."),
    example(args = r#"["tab_current_path", {"id": "%1"}]"#, desc = "A read-only one: where a terminal is.")
)]
pub fn pumi_get_option_value(cx: &mut Cx, name: String, opts: Option<CoreOptionOpts>) -> Result<Value> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    let (key, chain) = target(k, &caller, &name, &opts)?;
    if reports(k).0.get(&(key, name.clone())).and_then(|r| r.expires).is_some_and(|at| at <= now()) {
        return Err(Error::not_found(format!("no option `{name}`")));
    }
    match k.model.options.get(&name, &chain) {
        Some((value, _)) => Ok(value),
        None => Err(Error::not_found(format!("no option `{name}`"))),
    }
}
/// `option.set` as the core does it: the value already in the option's type (the host's
/// `option.set` coerces `on`, clamps and steps), with the `ttl` and `seq` it was given.
#[api(
    name = "core.setOptionValue",
    internal,
    example(args = r#"["@build", "ok", {"scope": "global"}]"#, result = "null")
)]
pub fn pumi_core_set_option_value(cx: &mut Cx, name: String, value: Value, opts: Option<CoreOptionOpts>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    let (key, _) = target(k, &caller, &name, &opts)?;
    let source = opts.source.clone().unwrap_or_else(|| k.source(&caller));
    if let Some(seq) = opts.seq
        && reports(k).0.get(&(key, name.clone())).and_then(|r| r.seq.get(&source)).is_some_and(|last| seq <= *last)
    {
        return Ok(());
    }
    if value.is_null() {
        if k.model.options.is_readonly(&name) {
            return Err(Error::invalid_params(format!("`{name}` is read-only")));
        }
        k.unset_option(&name, key, &source);
    } else {
        k.set_option(&name, key, value, &source, false)?;
    }
    let report = reports(k).0.entry((key, name.clone())).or_default();
    if let Some(seq) = opts.seq {
        report.seq.insert(source, seq);
    }
    report.expires = opts.ttl.filter(|ttl| *ttl > 0.0).map(|ttl| now() + Duration::from_secs_f64(ttl));
    Ok(())
}
/// Every option declared, as `registry.list("option")` lists them: its type, default, scope,
/// choices and description.
#[api(name = "core.options", internal, effect = "read", example(args = "[]", result = "[]"))]
pub fn pumi_core_options(cx: &mut Cx) -> Result<Vec<Decl>> {
    let (_, k) = kernel(cx)?;
    Ok(k.model.options.decls().cloned().collect())
}
/// A new option's declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionSpec {
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default)]
    pub default: Value,
    /// `global` by default.
    #[serde(default)]
    pub scope: Option<Scope>,
    pub desc: String,
    #[serde(default)]
    pub choices: Option<Vec<Value>>,
    /// Its title in Settings; made of its name by default.
    #[serde(default)]
    pub label: Option<String>,
    /// What Settings calls each of `choices`, in their order.
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// A number's unit in Settings (`pt`, `%`).
    #[serde(default)]
    pub unit: Option<String>,
    /// What an empty text field shows.
    #[serde(default)]
    pub placeholder: Option<String>,
    /// Its heading in Settings; its plugin's name by default.
    #[serde(default)]
    pub group: Option<String>,
    /// The plugin declaring it; Lua fills it in for a plugin's own files.
    #[serde(default)]
    pub plugin: Option<String>,
    /// A number's range and step (clamped and snapped when set).
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    /// How Settings edits it when its type does not say: `toggle`, `slider`, `json`, `color`.
    #[serde(default)]
    pub control: Option<String>,
    /// Set only by whoever declared it (and the core), never by other callers.
    #[serde(default)]
    pub readonly: bool,
    /// The blocks it belongs to (`url` is a page's); every tab's when empty.
    #[serde(default)]
    pub blocks: Vec<String>,
}
/// Declare an option, which Settings then shows. A plugin's is named after it, dotted:
/// `files.hide_hidden`. The host declares the chrome's and the app's settings the same way.
#[api(
    name = "option.declare",
    internal,
    example(
        args = r#"["files.sort", {"type": "string", "default": "name", "desc": "How files sort.", "choices": ["name", "mtime"]}]"#,
        result = "null"
    )
)]
pub fn pumi_create_option(cx: &mut Cx, name: String, spec: OptionSpec) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    if spec.plugin.is_some() && !name.contains('.') {
        return Err(Error::invalid_params(format!("a plugin's option is named `<plugin>.<name>`, not `{name}`")));
    }
    let source = k.source(&caller);
    let decl = Decl {
        name: name.clone(),
        ty: spec.ty,
        default: spec.default,
        scope: spec.scope.unwrap_or(Scope::Global),
        desc: spec.desc,
        choices: spec.choices,
        readonly: spec.readonly,
        blocks: spec.blocks,
        source,
        plugin: spec.plugin,
        label: spec.label,
        labels: spec.labels,
        unit: spec.unit,
        placeholder: spec.placeholder,
        group: spec.group,
        min: spec.min,
        max: spec.max,
        step: spec.step,
        control: spec.control,
        pending: false,
        order: 0,
    };
    k.model.options.declare(decl)?;
    k.emit("registry.changed", "option", About::default(), json!({ "kind": "option", "names": [name] }));
    Ok(())
}
