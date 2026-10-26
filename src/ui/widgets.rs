//! What the native windows draw with besides pumi-ui: text, and icons by name.
pub use pumi_ui::{ICON, StyledExt, icon_file};
const NAMED_ICONS: &[(&str, &str)] = &[
    ("search", "icons/magnifying_glass.svg"),
    ("terminal", "icons/terminal.svg"),
    ("branch", "icons/git_branch.svg"),
    ("git", "icons/git_branch.svg"),
    ("sliders", "icons/settings.svg"),
    ("settings", "icons/settings.svg"),
    ("folder", "icons/folder.svg"),
    ("folder-open", "icons/folder_open.svg"),
    ("dot", "icons/circle.svg"),
    ("ellipsis", "icons/ellipsis.svg"),
    ("sparkle", "icons/sparkle.svg"),
    ("agent", "icons/sparkle.svg"),
    ("sidebar", "icons/threads_sidebar_left_closed.svg"),
    ("extensions", "icons/box.svg"),
    ("plugin", "icons/box.svg"),
    ("blocks", "icons/blocks.svg"),
    ("puzzle", "icons/puzzle.svg"),
    ("zap", "icons/zap.svg"),
    ("keyboard", "icons/keyboard.svg"),
    ("close", "icons/close.svg"),
    ("check", "icons/check.svg"),
    ("plus", "icons/plus.svg"),
    ("minus", "icons/dash.svg"),
    ("person", "icons/person.svg"),
    ("arrow-up", "icons/arrow_up.svg"),
    ("globe", "icons/tool_web.svg"),
    ("code", "icons/code.svg"),
    ("image", "icons/image.svg"),
    ("copy", "icons/copy.svg"),
    ("chevron-down", "icons/chevron_down.svg"),
    ("chevron-right", "icons/chevron_right.svg"),
    ("square-dot", "icons/square_dot.svg"),
    ("square-minus", "icons/square_minus.svg"),
    ("square-plus", "icons/square_plus.svg"),
    ("split", "icons/split.svg"),
    ("split-down", "icons/split_down.svg"),
    ("warning", "icons/warning.svg"),
    ("status-alert", "icons/status_alert.svg"),
    ("status-check", "icons/status_check.svg"),
    ("status-dot", "icons/status_dot.svg"),
    ("status-ring", "icons/status_ring.svg"),
    ("status-spinner", "icons/status_spinner.svg"),
    ("status-unknown", "icons/status_unknown.svg"),
    ("status-x", "icons/status_x.svg"),
];
pub fn named_icon_path(name: &str) -> &'static str {
    named_path(name).unwrap_or("icons/circle.svg")
}
fn named_path(name: &str) -> Option<&'static str> {
    NAMED_ICONS.iter().find(|(known, _)| *known == name).map(|(_, path)| *path)
}
pub fn icon_path(name: &str) -> Option<String> {
    named_path(name).map(str::to_string)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, ICONS};
    use gpui::AssetSource as _;
    #[test]
    fn every_named_icon_ships() {
        for (name, path) in NAMED_ICONS {
            assert!(
                ICONS.iter().any(|(known, _)| known == path),
                "icon {name} points at {path}, which is not embedded"
            );
            assert!(Assets.load(path).unwrap().is_some(), "{path} did not load");
        }
    }
    #[test]
    fn core_icons_ship() {
        for path in [
            "icons/chevron_down.svg",
            "icons/chevron_right.svg",
            "icons/dash.svg",
            "icons/plus.svg",
            "icons/loader_circle.svg",
            "icons/chevron_up_down.svg",
            "icons/folder.svg",
            "icons/folder_open.svg",
            "icons/square_dot.svg",
            "icons/square_minus.svg",
            "icons/square_plus.svg",
            "icons/warning.svg",
            "icons/pumi.svg",
            "icons/split.svg",
            "icons/split_down.svg",
            "icons/maximize.svg",
            "icons/minimize.svg",
        ] {
            assert!(ICONS.iter().any(|(known, _)| *known == path), "{path} is not embedded");
        }
    }
}
