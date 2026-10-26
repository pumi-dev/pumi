use super::events::About;
use super::ops::{StateOpts, get_state};
use super::options::{Decl, Key, Set};
use super::state::{Blob, TabState};
use super::types::Restore;
use super::{Closed, Ids, Kernel, Place, Session, Tab, TabId, View};
use crate::tab::generic::GenericType;
use pumi_api::{Cx, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};
/// The version of `state.json`'s shape: 2 has views of panes of tabs. An older one is set aside.
pub const FORMAT: u32 = 2;
pub const BLOBS_EVERY: Duration = Duration::from_secs(30);
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    pumi_server: u32,
    ids: Ids,
    sessions: Vec<Session>,
    views: Vec<View>,
    tabs: Vec<SavedTab>,
    options: Vec<(Key, BTreeMap<String, Set>)>,
    #[serde(default)]
    decls: Vec<Decl>,
    /// The plugins' blocks, so their tabs restore before pumi-host says them again.
    #[serde(default)]
    plugin_blocks: Vec<GenericType>,
    /// The plugins' traits, for `trait.list` before pumi-host says them again.
    #[serde(default)]
    plugin_traits: Vec<crate::api::traits::TraitInfo>,
    closed: Vec<Closed>,
    /// The open requests (`request.open`): left out when there are none. One that no longer reads
    /// is dropped, not the state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requests: Vec<serde_json::Value>,
    /// The messages waiting for their agents (`agent.queue`), likewise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    queue: Vec<super::queue::QueuedItem>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedTab {
    id: TabId,
    place: Place,
    activity: u64,
    state: TabState,
}
#[derive(Default)]
pub struct Persisted {
    blobs: HashMap<TabId, BTreeMap<String, Blob>>,
    blobs_at: Option<Instant>,
}
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, path)
}
pub fn save(cx: &mut Cx, full: bool) -> Result<()> {
    let (dir, tabs, due) = {
        let kernel = cx.host::<Kernel>()?;
        let Some(dir) = kernel.config.data_dir.clone() else { return Ok(()) };
        if kernel.ext.get::<Persisted>().is_none() {
            kernel.ext.insert(Persisted::default());
        }
        let persisted = kernel.ext.get::<Persisted>().expect("inserted");
        let due = full || persisted.blobs_at.is_none_or(|at| at.elapsed() >= BLOBS_EVERY);
        (dir, kernel.model.tabs.keys().copied().collect::<Vec<_>>(), due)
    };
    let mut saved_tabs = Vec::new();
    for tab in tabs {
        let viewoptions = {
            let kernel = cx.host::<Kernel>()?;
            let all = kernel.global_option("viewoptions").as_str().unwrap_or("").to_string();
            if due { all } else { all.split(',').filter(|v| v.trim() != "scrollback").collect::<Vec<_>>().join(",") }
        };
        let opts = StateOpts { viewoptions: Some(viewoptions), purpose: Some("restore".into()), blobs: None };
        let mut state = match get_state(cx, tab, &opts) {
            Ok(state) => state,
            Err(err) => {
                crate::server::log(&format!("saving %{tab}: {}", err.message));
                continue;
            }
        };
        let kernel = cx.host::<Kernel>()?;
        let persisted = kernel.ext.get_mut::<Persisted>().expect("inserted");
        if due {
            persisted.blobs.insert(tab, state.blobs.clone());
        } else if let Some(previous) = persisted.blobs.get(&tab) {
            for (name, blob) in previous {
                state.blobs.entry(name.clone()).or_insert_with(|| blob.clone());
            }
        }
        let p = &kernel.model.tabs[&tab];
        saved_tabs.push(SavedTab { id: tab, place: p.place.clone(), activity: p.activity, state });
    }
    let kernel = cx.host::<Kernel>()?;
    let options = kernel
        .model
        .options
        .keys()
        .into_iter()
        .filter(|k| matches!(k, Key::Global | Key::Session(_) | Key::View(_)))
        .map(|k| (k, kernel.model.options.local(k)))
        .collect();
    let saved = Saved {
        pumi_server: FORMAT,
        ids: kernel.ids.clone(),
        sessions: kernel.model.sessions.values().cloned().collect(),
        views: kernel.model.views.values().cloned().collect(),
        tabs: saved_tabs,
        options,
        decls: kernel.model.options.foreign_decls(),
        plugin_blocks: kernel.plugin_blocks.values().map(|b| GenericType::clone(b)).collect(),
        plugin_traits: kernel.plugin_traits.clone(),
        closed: kernel.closed.iter().cloned().collect(),
        requests: kernel.requests.saved(),
        queue: kernel.queue.items.values().cloned().collect(),
    };
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|err| Error::exception(err.to_string()))?;
    write_atomic(&dir.join("state.json"), &bytes)
        .map_err(|err| Error::exception(format!("saving the state: {err}")))?;
    kernel.dirty = false;
    let persisted = kernel.ext.get_mut::<Persisted>().expect("inserted");
    persisted.blobs.retain(|tab, _| saved.tabs.iter().any(|p| p.id == *tab));
    if due {
        persisted.blobs_at = Some(Instant::now());
        let keep: HashSet<String> = saved
            .tabs
            .iter()
            .map(|p| &p.state)
            .chain(saved.closed.iter().map(|c| &c.state))
            .flat_map(|s| s.blobs.values().map(|b| b.reference.clone()))
            .collect();
        kernel.blobs.retain(&keep);
    }
    Ok(())
}
pub fn restore(cx: &mut Cx) -> Result<usize> {
    let saved = {
        let kernel = cx.host::<Kernel>()?;
        let Some(dir) = kernel.config.data_dir.clone() else { return Ok(0) };
        let path = dir.join("state.json");
        let Ok(bytes) = std::fs::read(&path) else { return Ok(0) };
        match serde_json::from_slice::<Saved>(&bytes) {
            Ok(saved) if saved.pumi_server == FORMAT => saved,
            Ok(saved) => {
                crate::server::log(&format!(
                    "state.json is version {}; set aside as state.json.old",
                    saved.pumi_server
                ));
                let _ = std::fs::rename(&path, path.with_extension("json.old"));
                return Ok(0);
            }
            Err(err) => {
                crate::server::log(&format!("state.json is unreadable ({err}); set aside as state.json.bad"));
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                return Ok(0);
            }
        }
    };
    {
        let kernel = cx.host::<Kernel>()?;
        kernel.ids = saved.ids;
        kernel.model.sessions = saved.sessions.into_iter().map(|s| (s.id, s)).collect();
        kernel.model.views = saved.views.into_iter().map(|t| (t.id, t)).collect();
        for decl in saved.decls {
            kernel.model.options.restore_decl(decl);
        }
        kernel.plugin_blocks = saved.plugin_blocks.into_iter().map(|b| (b.id.clone(), Rc::new(b))).collect();
        kernel.plugin_traits = saved.plugin_traits;
        for (key, values) in saved.options {
            for (name, set) in values {
                if let Err(err) = kernel.model.options.set(&name, key, set.value, &set.source, true) {
                    crate::server::log(&format!("restoring option {name}: {}", err.message));
                }
            }
        }
        kernel.closed = saved.closed.into_iter().collect();
        kernel.requests.restore(saved.requests);
        kernel.queue.restore(saved.queue);
        for tab in &saved.tabs {
            kernel.model.tabs.insert(
                tab.id,
                Tab { id: tab.id, block: tab.state.block.clone(), place: tab.place.clone(), activity: tab.activity },
            );
        }
    }
    let mut restored = 0;
    for tab in saved.tabs {
        match restore_tab(cx, tab.id, tab.state) {
            Ok(()) => restored += 1,
            Err(err) => {
                crate::server::log(&format!("restoring %{}: {}", tab.id, err.message));
                let kernel = cx.host::<Kernel>()?;
                if let Some(view) = tab.place.view()
                    && let Some(v) = kernel.model.views.get_mut(&view)
                {
                    v.remove(tab.id);
                }
                kernel.model.tabs.remove(&tab.id);
                kernel.model.options.drop_object(Key::Tab(tab.id));
            }
        }
    }
    let kernel = cx.host::<Kernel>()?;
    let views: Vec<_> = kernel.model.views.keys().copied().collect();
    for view in views {
        let tabs = kernel.model.views[&view].tabs();
        for tab in &tabs {
            if !kernel.model.tabs.contains_key(tab) {
                kernel.model.views.get_mut(&view).expect("a view").remove(*tab);
            }
        }
    }
    kernel.prune();
    kernel.dirty = false;
    Ok(restored)
}
fn restore_tab(cx: &mut Cx, tab: TabId, state: TabState) -> Result<()> {
    let kernel = cx.host::<Kernel>()?;
    let block = kernel.block(&state.block)?;
    state.check(&state.block, block.version())?;
    let blobs = state
        .blobs
        .iter()
        .map(|(name, blob)| Ok((name.clone(), kernel.blobs.get(blob)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    kernel.set_option("tab_block", Key::Tab(tab), json!(state.block), "server", true)?;
    for (name, value) in &state.options {
        if !kernel.model.options.is_readonly(name) {
            let _ = kernel.set_option(name, Key::Tab(tab), value.clone(), "state", false);
        }
    }
    let instance = block.open(
        kernel,
        tab,
        &serde_json::Value::Null,
        Some(Restore { data: &state.data, blobs, options: &state.options }),
    )?;
    kernel.set_instance(tab, instance);
    let name = state.block.clone();
    let data = json!({ "tab": pumi_api::TabRef(tab), "block": name, "restored": true });
    kernel.emit("tab.opened", &name, About::tab(tab), data);
    Ok(())
}
