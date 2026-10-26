use super::kernel;
use pumi_api::{Cx, Error, Result, api};
use serde_json::Value;
/// `tab.call` as the core answers it: the methods every block has (`read`, `input`,
/// `selection`; a terminal's `run` and `attach`), and a block's own — a core block's in its
/// type (`respawn`), a plugin block's in the function `<block>.<method>`. The host's `tab.call`
/// adds key names, the program's own report, and the fallback to a file tab's viewer. `resize`
/// (`{width, height}` in cells) is how a tab no client draws is sized: the host's.
#[api(name = "core.tabCall", internal, example(args = r#"[1, "input", {"text": "make test\r"}]"#, result = "null"))]
pub fn pumi_core_tab_call(cx: &mut Cx, tab: u64, method: String, args: Option<Value>) -> Result<Value> {
    let args = args.unwrap_or(Value::Null);
    let caller = cx.caller().clone();
    let tab = {
        let k = cx.host::<crate::kernel::Kernel>()?;
        let tab = k.model.tab_id(&caller, tab)?;
        if !k.model.tabs.contains_key(&tab) {
            return Err(Error::not_found(format!("%{tab} is not open")));
        }
        tab
    };
    // A stream is the core's: it is a client of the tab's own terminal.
    if method == "attach" {
        return crate::attach::open(cx.host::<crate::kernel::Kernel>()?, caller.channel, tab, &args);
    }
    let k = cx.host::<crate::kernel::Kernel>()?;
    match method.as_str() {
        "read" => k.with_instance(tab, |instance, k| instance.read(k, tab, &args)),
        "input" => k.with_instance(tab, |instance, k| instance.input(k, tab, &args).map(|()| Value::Null)),
        "selection" => k.with_instance(tab, |instance, k| instance.selection(k, tab)),
        "resize" => {
            let cells = |name: &str| args.get(name).and_then(Value::as_u64).filter(|n| *n >= 1).map(|n| n as u32);
            let (Some(width), Some(height)) = (cells("width"), cells("height")) else {
                return Err(Error::invalid_params("resize takes `width` and `height` in cells"));
            };
            k.resize_pane(tab, width, height);
            Ok(Value::Null)
        }
        _ => k.with_instance(tab, |instance, k| instance.call(k, tab, &method, args)),
    }
}
/// What a stream's owner sends it (`tab.call(…, "attach")`): `input`, `paste`, `resize`,
/// `scroll`, `mouse` or `detach`. An observer's write is refused before a byte reaches the
/// program.
#[api(name = "core.streamSend", internal, example(args = r#"["stream/1", {"paste": "make test\r"}]"#, result = "null"))]
pub fn pumi_core_stream_send(cx: &mut Cx, stream: String, message: Value) -> Result<Value> {
    let (caller, k) = kernel(cx)?;
    crate::attach::send(k, caller.channel, &stream, message)?;
    Ok(Value::Null)
}
