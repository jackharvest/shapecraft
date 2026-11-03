//! File commands: Save, and the submodules for export, drawings, import, the Shapr3D round trip and measuring.

use std::path::PathBuf;

use serde_json::{Value, json};
use shapecraft_doc::project;

use super::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session};

mod drawing;
mod export;
mod import;
mod measure;
mod shapr;

pub(crate) use shapr::{Followup, OwnHistory, run_followups};

pub(super) fn commands() -> Vec<CommandSpec> {
    let mut v = vec![CommandSpec {
        id: "save",
        label: "Save",
        menu: "File/Save",
        shortcut: Some("⌘S"),
        help: "Save the project folder",
        params: r#"{"path": "/folder"?}  (default: where it was opened from)"#,
        modifies: false,
        enabled: always,
        run: save,
    }];
    v.extend(export::commands());
    v.extend(drawing::commands());
    v.extend(import::commands());
    v.extend(measure::commands());
    v
}

fn path_param(p: &Params, key: &str) -> Result<PathBuf> {
    let s = p.str(key)?;
    if s.is_empty() || s.contains('\0') {
        return Err(EngineError::BadParams(format!("'{key}' must be a file path")));
    }
    Ok(PathBuf::from(s))
}

fn save(s: &mut Session, p: &Params) -> Result<Value> {
    let dir = if p.has("path") {
        path_param(p, "path")?
    } else {
        s.path.clone().ok_or_else(|| EngineError::BadParams("'path' is required for a new project".into()))?
    };
    project::save(&s.doc, s.kernel.as_ref(), &dir)?;
    s.path = Some(dir.clone());
    s.dirty = false;
    Ok(json!({ "path": dir }))
}
