use crate::kernel::Kernel;
use crate::kernel::options::{Key, normalise};
use pumi_api::{Caller, Cx, Error, Result};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::SystemTime;
pub const SOURCE: &str = "settings.json";
pub fn path() -> Option<PathBuf> {
    Some(crate::dirs::config().join("settings.json"))
}
fn read(path: &std::path::Path) -> Result<Map<String, Value>> {
    match std::fs::read(path) {
        Ok(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Ok(Map::new()),
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err(Error::invalid_params(format!("{} is not a JSON object", path.display()))),
            Err(err) => Err(Error::invalid_params(format!("{}: {err}", path.display()))),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(err) => Err(Error::exception(format!("{}: {err}", path.display()))),
    }
}
fn edit(f: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let path = path().ok_or_else(|| Error::exception("no config directory"))?;
    let mut file = read(&path)?;
    f(&mut file);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::exception(format!("{}: {e}", dir.display())))?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(file)).unwrap_or_default() + "\n";
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| Error::exception(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path).map_err(|e| Error::exception(format!("{}: {e}", path.display())))
}
/// What `settings.json` keeps under `key` (`scripts`), if the file has it and reads.
pub fn section(key: &str) -> Option<Value> {
    path().and_then(|p| read(&p).ok()).and_then(|mut file| file.remove(key))
}
/// Replaces what `settings.json` keeps under `key`.
pub fn save_section(key: &str, value: Value) -> Result<()> {
    edit(|file| {
        file.insert(key.to_string(), value);
    })
}
pub fn save_option(name: &str, value: &Value) -> Result<()> {
    edit(|file| {
        let options = file.entry("options").or_insert_with(|| json!({}));
        if !options.is_object() {
            *options = json!({});
        }
        let options = options.as_object_mut().expect("just made an object");
        if value.is_null() {
            options.remove(name);
        } else {
            options.insert(name.to_string(), value.clone());
        }
    })
}
pub fn save_keymap(mode: &str, lhs: &str, to: Option<(&Value, Option<&str>)>) -> Result<()> {
    edit(|file| {
        let keymaps = file.entry("keymaps").or_insert_with(|| json!([]));
        if !keymaps.is_array() {
            *keymaps = json!([]);
        }
        let list = keymaps.as_array_mut().expect("just made an array");
        list.retain(|m| !(m["mode"] == mode && m["lhs"] == lhs));
        if let Some((rhs, desc)) = to {
            let mut m = json!({ "mode": mode, "lhs": lhs, "rhs": rhs });
            if let Some(desc) = desc {
                m["desc"] = json!(desc);
            }
            list.push(m);
        }
    })
}
#[derive(Default)]
struct Applied {
    options: BTreeSet<String>,
    keymaps: BTreeSet<(String, String)>,
    mtime: Option<SystemTime>,
}
fn mtime() -> Option<SystemTime> {
    path().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
}
pub fn poll(cx: &mut Cx) {
    let Ok(k) = cx.host::<Kernel>() else { return };
    let now = mtime();
    if k.ext.get_or_default::<Applied>().mtime == now {
        return;
    }
    apply(cx);
}
pub fn apply(cx: &mut Cx) {
    let now_mtime = mtime();
    let file = match path().map(|p| read(&p)) {
        Some(Ok(file)) => file,
        Some(Err(err)) => {
            if let Ok(k) = cx.host::<Kernel>() {
                k.ext.get_or_default::<Applied>().mtime = now_mtime;
                crate::api::requests::notify_all(k, "error", err.message);
            }
            return;
        }
        None => return,
    };
    let caller = Caller { source: Some(SOURCE.into()), ..cx.caller().clone() };
    let mut warnings = Vec::new();
    let mut options = BTreeSet::new();
    let mut keymaps = BTreeSet::new();
    cx.as_caller(&caller, |cx| {
        for (name, value) in file.get("options").and_then(Value::as_object).cloned().unwrap_or_default() {
            match cx.call("option.set", json!([name, value, { "scope": "global" }])) {
                Ok(_) => {
                    options.insert(name);
                }
                Err(err) => warnings.push(format!("settings.json: {name}: {}", err.message)),
            }
        }
        for m in file.get("keymaps").and_then(Value::as_array).cloned().unwrap_or_default() {
            let (Some(mode), Some(lhs)) = (m["mode"].as_str(), m["lhs"].as_str()) else { continue };
            if m["rhs"].is_null() {
                continue;
            }
            let rhs: crate::callback::Rhs = match serde_json::from_value(m["rhs"].clone()) {
                Ok(rhs) => rhs,
                Err(err) => {
                    warnings.push(format!("settings.json: {mode} {lhs}: {err}"));
                    continue;
                }
            };
            let crate::callback::Rhs::Call(call) = rhs else {
                warnings.push(format!("settings.json: {mode} {lhs}: a key binds a call"));
                continue;
            };
            match crate::api::keymap::set_binding(cx, mode, lhs, call, SOURCE) {
                Ok(_) => {
                    keymaps.insert((mode.to_string(), pumi_lang::keys::normalise(lhs)));
                }
                Err(err) => warnings.push(format!("settings.json: {mode} {lhs}: {}", err.message)),
            }
        }
    });
    let Ok(k) = cx.host::<Kernel>() else { return };
    let before = std::mem::take(k.ext.get_or_default::<Applied>());
    for warning in warnings {
        crate::api::requests::notify_all(k, "warn", warning);
    }
    for name in before.options.difference(&options) {
        let Ok(k) = cx.host::<Kernel>() else { return };
        let ours =
            k.model.options.get(&normalise(name), &[Key::Global]).and_then(|(_, set)| set.map(|s| s.source.clone()));
        if ours.as_deref() == Some(SOURCE) {
            let _ = cx.call("option.set", json!([name, null, { "scope": "global" }]));
        }
    }
    for (mode, lhs) in before.keymaps.difference(&keymaps) {
        let _ = crate::api::keymap::del_binding(cx, mode, lhs, SOURCE);
    }
    if let Ok(k) = cx.host::<Kernel>() {
        *k.ext.get_or_default::<Applied>() = Applied { options, keymaps, mtime: now_mtime };
    }
}
