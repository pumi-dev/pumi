use super::events::{EventNames, OneOrMany};
use super::kernel;
use crate::kernel::events::check_event;
use crate::kernel::waits::{Wait, WaitedEvent};
use pumi_api::{Cx, Error, Result, api};
use serde::{Deserialize, Serialize};
/// What `event.wait` waits for, beside the event's name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitOpts {
    /// Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's id
    /// for `request.closed`); any match without one.
    #[serde(default, rename = "match")]
    pub matched: Option<OneOrMany>,
    /// Seconds to wait; none: until it fires.
    #[serde(default)]
    pub timeout: Option<f64>,
}
/// Wait for an event, by name or glob: the call answers when one that matches fires, with the
/// event (`{event, match, session, view, tab, data}`), or with `null` when `timeout` passes or
/// the caller goes. A wait that times out is not an error.
#[api(
    name = "event.wait",
    example(
        args = r#"["tab.changed", {"match": "%3", "timeout": 10}]"#,
        desc = "The next change to tab 3, for ten seconds."
    ),
    example(args = r#"["request.closed", {"match": "+7"}]"#, desc = "Until request 7 is answered or closed."),
    note = "In `call.atomic` it does not wait: it answers `null`."
)]
pub fn pumi_event_wait(cx: &mut Cx, event: EventNames, opts: Option<WaitOpts>) -> Result<Option<WaitedEvent>> {
    let events = event.list();
    if events.is_empty() {
        return Err(Error::invalid_params("a wait needs an event"));
    }
    for event in &events {
        check_event(event, true)?;
    }
    let opts = opts.unwrap_or_default();
    let deadline = match opts.timeout {
        None => 0,
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => {
            crate::kernel::requests::now_ms() + ((seconds * 1000.0) as u64).max(1)
        }
        Some(seconds) => return Err(Error::invalid_params(format!("`timeout` is a number of seconds, not {seconds}"))),
    };
    let (_, k) = kernel(cx)?;
    k.waits.add(Wait { events, patterns: opts.matched.map(OneOrMany::list).unwrap_or_default(), deadline });
    // Answered when it ends: the server holds the call. Inside `call.atomic`, which can't be
    // held, it answers `null` now.
    Ok(None)
}
