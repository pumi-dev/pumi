use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<u64>,
    pub focused_workspace_id: Option<String>,
    pub focused_view_id: Option<String>,
    pub focused_tab_id: Option<String>,
    pub workspaces: Vec<Workspace>,
    pub views: Vec<View>,
    pub tabs: Vec<Tab>,
    pub layouts: Vec<ViewLayout>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub others: Vec<Tab>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutTab {
    pub tab_id: String,
    pub rect: Rect,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewLayout {
    pub view_id: String,
    pub area: Rect,
    pub tabs: Vec<LayoutTab>,
    pub zoomed: bool,
    pub focused_tab_id: Option<String>,
    pub splits: Vec<LayoutSplit>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSplit {
    pub path: Vec<bool>,
    pub ratio: Option<f32>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub active_view_id: Option<String>,
    pub status: String,
    /// The folder it was opened at: where what starts in it starts (`session_path`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Worktree {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub checkout_path: String,
    pub is_linked_worktree: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub view_id: String,
    pub workspace_id: String,
    pub number: usize,
    /// What the user named it; none (`null`) while it goes by its tabs.
    pub label: Option<String>,
    pub status: String,
    /// The git worktree it works in (its `worktree` option), when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tab {
    pub tab_id: String,
    /// The block it holds: `terminal`, or a plugin's (`owner.name.id`).
    pub block: String,
    /// What it is beside that (`agent`): each trait whose option `@<trait>` it has set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<String>,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub place: serde_json::Value,
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    pub options: serde_json::Map<String, serde_json::Value>,
    pub view_id: String,
    pub workspace_id: String,
    pub status: String,
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground_cwd: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// What the tab's agent says of itself, from its options (`@agent.session`, `@agent.model`,
    /// `@agent.transcript`, `@agent.conversation`): the options are the truth, these their copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_transcript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_conversation: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub program_status: Vec<pumi_proto::status::Record>,
}
impl Snapshot {
    #[cfg(test)]
    pub fn tab(&self, tab_id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.tab_id == tab_id)
    }
    #[cfg(test)]
    pub fn view(&self, view_id: &str) -> Option<&View> {
        self.views.iter().find(|view| view.view_id == view_id)
    }
    #[cfg(test)]
    pub fn workspace(&self, workspace_id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.workspace_id == workspace_id)
    }
    pub fn layout(&self, view_id: &str) -> Option<&ViewLayout> {
        self.layouts.iter().find(|layout| layout.view_id == view_id)
    }
}
impl Tab {
    #[cfg(test)]
    pub fn title(&self) -> &str {
        self.terminal_title_stripped
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .or(self.terminal_title.as_deref())
            .filter(|title| !title.trim().is_empty())
            .or(self.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()))
            .unwrap_or(&self.tab_id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_untitled_tab_reads_its_working_directory() {
        let mut tab = Tab { tab_id: "w16:pA".into(), cwd: Some("/work/pumi".into()), ..Tab::default() };
        assert_eq!(tab.title(), "/work/pumi");
        tab.terminal_title = Some("π - pumi".into());
        assert_eq!(tab.title(), "π - pumi");
        tab.terminal_title_stripped = Some("pumi".into());
        assert_eq!(tab.title(), "pumi");
        tab.terminal_title_stripped = Some("  ".into());
        assert_eq!(tab.title(), "π - pumi");
        tab.terminal_title = None;
        tab.cwd = Some("   ".into());
        assert_eq!(tab.title(), "w16:pA");
    }
    #[test]
    fn empty_fields_stay_off_the_wire() {
        let value = serde_json::to_value(Tab { tab_id: "p".into(), ..Tab::default() }).unwrap();
        assert!(value.get("agent_model").is_none() && value.get("agent_transcript").is_none());
        assert_eq!(value["tab_id"], "p");
    }
}
