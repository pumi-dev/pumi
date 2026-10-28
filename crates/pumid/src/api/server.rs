use super::kernel;
use crate::kernel::Route;
use crate::kernel::events::About;
use pumi_api::meta::{Effect, ExampleInfo, RunsOn};
use pumi_api::{Cx, Error, FunctionInfo, Nullable, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
/// `{"chan": 0, "ref": "hello/3"}`: a callback a channel holds; `0` is the caller's channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChanRef {
    pub chan: u64,
    #[serde(rename = "ref")]
    pub reference: String,
}
/// Stop the core, saving everything first. Its terminals' programs are
/// hung up; their state comes back when it starts again.
#[api(name = "server.kill", internal, example(args = "[]", result = "null"))]
pub fn pumi_kill_server(cx: &mut Cx) -> Result<()> {
    let (_, k) = kernel(cx)?;
    k.stop = true;
    Ok(())
}
/// Start the core again as the binary on disk now (an update), keeping every terminal and its
/// program running: the model and the PTYs pass to the new process image. Channels are
/// disconnected and reconnect; a client attached to a terminal resumes from the byte it had.
#[api(name = "server.restart", internal, example(args = "[]", result = "null"))]
pub fn pumi_restart(cx: &mut Cx) -> Result<()> {
    let (_, k) = kernel(cx)?;
    k.restart = true;
    Ok(())
}
/// A binary the server runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Binary {
    pub path: String,
    /// Another file is on disk at `path` now.
    pub outdated: bool,
}
/// The binaries the core and its host run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Image {
    /// Either binary is outdated: `server.restart` would start the new ones.
    pub outdated: bool,
    pub core: Option<Binary>,
    pub host: Option<Binary>,
}
/// The binaries the core and its host run, and whether an update has replaced either on disk
/// since it started. `server.restart` starts the core again as the new one and, when the host's
/// was replaced, the host too.
#[api(name = "server.getImage", internal, effect = "read", example(args = "[]"))]
pub fn pumi_get_image(cx: &mut Cx) -> Result<Image> {
    let (_, k) = kernel(cx)?;
    let Some(running) = k.ext.get::<crate::lifecycle::Running>() else { return Ok(Image::default()) };
    let binary =
        |image: &crate::lifecycle::Image| Binary { path: image.path.display().to_string(), outdated: image.outdated() };
    let (core, host) = (running.core.as_ref().map(binary), running.host.as_ref().map(binary));
    let outdated = core.iter().chain(&host).any(|b| b.outdated);
    Ok(Image { outdated, core, host })
}
/// A function `fn.set` provides: what it does, its arguments and its answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FnSpec {
    pub desc: String,
    /// `read` or `write`; `write` by default.
    #[serde(default)]
    pub effect: Option<FnEffect>,
    /// A JSON Schema for its arguments; one that takes any object without it.
    #[serde(default)]
    pub params_schema: Option<Schema>,
    /// A JSON Schema for its answer.
    #[serde(default)]
    pub returns_schema: Option<Schema>,
    /// Send each call here as a `rpc.callback` notification, with its arguments in order, and
    /// answer `null`: `{"chan": 0, "ref": …}`, a reference a channel holds. Without one the
    /// call comes to the channel as a request, and its answer is the call's.
    #[serde(default)]
    pub callback: Option<Value>,
    /// `client`: each app that provides it answers the calls made for its own client (its
    /// clipboard, its updater), and others' go to the first app that does. `server` by default:
    /// one channel answers every call.
    #[serde(default)]
    pub runs_on: Option<FnRunsOn>,
}
/// Where a provided function runs: for everyone (`server`), or in each client's own app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FnRunsOn {
    Server,
    Client,
}
/// A JSON Schema, as a provided function takes one: an object, and what it says.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema(pub Value);
impl schemars::JsonSchema for Schema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Schema".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": true,
        })
    }
}
/// What a provided function does: a read, or a write (the default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FnEffect {
    Read,
    Write,
}
/// Provide a function every surface can call (`pumi call git.status`, a script's
/// `pumi.api["git.status"]`): calls to it from any channel come to this one as requests, with
/// the caller in `rpc.caller`, and its answers go back; or, for one with a `callback`, as a
/// `rpc.callback` notification, while the call answers `null`. It appears in `api.getInfo` with
/// the rest. A core function cannot be provided, and nor can one another channel provides as
/// someone else (another plugin, another program): it is theirs until they withdraw it. With
/// `runsOn: "client"` each app provides its own, for its own client. `null` withdraws it; it goes
/// with the channel too.
#[api(
    name = "fn.set",
    example(
        args = r#"["hello.greet", {"desc": "Greet.", "effect": "read"}]"#,
        result = "null",
        desc = "A function the channel answers."
    ),
    example(
        args = r#"["build.log", {"desc": "Log a line.", "paramsSchema": {"type": "object", "properties": {"line": {"type": "string"}}}, "callback": {"chan": 0, "ref": "build/1"}}]"#,
        result = "null",
        desc = "A function run by a callback; the call answers `null`."
    ),
    example(args = r#"["hello.greet", null]"#, result = "null", desc = "Withdrawn.")
)]
pub fn pumi_fn_set(cx: &mut Cx, name: String, spec: Nullable<FnSpec>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let Some(spec) = spec.into_option() else {
        k.withdraw_routes(caller.channel, Some(&name));
        k.emit("registry.changed", "fn", About::default(), json!({ "kind": "fn", "names": [name] }));
        return Ok(());
    };
    if pumi_api::Registry::core(&name).is_some() {
        return Err(Error::invalid_params(format!("`{name}` is the core's")));
    }
    let source = k.source(&caller);
    let callback = match spec.callback {
        Some(value) => {
            let r: ChanRef = serde_json::from_value(value)
                .map_err(|e| Error::invalid_params(format!("`callback` is `{{chan, ref}}`: {e}")))?;
            let chan = if r.chan == 0 { caller.channel } else { r.chan };
            if !k.model.chans.contains_key(&chan) {
                return Err(Error::not_found(format!("no channel {chan}")));
            }
            Some((chan, r.reference))
        }
        None => None,
    };
    let params_schema = spec
        .params_schema
        .map(|schema| schema.0)
        .unwrap_or_else(|| json!({ "type": ["object", "array"], "properties": {}, "additionalProperties": false }));
    let parameters = params_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().map(|name| (String::new(), name.clone())).collect())
        .unwrap_or_default();
    let info = FunctionInfo {
        name,
        since: 1,
        provider: source.clone(),
        runs_on: match spec.runs_on.unwrap_or(FnRunsOn::Server) {
            FnRunsOn::Server => RunsOn::Server,
            FnRunsOn::Client => RunsOn::Client,
        },
        effect: match spec.effect.unwrap_or(FnEffect::Write) {
            FnEffect::Read => Effect::Read,
            FnEffect::Write => Effect::Write,
        },
        platforms: Vec::new(),
        desc: spec.desc,
        parameters,
        params_schema,
        returns: "Object".into(),
        returns_schema: spec.returns_schema.map(|schema| schema.0).unwrap_or_else(|| json!({})),
        examples: vec![ExampleInfo { args: json!([]), result: None, desc: None }],
        notes: Vec::new(),
        source,
        internal: false,
    };
    info.validate()?;
    let chan = callback.as_ref().map_or(caller.channel, |(chan, _)| *chan);
    let name = info.name.clone();
    // Someone else's while they are connected: a channel may move its own, and an app may
    // take over from the app before it (the same source), not from a script or a plugin.
    let others = k.routes.get(&name).into_iter().chain(k.client_routes.get(&name).into_iter().flat_map(|a| a.values()));
    if let Some(theirs) =
        others.filter(|r| r.chan != chan && k.model.chans.contains_key(&r.chan)).find(|r| r.info.source != info.source)
    {
        return Err(Error::invalid_params(format!(
            "`{name}` is provided by {}: it is theirs until they withdraw it",
            theirs.info.source
        )));
    }
    let per_client = info.runs_on == RunsOn::Client;
    let route = Route { chan, info, callback: callback.map(|(_, reference)| reference) };
    if per_client {
        k.client_routes.entry(name.clone()).or_default().insert(chan, route.clone());
        // Everyone else's calls stay with the app that had it, while it does.
        if k.routes.get(&name).is_none_or(|r| r.chan == chan || !k.model.chans.contains_key(&r.chan)) {
            k.routes.insert(name.clone(), route);
        }
    } else {
        k.client_routes.remove(&name);
        k.routes.insert(name.clone(), route);
    }
    k.emit("registry.changed", "fn", About::default(), json!({ "kind": "fn", "names": [name] }));
    Ok(())
}
/// Every function the server has, as `api.getInfo` describes them: the core's, and those
/// channels provide (`fn.set`). Internal ones are left out.
#[api(name = "core.functions", internal, effect = "read", example(args = "[]", result = "[]"))]
pub fn pumi_core_functions(cx: &mut Cx) -> Result<Vec<FunctionInfo>> {
    let mut functions = cx.registry().functions();
    let (_, k) = kernel(cx)?;
    for function in &mut functions {
        function.provider = "core".into();
    }
    functions.retain(|f| !k.routes.contains_key(&f.name));
    functions.extend(k.routes.values().filter(|route| !route.info.internal).map(|route| route.info.clone()));
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(functions)
}
/// The channel calls to functions the core does not know go to: pumi-host answers the functions
/// it has and says which it does not. Without a channel, the caller's own.
#[api(name = "core.setFallback", internal, example(args = r#"[7]"#, result = "null"))]
pub fn pumi_set_fallback(cx: &mut Cx, chan: Option<u64>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    k.fallback = Some(chan.unwrap_or(caller.channel));
    Ok(())
}
/// Send a notification to another channel: how a channel calls a callback another one holds
/// (`rpc.callback`), or tells it anything else.
#[api(name = "channel.notify", internal, example(args = r#"[7, "rpc.callback", ["hello/1", []]]"#, result = "null"))]
pub fn pumi_chan_notify(cx: &mut Cx, chan: u64, method: String, params: Value) -> Result<()> {
    let (_, k) = kernel(cx)?;
    if !k.send(chan, pumi_api::rpc::notification(&method, params)) {
        return Err(Error::not_found(format!("no channel {chan}")));
    }
    Ok(())
}
