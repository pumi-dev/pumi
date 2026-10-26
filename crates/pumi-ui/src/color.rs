use crate::theme::token;
use gpui::Rgba;
macro_rules! tokens {
    ($($name:ident => $token:literal),* $(,)?) => {
        $(pub fn $name() -> Rgba { token($token) })*
    };
}
tokens! {
    background => "editor",
    foreground => "surface_text",
    muted_foreground => "muted",
    placeholder => "placeholder",
    popover => "surface",
    border => "border",
    input => "border",
    input_hover => "input_hover",
    ring => "focus_ring",
    selection => "selection",
    accent => "hover",
    accent_active => "active",
    list_hover => "hover",
    list_active => "hover",
    danger => "error",
    danger_hover => "danger_hover",
    danger_active => "danger_active",
    danger_foreground => "danger_foreground",
    warning => "warning",
    success => "success",
    primary => "primary",
    primary_hover => "primary_hover",
    primary_active => "primary_active",
    primary_foreground => "primary_foreground",
    overlay => "overlay",
    track => "element",
    thumb => "surface_text",
    text => "text",
    chrome_muted => "chrome_muted",
    chrome_accent => "accent",
}
pub fn transparent() -> Rgba {
    Rgba { r: 0., g: 0., b: 0., a: 0. }
}
pub fn radius_sm() -> f32 {
    crate::theme::metric("radius_sm")
}
pub fn radius() -> f32 {
    crate::theme::metric("radius")
}
pub fn radius_lg() -> f32 {
    crate::theme::metric("radius_lg")
}
pub fn border_width() -> f32 {
    crate::theme::metric("border")
}
