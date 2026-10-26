//! The icons the app's native windows (Settings) draw, embedded: `icons/<name>.svg`.
use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;
pub struct Assets;
macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        pub const ICONS: &[(&str, &[u8])] = &[
            $((
                concat!("icons/", $name, ".svg"),
                include_bytes!(concat!("../assets/icons/", $name, ".svg")),
            ),)*
        ];
    };
}
icons![
    "arrow_left",
    "arrow_right",
    "arrow_up",
    "arrow_up_right",
    "blocks",
    "box",
    "check",
    "chevron_down",
    "chevron_right",
    "chevron_up_down",
    "circle",
    "close",
    "code",
    "copy",
    "dash",
    "ellipsis",
    "folder",
    "folder_open",
    "git_branch",
    "pumi",
    "image",
    "keyboard",
    "layout_panel_bottom",
    "layout_sidebar_left",
    "layout_sidebar_right",
    "link",
    "loader_circle",
    "lock",
    "lock_off",
    "magnifying_glass",
    "maximize",
    "minimize",
    "person",
    "plus",
    "puzzle",
    "rotate_ccw",
    "rotate_cw",
    "settings",
    "sidebar_close_left",
    "sidebar_close_right",
    "sparkle",
    "split",
    "split_down",
    "square_dot",
    "square_minus",
    "square_plus",
    "status_alert",
    "status_check",
    "status_dot",
    "status_ring",
    "status_spinner",
    "status_unknown",
    "status_x",
    "terminal",
    "threads_sidebar_left_closed",
    "tool_web",
    "warning",
    "zap",
];
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS.iter().find(|(name, _)| *name == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| SharedString::from(*name)).collect())
    }
}
