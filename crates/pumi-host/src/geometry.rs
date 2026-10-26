use crate::kernel::Kernel;
use crate::kernel::events::Event;
use serde_json::json;
const RELAYOUT: &[&str] = &["view.opened", "tab.opened", "session.opened", "client.detached", "core.started"];
/// The fields whose change lays panes out again.
const RELAYOUT_KEYS: &[&str] = &["layout", "zoom", "place", "session", "index"];
pub fn follow(k: &Kernel, event: &Event) {
    let changed = matches!(event.event.as_str(), "view.changed" | "tab.changed")
        && event.data["keys"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key.as_str().is_some_and(|key| RELAYOUT_KEYS.contains(&key))));
    let relayout = RELAYOUT.contains(&event.event.as_str())
        || changed
        || (event.event == "option.changed" && event.matched == "default_size");
    if relayout {
        size_headless(k);
    }
}
pub fn size_headless(k: &Kernel) {
    if k.model.chans.values().any(|c| c.ui.is_some()) {
        return;
    }
    let Some(core) = &k.core else { return };
    for view in k.model.views.keys() {
        for (tab, rect) in k.view_rects(*view) {
            let has = (k.tab_option(tab, "pane_width"), k.tab_option(tab, "pane_height"));
            if has != (json!(rect.width), json!(rect.height)) {
                let size = json!({ "width": rect.width, "height": rect.height });
                core.notify("core.tabCall", json!([tab, "resize", size]));
            }
        }
    }
}
