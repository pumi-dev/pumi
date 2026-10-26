use crate::kernel::Kernel;
use pumi_api::{Caller, Cx, Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
/// `{"chan": 0, "ref": "hello/3"}`: called as a `rpc.callback` notification, `[ref, args]`, on
/// the channel; `0` is the channel that made it. Asynchronous: the answer is `null`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChanRef {
    pub chan: u64,
    #[serde(rename = "ref")]
    pub reference: String,
}
impl ChanRef {
    /// Checked, with a channel of `0` made the caller's.
    pub fn bound(mut self, cx: &mut Cx) -> Result<ChanRef> {
        if self.chan == 0 {
            self.chan = cx.caller().channel;
        }
        if !cx.host::<Kernel>()?.model.chans.contains_key(&self.chan) {
            return Err(Error::not_found(format!("no channel {}", self.chan)));
        }
        Ok(self)
    }
    /// Sends `args` to the channel that holds it.
    pub fn call(&self, cx: &mut Cx, args: Value) -> Result<Value> {
        let kernel = cx.host::<Kernel>()?;
        let message = pumi_api::rpc::notification("rpc.callback", json!([self.reference, args]));
        if !kernel.send(self.chan, message) {
            return Err(Error::new(ErrorKind::ProviderUnavailable, format!("channel {} is gone", self.chan)));
        }
        Ok(Value::Null)
    }
}
/// `{"call": "view.focus", "args": [null, {"direction": "up"}]}`: a function the host calls,
/// as the caller, at the tab the key was pressed in (so `0` is that tab).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub call: String,
    /// Its arguments, positional or by name; none by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}
impl Call {
    /// Only its shape: the function may be a provider's that registers later (a plugin's).
    pub fn check(&self) -> Result<()> {
        if self.call.is_empty() {
            return Err(Error::invalid_params("a call names a function"));
        }
        Ok(())
    }
    pub fn run(&self, cx: &mut Cx, tab: Option<u64>) -> Result<Value> {
        let caller = Caller { tab: tab.or(cx.caller().tab), ..cx.caller().clone() };
        let args = self.args.clone().unwrap_or_else(|| json!([]));
        cx.as_caller(&caller, |cx| cx.call(&self.call, args))
    }
    /// The command line that does the same through `call`, for listings that show one.
    pub fn as_command(&self) -> String {
        let args = self.args.clone().unwrap_or_else(|| json!([]));
        format!("call {} {}", self.call, pumi_lang::cmd::quote(&args.to_string()))
    }
}
/// A key's or a menu item's right-hand side: a command line, a call, or a channel's reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Rhs {
    /// A command line, run by the provider of `cmd.run`.
    Command(String),
    Call(Call),
    Chan(ChanRef),
}
impl Rhs {
    /// Checked, with a reference's channel `0` made the caller's.
    pub fn bound(self, cx: &mut Cx) -> Result<Rhs> {
        match self {
            Rhs::Chan(r) => Ok(Rhs::Chan(r.bound(cx)?)),
            Rhs::Call(c) => {
                c.check()?;
                Ok(Rhs::Call(c))
            }
            other => Ok(other),
        }
    }
}
/// Runs a command line through `cmd.run`; with nobody providing it, says so as an error notice.
pub fn run_command(cx: &mut Cx, line: &str, opts: Value) -> Result<Value> {
    match cx.call("cmd.run", json!([line, opts])) {
        Err(err) if is_unprovided(&err) => {
            let message = "no provider for cmd.run".to_string();
            let k = cx.host::<Kernel>()?;
            crate::api::requests::notify_all(k, "error", format!("{message}: {line}"));
            Err(Error::new(ErrorKind::ProviderUnavailable, message))
        }
        other => other,
    }
}
/// The error of a call nobody provides: the function is unknown, or its provider went away.
fn is_unprovided(err: &Error) -> bool {
    (err.kind == ErrorKind::NotFound && err.message.contains("cmd.run")) || err.kind == ErrorKind::ProviderUnavailable
}
