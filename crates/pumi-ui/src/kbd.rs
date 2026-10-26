use crate::ThemedBorder;
use gpui::{App, SharedString, Window, div, prelude::*, px};

use super::color;
use crate::StyledExt;

#[derive(IntoElement)]
pub struct Kbd {
    chord: SharedString,
    bare: bool,
}

impl Kbd {
    pub fn new(chord: impl Into<SharedString>) -> Self {
        Self { chord: chord.into(), bare: false }
    }

    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let glyphs = chord_glyphs(&self.chord);
        if self.bare {
            return div().flex_none().text(11., color::muted_foreground()).child(glyphs);
        }
        div()
            .flex_none()
            .min_w(px(18.))
            .px(px(4.))
            .py(px(1.))
            .rounded(px(color::radius_sm()))
            .border_themed()
            .border_color(color::border())
            .bg(color::background())
            .text(12., color::muted_foreground())
            .text_center()
            .child(glyphs)
    }
}

pub fn chord_glyphs(chord: &str) -> String {
    chord.split(' ').map(stroke_glyphs).collect::<Vec<_>>().join(" ")
}

fn stroke_glyphs(chord: &str) -> String {
    let mut out = String::new();
    let parts: Vec<&str> = chord.split('+').collect();
    for (index, part) in parts.iter().enumerate() {
        let last = index + 1 == parts.len();
        out.push_str(match *part {
            "ctrl" if !last => "⌃",
            "alt" if !last => "⌥",
            "shift" if !last => "⇧",
            "cmd" if !last => "⌘",
            "enter" => "↵",
            "tab" => "⇥",
            "escape" => "⎋",
            "backspace" => "⌫",
            "space" => "Space",
            "left" => "←",
            "right" => "→",
            "up" => "↑",
            "down" => "↓",
            "minus" => "-",
            "plus" => "+",
            "equal" => "=",
            "slash" => "/",
            "delete" => "⌦",
            "home" => "↖",
            "end" => "↘",
            "pageup" => "⇞",
            "pagedown" => "⇟",
            other => {
                if other.chars().count() == 1 {
                    out.push_str(&other.to_uppercase());
                    continue;
                }
                other
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::chord_glyphs;

    #[test]
    fn glyphs() {
        assert_eq!(chord_glyphs("shift+cmd+["), "⇧⌘[");
        assert_eq!(chord_glyphs("ctrl+b"), "⌃B");
        assert_eq!(chord_glyphs("1..9"), "1..9");
        assert_eq!(chord_glyphs("cmd+escape"), "⌘⎋");
        assert_eq!(chord_glyphs("ctrl+b shift+v"), "⌃B ⇧V");
        assert_eq!(chord_glyphs("cmd+slash"), "⌘/");
    }
}
