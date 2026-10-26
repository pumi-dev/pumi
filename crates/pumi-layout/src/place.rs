use crate::{PaneId, Side};
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// Where a tab goes, as callers write it: `{"split": "right", "size": "30%"}`, `{"pane": 4}`,
/// `{"kind": "view"}`, `{"kind": "float", "width": "60%", "height": 20}`. Without `kind` it is
/// `tiled`: in a pane split from the current tab's. Tab ids are `PaneId`s here, as numbers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceSpec {
    /// `tiled` (the default), `view` (a new view), `session` (a new session), `float`, `slot`,
    /// `detached`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PlaceKind>,
    /// Tiled: the tab whose pane to split; the current tab by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beside: Option<PaneId>,
    /// Tiled: into this pane, as one of its tabs (at `index` in its strip); with `split`, the
    /// pane to split instead of `beside`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<PaneId>,
    /// Tiled: which side of it the new pane goes; `down` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<Side>,
    /// Tiled: the new pane's share, `"30%"` or cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub size: Option<Value>,
    /// Tiled: split the whole view, not one pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    /// Tiled: into this view, beside its active pane. Float: over this view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<u64>,
    /// View: the session the new view goes in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<u64>,
    /// View: its index. Into a pane: where in its strip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// View, session: its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Session: its directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Float: what it floats over: `view` (the default: the view's panes, and only while that view
    /// shows) or `client` (the whole window, whichever view it shows). `pane`, `cursor` and
    /// `mouse` are kept and drawn as `view`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative: Option<String>,
    /// Float: the corner or edge it is pinned to: `NW`, `N`, `NE`, `W`, `center`, `E`, `SW`,
    /// `S`, `SE`. By default the edges `row` and `col` were given for, centred on the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Float: how far in from its anchor's edge: cells, `"24px"` or `"10%"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub row: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub col: Option<Value>,
    /// Float: cells, `"480px"` or a share of what it floats over (`"50%"`, the default); a
    /// share stays one as the window changes size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub width: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub height: Option<Value>,
    /// Float: `none` draws it bare (no frame, no shadow, no background of its own); anything
    /// else is the app's rounded frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    /// Float: it takes the keyboard when it opens (`true` by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,
    /// Float: it takes the pointer, so what is under it does not (`focusable` by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse: Option<bool>,
    /// Float: what closes it, which the app does: `escape` (Escape pressed), `key` (any key),
    /// `press` (a press outside it, the backdrop's included), `scroll` (a scroll outside it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismiss: Option<Vec<String>>,
    /// Float: dims what is under it, from 0 to 100, and takes the presses there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<u8>,
    /// Float: above floats with a lower one (50 by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zindex: Option<i64>,
    /// Moving: trade places with this tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap: Option<PaneId>,
    /// Make it the current tab; `true` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<bool>,
    /// Opening: in this tab, instead of the block it holds. Its old block's engine closes and
    /// the new one opens there, keeping the tab's id, place, focus and options
    /// (`tab.blockChanged`). Said alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<PaneId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    Tiled,
    View,
    Session,
    Float,
}
