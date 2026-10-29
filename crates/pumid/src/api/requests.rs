use super::kernel;
use crate::kernel::requests::{self, Reason, Request, RequestAsk, RequestKind};
use crate::kernel::state::parse_rfc3339;
use pumi_api::{Cx, Error, ErrorKind, RequestRef, Result, TabRef, api};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestListOpts {
    /// Only this tab's.
    #[serde(default)]
    pub tab: Option<TabRef>,
    /// Only requests of this kind.
    #[serde(default)]
    pub kind: Option<RequestKind>,
}
/// The open requests, oldest first: every one, or those of `tab` and of `kind`.
#[api(
    name = "request.list",
    effect = "read",
    example(args = r#"[{"tab": "%3"}]"#, result = r#"["+7"]"#),
    example(args = r#"[{"kind": "permission"}]"#, result = "[]")
)]
pub fn pumi_request_list(cx: &mut Cx, opts: Option<RequestListOpts>) -> Result<Vec<RequestRef>> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    let tab = opts.tab.map(|p| k.model.tab_id(&caller, p.0)).transpose()?.map(TabRef);
    Ok(k.requests
        .open
        .values()
        .filter(|r| tab.is_none_or(|p| r.tab == Some(p)) && opts.kind.is_none_or(|kind| r.kind == kind))
        .map(|r| r.id)
        .collect())
}
/// An open request.
#[api(name = "request.get", effect = "read", example(args = r#"["+7"]"#))]
pub fn pumi_request_get(cx: &mut Cx, request: RequestRef) -> Result<Request> {
    let (_, k) = kernel(cx)?;
    k.requests.open.get(&request.0).cloned().ok_or_else(|| Error::not_found(format!("no open request {request}")))
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestOpenOpts {
    /// The tab it is for; a permission has one.
    #[serde(default)]
    pub tab: Option<TabRef>,
    /// When it closes unanswered (`timeout`), RFC 3339. A permission or a question expires in
    /// 300 seconds without; a notification stays until it is closed.
    #[serde(default)]
    pub expires: Option<String>,
    /// Who opens it; the caller's plugin, or its channel's name, without.
    #[serde(default)]
    pub source: Option<String>,
}
/// Put something to the user: a notification (`kind: "notification"`, no `options`), a
/// permission for what runs in `tab` (the `tool` and the `options` to allow or reject it), or a
/// question. Whoever listens to `request.opened` shows it, and anyone may answer it with
/// `request.close(id, {answer})`; the first answer wins, and `request.closed` says how it ended.
/// pumi-host posts notifications to the OS's notification centre and takes them back when they
/// close. Answers the request's id.
#[api(
    name = "request.open",
    example(args = r#"[{"kind": "notification", "title": "Build finished"}]"#, result = r#""+7""#),
    example(
        args = r#"[{"kind": "permission", "title": "Run `make deploy`?", "tool": "Bash", "options": [{"id": "allow", "label": "Allow", "kind": "allow_once"}, {"id": "deny", "label": "Deny", "kind": "reject_once"}]}, {"tab": "%3"}]"#,
        result = r#""+8""#,
        desc = "Ask whether what runs in tab 3 may deploy; wait for `request.closed` with `match: \"+8\"`."
    ),
    example(
        args = r#"[{"kind": "notification", "title": "Tests", "detail": "All 212 passed", "notification": {"group": "tests", "sound": "default"}}]"#,
        result = r#""+9""#,
        desc = "One with the same `group` open is replaced."
    ),
    note = "A permission or a question opened while nothing listens to `request.opened` is refused (`provider_unavailable`): nobody would see it.",
    note = "A permission or a question is cancelled when the channel that opened it closes.",
    note = "At most 256 notifications are open at once; past that, the oldest is closed."
)]
pub fn pumi_request_open(cx: &mut Cx, request: RequestAsk, opts: Option<RequestOpenOpts>) -> Result<RequestRef> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    if request.title.trim().is_empty() {
        return Err(Error::invalid_params("a request has a `title`"));
    }
    requests::check_options(&request).map_err(Error::invalid_params)?;
    if request.notification.is_some() && request.kind != RequestKind::Notification {
        return Err(Error::invalid_params("only a notification has `notification` fields"));
    }
    let tab = opts.tab.map(|p| k.model.tab_id(&caller, p.0)).transpose()?;
    if request.kind == RequestKind::Permission && tab.is_none() {
        return Err(Error::invalid_params("a permission is asked for a tab: give its `tab`"));
    }
    let now = requests::now_ms();
    let expires = match opts.expires.as_deref() {
        Some(text) => {
            let at = parse_rfc3339(text)
                .ok_or_else(|| Error::invalid_params(format!("`expires` is an RFC 3339 time, not `{text}`")))?;
            let at = (at.max(0) as u64) * 1000;
            if at <= now {
                return Err(Error::invalid_params(format!("`expires` ({text}) has passed")));
            }
            Some(at)
        }
        None if request.kind.waits() => Some(now + requests::TIMEOUT * 1000),
        None => None,
    };
    if request.kind.waits() && !requests::heard(k) {
        return Err(Error::new(
            ErrorKind::ProviderUnavailable,
            format!("nothing listens to `request.opened` to show this {}", request.kind.word()),
        ));
    }
    let source = Some(opts.source.unwrap_or_else(|| k.source(&caller)));
    Ok(RequestRef(requests::open(k, request, tab, expires, source, Some(caller.channel))))
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestCloseOpts {
    /// Its answer: `{"option": id}` picks one of its options, `{"text": …}` answers a question in
    /// words (other fields go along). Without, it is closed unanswered.
    #[serde(default)]
    pub answer: Option<Value>,
}
/// Close a request: answer it, or dismiss it without `answer`. A notification takes no answer.
/// The first close wins: a request already closed is not found.
#[api(
    name = "request.close",
    example(args = r#"["+7", {"answer": {"option": "allow"}}]"#, result = "null", desc = "Allow it."),
    example(args = r#"["+9"]"#, result = "null", desc = "Take a notification back.")
)]
pub fn pumi_request_close(cx: &mut Cx, request: RequestRef, opts: Option<RequestCloseOpts>) -> Result<()> {
    let (_, k) = kernel(cx)?;
    let Some(open) = k.requests.open.get(&request.0) else {
        return Err(Error::not_found(format!("no open request {request}")));
    };
    let answer = opts.unwrap_or_default().answer.filter(|a| !a.is_null());
    if let Some(answer) = &answer {
        requests::check_answer(open, answer).map_err(Error::invalid_params)?;
    }
    let reason = if answer.is_some() { Reason::Answered } else { Reason::Closed };
    requests::close(k, request.0, answer.unwrap_or(Value::Null), reason);
    Ok(())
}
