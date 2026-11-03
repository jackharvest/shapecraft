//! The command registry.

use serde_json::Value;

use crate::{Params, Result, Session};

mod construct;
mod edit;
pub(crate) mod file;
pub(crate) mod model;
mod sketch;
mod visual;

pub use sketch::{face_offset, offset_entities};

pub use model::{revolve_auto, set_body_shape};

pub type RunFn = fn(&mut Session, &Params) -> Result<Value>;
pub type EnabledFn = fn(&Session) -> bool;

pub struct CommandSpec {
    /// Stable id used by scripts, the control channel and MCP.
    pub id: &'static str,
    /// What the user sees ("Extrude - One-Sided").
    pub label: &'static str,
    /// Menu path in the reference app's menu bar ("Tools/Extrude").
    pub menu: &'static str,
    /// Shortcut glyphs as shown in menus ("E", "⌘Z", "⇧⌘A").
    pub shortcut: Option<&'static str>,
    /// One-line help, shown in the hint banner and command search.
    pub help: &'static str,
    /// Parameter documentation for agents.
    pub params: &'static str,
    /// Whether a successful run is recorded in undo/history.
    pub modifies: bool,
    pub enabled: EnabledFn,
    pub run: RunFn,
}

pub(crate) fn always(_: &Session) -> bool {
    true
}

pub fn registry() -> &'static [CommandSpec] {
    static ALL: std::sync::OnceLock<Vec<CommandSpec>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v = Vec::new();
        v.extend(edit::commands());
        v.extend(sketch::commands());
        v.extend(model::commands());
        v.extend(construct::commands());
        v.extend(file::commands());
        v.extend(visual::commands());
        v
    })
}

pub fn find(id: &str) -> Option<&'static CommandSpec> {
    registry().iter().find(|c| c.id == id)
}
