#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Marks {
    #[default]
    Icons,
    Dots,
}
impl Marks {
    pub fn parse(value: &str) -> Self {
        match value {
            "dots" => Marks::Dots,
            _ => Marks::Icons,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub icon: &'static str,
    pub color: &'static str,
    pub motion: &'static str,
}
pub const STATES: &[&str] = &["blocked", "error", "done", "working", "idle"];
pub fn mark(status: &str, marks: Marks) -> Mark {
    let mark = |icon, color, motion| Mark { icon, color, motion };
    match (marks, status) {
        (Marks::Icons, "working") => mark("status-spinner", "agent.working", "spin"),
        (Marks::Icons, "blocked") => mark("status-alert", "agent.blocked", "pulse"),
        (Marks::Icons, "error") => mark("status-x", "agent.error", ""),
        (Marks::Icons, "done") => mark("status-check", "agent.done", ""),
        (Marks::Dots, "working") => mark("status-dot", "agent.working", "pulse"),
        (Marks::Dots, "blocked" | "error" | "done") => mark("status-dot", agent_token(status), ""),
        (_, "idle") => mark("status-ring", "muted", ""),
        _ => mark("status-unknown", "faint", ""),
    }
}
pub fn agent_token(status: &str) -> &'static str {
    match status {
        "blocked" => "agent.blocked",
        "error" => "agent.error",
        "done" => "agent.done",
        "working" => "agent.working",
        "idle" => "agent.idle",
        _ => "agent.unknown",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_state_has_its_own_mark() {
        let shapes: Vec<&str> =
            STATES.iter().chain(["unknown"].iter()).map(|state| mark(state, Marks::Icons).icon).collect();
        let mut distinct = shapes.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), shapes.len(), "{shapes:?}");
        assert_eq!(mark("working", Marks::Icons).motion, "spin");
        assert_eq!(mark("blocked", Marks::Icons).motion, "pulse");
        assert_eq!(mark("idle", Marks::Icons).color, "muted", "idle is quiet");
        for state in ["blocked", "error", "done"] {
            assert_eq!(mark(state, Marks::Dots).icon, "status-dot", "{state} in dots");
        }
    }
}
pub fn state_mark(
    id: impl Into<gpui::SharedString>,
    status: &str,
    icon: &str,
    color: Option<gpui::Rgba>,
    marks: Marks,
    size: f32,
) -> gpui::AnyElement {
    use gpui::{IntoElement, Styled, px, svg};
    if icon == "none" {
        return gpui::div().flex_none().size(px(size)).into_any_element();
    }
    let mut mark = mark(status, marks);
    let name = match icon {
        "" => mark.icon,
        icon => {
            if icon != "status-spinner" && mark.motion == "spin" {
                mark.motion = "";
            }
            icon
        }
    };
    let color = color.unwrap_or_else(|| crate::theme::token(mark.color));
    let Some(path) = crate::theme::icon(name).or_else(|| crate::theme::icon("status-unknown")) else {
        return gpui::div().flex_none().size(px(size)).into_any_element();
    };
    crate::icon::moving(svg().path(path).flex_none().size(px(size)).text_color(color), mark.motion, id)
}
