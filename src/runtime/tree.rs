//! A view's panes, as the server lays them out, each with its tabs: what the app draws and drags
//! (in React) and what it sends back as a new ratio; the geometry itself is the theme's.
use serde::{Deserialize, Serialize};
pub type Id = u64;
/// A pane and its tabs, or an app in the layout.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Leaf {
    Pane(Pane),
    App(Id),
}
/// A pane: its id (`:4`), its tabs in its strip's order, and the one in front.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pane {
    pub id: String,
    pub tabs: Vec<String>,
    pub active: Option<String>,
}
impl Leaf {
    /// The pane a leaf is, if it is one (the layout writer's).
    #[cfg(test)]
    pub fn pane(&self) -> Option<&Pane> {
        match self {
            Leaf::Pane(pane) => Some(pane),
            Leaf::App(_) => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Row,
    Column,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Leaf(Leaf),
    Split { axis: Axis, ratio: f32, first: Box<Node>, second: Box<Node> },
}
impl Node {
    pub fn split(axis: Axis, ratio: f32, first: Node, second: Node) -> Node {
        Node::Split { axis, ratio, first: Box::new(first), second: Box::new(second) }
    }
}
