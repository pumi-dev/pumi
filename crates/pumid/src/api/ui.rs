use super::kernel;
use crate::kernel::events::About;
use crate::kernel::options::Key;
use crate::kernel::{ChanId, ClientInfo, Kernel, Ui};
use pumi_api::{Cx, Error, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachOpts {
    /// The session it shows; the current one by default.
    #[serde(default)]
    pub session: Option<u64>,
    /// Its size in cells: what its views are laid out in.
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}
/// What an attaching client starts from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Attached {
    /// Its id: its channel's.
    pub client: u64,
    /// The last event already in `state`; every later one arrives as a `redraw` notification,
    /// `{"method": "redraw", "params": [event, …]}`, in order.
    pub seq: u64,
    /// The whole model: sessions, views, tabs with their places and options, and options.
    pub state: Value,
}
/// Attach as a UI: the channel becomes a client showing a session,
/// is sent the model, then every change to it in order (the attach stream).
#[api(name = "client.attach", internal, example(args = r#"[{"width": 120, "height": 40}]"#))]
pub fn pumi_ui_attach(cx: &mut Cx, opts: Option<AttachOpts>) -> Result<Attached> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    if caller.channel == crate::kernel::SERVER {
        return Err(Error::invalid_params("the core cannot attach to itself"));
    }
    let session = match opts.session {
        Some(s) => k.model.session_id(&caller, s)?,
        None => k.model.current(&caller).map(|c| c.session).unwrap_or(0),
    };
    let (width, height) = (opts.width.unwrap_or(80).max(1), opts.height.unwrap_or(24).max(1));
    let chan = k.model.chans.get_mut(&caller.channel).ok_or_else(|| Error::not_found("no such channel"))?;
    chan.ui = Some(Ui { session, width, height, from_seq: u64::MAX });
    set_size(k, caller.channel, width, height);
    if session != 0 {
        k.touch_session(session);
    }
    k.emit("client.attached", "", About::default(), json!({ "client": caller.channel, "session": session }));
    let seq = k.events.seq();
    if let Some(ui) = k.model.chans.get_mut(&caller.channel).and_then(|c| c.ui.as_mut()) {
        ui.from_seq = seq;
    }
    Ok(Attached { client: caller.channel, seq, state: k.dump() })
}
/// The whole model as `client.attach` sends it, and the last event in it: what a client reads
/// again when it would rather not apply the stream's events itself.
#[api(name = "server.getState", internal, effect = "read", example(args = "[]"))]
pub fn pumi_get_state(cx: &mut Cx) -> Result<Value> {
    let (_, k) = kernel(cx)?;
    Ok(json!({ "seq": k.events.seq(), "state": k.dump() }))
}
/// The core's whole model, as pumi-host's replica holds it: sessions, views, tabs, channels and
/// options, and `events`, the last event whose change it includes. The host reads it again when
/// it may have moved; events reach it as they reach anyone, by `event.on`.
#[api(name = "core.model", internal, effect = "read", example(args = "[]"))]
pub fn pumi_core_model(cx: &mut Cx) -> Result<Value> {
    let (_, k) = kernel(cx)?;
    Ok(json!({ "events": k.events.seq(), "model": k.model }))
}
fn set_size(k: &mut Kernel, chan: ChanId, width: u32, height: u32) {
    let _ = k.set_option("client_width", Key::Client(chan), json!(width), "server", true);
    let _ = k.set_option("client_height", Key::Client(chan), json!(height), "server", true);
}
/// Stop being a UI; the channel stays connected.
#[api(name = "client.detach", internal, example(args = "[]"))]
pub fn pumi_ui_detach(cx: &mut Cx) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    detach(k, caller.channel);
    Ok(())
}
pub fn detach(k: &mut Kernel, chan: ChanId) {
    if k.model.chans.get_mut(&chan).and_then(|c| c.ui.take()).is_none() {
        return;
    }
    k.model.options.drop_object(Key::Client(chan));
    k.emit("client.detached", "", About::default(), json!({ "client": chan }));
}
/// The client's size changed: its views are laid out again.
#[api(name = "client.resize", internal, example(args = "[160, 48]"))]
pub fn pumi_ui_try_resize(cx: &mut Cx, width: u32, height: u32) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let ui = k
        .model
        .chans
        .get_mut(&caller.channel)
        .and_then(|c| c.ui.as_mut())
        .ok_or_else(|| Error::invalid_params("this channel is not attached"))?;
    ui.width = width.max(1);
    ui.height = height.max(1);
    set_size(k, caller.channel, width.max(1), height.max(1));
    Ok(())
}
/// Say who this channel is: `name` is what provenance shows
/// (`set by cli`), `type` is `cli`, `ui`, `plugin`…, and `attributes.tab` (`"%3"`)
/// is the tab its calls are relative to, as `PUMI_TAB` says. `methods` is unused.
#[api(
    name = "channel.setInfo",
    internal,
    example(args = r#"["pumi-cli", {"major": 0}, "cli", {}, {"tab": "%1"}]"#, result = "null")
)]
pub fn pumi_set_client_info(
    cx: &mut Cx,
    name: String,
    version: Value,
    r#type: String,
    methods: Value,
    attributes: Value,
) -> Result<()> {
    drop(methods);
    let (caller, k) = kernel(cx)?;
    let tab = match attributes.get("tab") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => Some(
            s.trim_start_matches('%')
                .parse::<u64>()
                .map_err(|_| Error::invalid_params(format!("attributes.tab is a tab id, not {s:?}")))?,
        ),
        Some(other) => return Err(Error::invalid_params(format!("attributes.tab is a tab id, not {other}"))),
    };
    let chan = k.model.chans.get_mut(&caller.channel).ok_or_else(|| Error::not_found("no such channel"))?;
    chan.info = ClientInfo { name, version, ty: r#type, attributes };
    chan.tab = tab;
    Ok(())
}
/// What the app calls itself (`channel.setInfo`'s name).
pub const APP: &str = "pumi-app";
/// The primary app: the first app still attached as a UI. Plugins run only there; when it
/// detaches, the next app attached is the primary one.
pub fn primary(k: &Kernel) -> Option<ChanId> {
    k.model.chans.values().filter(|c| c.ui.is_some() && c.info.name == APP).map(|c| c.id).min()
}
fn chan_info(k: &Kernel, id: ChanId) -> Value {
    let chan = &k.model.chans[&id];
    json!({
        "id": id,
        "client": { "name": chan.info.name, "version": chan.info.version, "type": chan.info.ty, "attributes": chan.info.attributes },
        "tab": chan.tab,
        "ui": chan.ui.as_ref().map(|ui| json!({ "session": ui.session, "width": ui.width, "height": ui.height })),
        "primary": primary(k) == Some(id),
    })
}
/// Which channels `channel.list` lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListChans {
    /// Only the clients: the channels attached as UIs.
    #[serde(default)]
    pub ui: bool,
}
/// Every channel: each connection, what it said it is, whether it is a UI, and which is the
/// primary app (`primary: true`), the first app attached, which runs every plugin; when it
/// detaches the next app attached is. `ui` lists only the clients.
#[api(
    name = "channel.list",
    internal,
    effect = "read",
    example(args = "[]"),
    example(args = r#"[{"ui": true}]"#, desc = "The clients.")
)]
pub fn pumi_list_chans(cx: &mut Cx, opts: Option<ListChans>) -> Result<Vec<Value>> {
    let (_, k) = kernel(cx)?;
    let ui = opts.unwrap_or_default().ui;
    Ok(k.model
        .chans
        .values()
        .filter(|c| c.id != crate::kernel::SERVER && (!ui || c.ui.is_some()))
        .map(|c| chan_info(k, c.id))
        .collect())
}
/// One channel; `0` is the caller's.
#[api(name = "channel.getInfo", internal, effect = "read", example(args = "[0]"))]
pub fn pumi_get_chan_info(cx: &mut Cx, chan: u64) -> Result<Value> {
    let (caller, k) = kernel(cx)?;
    let chan = if chan == 0 { caller.channel } else { chan };
    if !k.model.chans.contains_key(&chan) {
        return Err(Error::not_found(format!("no channel {chan}")));
    }
    Ok(chan_info(k, chan))
}
