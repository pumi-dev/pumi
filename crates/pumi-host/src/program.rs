use crate::kernel::Kernel;
use crate::kernel::events::Event;
use pumi_proto::osc::{Osc, Scanner};
use pumi_proto::status::{Kind, Record, Records, Sequence, State};
/// The records permissions and questions are kept as: `request/<n>`, of request `+n`.
const REQUEST: &str = "request";
use serde_json::{Value, json};
use std::collections::HashMap;
#[derive(Default)]
struct Tab {
    records: Records,
    scanner: Scanner,
}
#[derive(Default)]
struct Programs(HashMap<u64, Tab>);
pub fn heard(kernel: &mut Kernel, event: &Event) {
    let Some(tab) = event.about.tab else { return };
    let changed = match event.event.as_str() {
        "tab.closed" => {
            kernel.ext.get_or_default::<Programs>().0.remove(&tab);
            return;
        }
        "terminal.osc" => {
            let code = event.data["code"].as_u64().unwrap_or_default();
            let data = event.data["data"].as_str().unwrap_or_default();
            let owner = event.data["fg"].as_i64().map(|fg| fg as i32);
            let shell = shell(kernel, tab);
            let entry = entry(kernel, tab, owner);
            let mut heard = Vec::new();
            entry.scanner.feed(format!("\x1b]{code};{data}\x07").as_bytes(), &mut heard);
            let mut changed = false;
            for osc in heard {
                changed |= match osc {
                    Osc::Status(sequence) => entry.records.apply(sequence, owner),
                    Osc::Progress(progress) => entry.records.progress(progress, owner),
                    Osc::Prompt if owner.is_none() || owner == shell => entry.records.prompt(),
                    _ => false,
                };
            }
            changed
        }
        "tab.changed" => {
            let keys = event.data["keys"].as_array().cloned().unwrap_or_default();
            // Its program ended: the exit code is the change that says so.
            if keys.iter().any(|key| key == "exitCode") {
                known(kernel, tab, None).is_some_and(|entry| entry.records.ended())
            } else if keys.iter().any(|key| key == "title") {
                // A program without OSC 7501: a spinner in its title says it is working.
                let title = event.data["patch"]["title"].as_str().unwrap_or_default().to_string();
                let owner = foreground(kernel, tab).filter(|o| Some(*o) != shell(kernel, tab));
                entry(kernel, tab, owner).records.title(&title, owner)
            } else if keys.iter().any(|key| key == "options.tab_foreground_pgid") {
                // Another process group has the terminal: what the ones before it reported ends.
                let now = event.data["patch"]["options"]["tab_foreground_pgid"].as_i64().map(|g| g as i32);
                let shell = shell(kernel, tab);
                let Some(entry) = known(kernel, tab, None) else { return };
                entry.records.foreground(now);
                let mut changed = false;
                for owner in entry.records.owners() {
                    if Some(owner) != now && Some(owner) != shell {
                        changed |= entry.records.exited(owner);
                    }
                }
                changed
            } else {
                return;
            }
        }
        // A permission or a question pumid keeps for the tab: it is `blocked` until it closes,
        // whoever has the terminal (a record of no process group's).
        "request.opened" if event.data["kind"] != "notification" => {
            let Some(id) = request_number(&event.data) else { return };
            let kind = match event.data["kind"].as_str() {
                Some("question") => Kind::Question,
                _ => Kind::Permission,
            };
            let title = event.data["title"].as_str().map(|t| t.chars().filter(|c| !c.is_control()).take(192).collect());
            let record = Record {
                id: format!("{REQUEST}/{id}"),
                state: State::Blocked,
                kind: Some(kind),
                progress: None,
                app: None,
                title,
                msg: None,
            };
            entry(kernel, tab, None).records.set(Sequence::Report(record), None)
        }
        // A program without OSC 7501 that sends a desktop notification has something for the user.
        "tab.alerted" if matches!(event.data["kind"].as_str(), Some("osc9" | "osc777")) => {
            let message = event.data["message"].as_str().map(str::to_string);
            let owner = foreground(kernel, tab).filter(|o| Some(*o) != shell(kernel, tab));
            entry(kernel, tab, owner).records.notified(message.as_deref(), owner)
        }
        "request.closed" => {
            let Some(id) = request_number(&event.data) else { return };
            known(kernel, tab, None)
                .is_some_and(|e| e.records.set(Sequence::Clear(Some(format!("{REQUEST}/{id}"))), None))
        }
        _ => false,
    };
    if changed {
        publish(kernel, tab);
    }
}
/// The number of the request an event's `data` names (`+7`).
fn request_number(data: &Value) -> Option<u64> {
    data["request"].as_str()?.strip_prefix('+')?.parse().ok()
}
pub fn seen(kernel: &mut Kernel, tab: u64) -> bool {
    let present = kernel.tab_option(tab, "tab_foreground_pgid").as_i64().map(|g| g as i32);
    let changed = known(kernel, tab, present).is_some_and(|entry| entry.records.seen(present));
    if changed {
        publish(kernel, tab);
    }
    changed
}
pub fn report(kernel: &mut Kernel, tab: u64, body: &str) -> Result<bool, String> {
    let sequence = match pumi_proto::status::parse(body) {
        Some(pumi_proto::status::Sequence::Query) | None => {
            return Err(format!("`{body}` is not an OSC 7501 report (`state=working:app=NAME`)"));
        }
        Some(sequence) => sequence,
    };
    let owner = kernel.tab_option(tab, "tab_foreground_pgid").as_i64().map(|g| g as i32);
    let changed = entry(kernel, tab, owner).records.apply(sequence, owner);
    if changed {
        publish(kernel, tab);
    }
    Ok(changed)
}
fn foreground(kernel: &Kernel, tab: u64) -> Option<i32> {
    kernel.tab_option(tab, "tab_foreground_pgid").as_i64().map(|g| g as i32)
}
fn shell(kernel: &Kernel, tab: u64) -> Option<i32> {
    kernel.tab_option(tab, "tab_pid").as_i64().map(|pid| pid as i32)
}
fn entry(kernel: &mut Kernel, tab: u64, owner: Option<i32>) -> &mut Tab {
    let published = kernel.tab_option(tab, "@program_status");
    let owner = owner.filter(|owner| Some(*owner) != shell(kernel, tab));
    kernel.ext.get_or_default::<Programs>().0.entry(tab).or_insert_with(|| {
        let list: Vec<Record> = serde_json::from_value(published).unwrap_or_default();
        Tab { records: Records::restore(list, owner), scanner: Scanner::default() }
    })
}
fn known(kernel: &mut Kernel, tab: u64, owner: Option<i32>) -> Option<&mut Tab> {
    let reported = kernel.ext.get_or_default::<Programs>().0.contains_key(&tab)
        || !kernel.tab_option(tab, "@program_status").is_null();
    reported.then(|| entry(kernel, tab, owner))
}
fn publish(kernel: &mut Kernel, tab: u64) {
    let Some(entry) = kernel.ext.get_or_default::<Programs>().0.get(&tab) else { return };
    let records = entry.records.list();
    let status = entry.records.most_demanding().map(|record| record.state.word());
    let Some(core) = kernel.core.clone() else { return };
    let program = if records.is_empty() { Value::Null } else { json!(records) };
    core.notify("core.setOptionValue", json!(["@program_status", program, { "tab": tab }]));
    core.notify("core.setOptionValue", json!(["@status", status, { "tab": tab }]));
}
#[cfg(test)]
mod tests {
    use pumi_proto::status::{State, parse};
    #[test]
    fn the_most_demanding_report_is_the_tabs_state() {
        let status = |bodies: &[&str]| {
            let mut records = pumi_proto::status::Records::default();
            for body in bodies {
                records.apply(parse(body).unwrap(), None);
            }
            records.most_demanding().map(|r| r.state.word())
        };
        assert_eq!(status(&["state=idle"]), Some("idle"));
        assert_eq!(status(&["state=working:app=x"]), Some("working"));
        assert_eq!(status(&["state=error"]), Some("error"), "a failed program says so");
        assert_eq!(status(&["state=working", "state=done:id=job"]), Some("done"));
        assert_eq!(status(&["state=error:id=a", "state=blocked:id=b:kind=permission"]), Some("blocked"));
        assert_eq!(status(&[]), None);
        assert_eq!(State::Error.word(), "error");
    }
}
