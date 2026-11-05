use serde_json::{Value, json};
use shapecraft_doc::Pick;

use super::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "undo",
            label: "Undo",
            menu: "Edit/Undo",
            shortcut: Some("⌘Z"),
            help: "Undo the last step",
            params: "{}",
            modifies: false,
            enabled: |s| s.can_undo(),
            run: |s, _| Ok(json!({ "undone": s.undo_step() })),
        },
        CommandSpec {
            id: "redo",
            label: "Redo",
            menu: "Edit/Redo",
            shortcut: Some("⇧⌘Z"),
            help: "Redo the last undone step",
            params: "{}",
            modifies: false,
            enabled: |s| s.can_redo(),
            run: |s, _| Ok(json!({ "redone": s.redo_step() })),
        },
        CommandSpec {
            id: "edit_step",
            label: "Edit History Step",
            menu: "",
            shortcut: None,
            help: "Change a history step's parameters and rebuild the model",
            params: r#"{"step": n (index into the history), "params": {"distance": 5, …}}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty(),
            run: |s, p| {
                let step = p.f64("step")?;
                let changes: serde_json::Map<String, serde_json::Value> = p.parse("params")?;
                s.edit_step(step.max(0.0) as usize, &changes)?;
                Ok(json!({ "step": step }))
            },
        },
        CommandSpec {
            id: "set_variable",
            label: "Set Variable",
            menu: "",
            shortcut: None,
            help: "Add or change a named value; steps that use it rebuild",
            params: r#"{"name": "width", "value": "40 mm" | 40, "rename": "new_name"?, "kind": "length"|"angle"|"number"?}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let name = p.str("name")?.trim().to_string();
                let value = match p.opt_str("value") {
                    Some(text) => text.trim().to_string(),
                    None => p.f64("value")?.to_string(),
                };
                if value.is_empty() || value.len() > 500 {
                    return Err(EngineError::BadParams("give the variable a value".into()));
                }
                let rename = p.opt_str("rename").map(str::trim);
                let kind = match p.opt_str("kind") {
                    None => None,
                    Some("length") => Some(shapecraft_doc::VarKind::Length),
                    Some("angle") => Some(shapecraft_doc::VarKind::Angle),
                    Some("number") => Some(shapecraft_doc::VarKind::Number),
                    Some(other) => return Err(EngineError::BadParams(format!("kind is length, angle or number, not {other:?}"))),
                };
                s.set_variable(&name, &value, rename)?;
                if let Some(k) = kind
                    && let Some(v) = s.doc.variables.iter_mut().find(|v| v.name == rename.unwrap_or(&name))
                {
                    v.kind = k;
                }
                let shown = rename.unwrap_or(&name).to_string();
                let v = shapecraft_doc::expr::eval(&value, &s.doc.variables).unwrap_or(f64::NAN);
                Ok(json!({ "name": shown, "value": v }))
            },
        },
        CommandSpec {
            id: "delete_variable",
            label: "Delete Variable",
            menu: "",
            shortcut: None,
            help: "Remove a named value (not while other values or steps use it)",
            params: r#"{"name": "width"}"#,
            modifies: true,
            enabled: |s| !s.doc.variables.is_empty(),
            run: |s, p| {
                s.delete_variable(p.str("name")?)?;
                Ok(json!({ "variables": s.doc.variables.len() }))
            },
        },
        CommandSpec {
            id: "delete_step",
            label: "Delete Step",
            menu: "",
            shortcut: None,
            help: "Remove a history step and rebuild the model without it",
            params: r#"{"step": n (index into the history)}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty(),
            run: |s, p| {
                let step = p.f64("step")?;
                s.delete_step(step.max(0.0) as usize)?;
                Ok(json!({ "steps": s.doc.history.len() }))
            },
        },
        CommandSpec {
            id: "suppress_step",
            label: "Suppress Step",
            menu: "",
            shortcut: None,
            help: "Suppress a history step (or bring it back): the model rebuilds without it, and steps that needed it are marked as failing",
            params: r#"{"step": n (index into the history), "suppressed"?: true|false (default: toggle)}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty() || !s.doc.future.is_empty(),
            run: |s, p| {
                let step = p.f64("step")?;
                let on = p.raw("suppressed").and_then(Value::as_bool);
                let now = s.suppress_step(step.max(0.0) as usize, on)?;
                Ok(json!({ "suppressed": now }))
            },
        },
        CommandSpec {
            id: "rename_step",
            label: "Rename Step",
            menu: "",
            shortcut: None,
            help: "Rename a history step",
            params: r#"{"step": n (index into the history), "name": "text"}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty() || !s.doc.future.is_empty(),
            run: |s, p| {
                let step = p.f64("step")?;
                s.rename_step(step.max(0.0) as usize, p.str("name")?)?;
                Ok(json!({}))
            },
        },
        CommandSpec {
            id: "set_breakpoint",
            label: "Insert Breakpoint",
            menu: "",
            shortcut: None,
            help: "Roll the model back to a point in the history: the steps after it wait, and new steps go in there. Without \"before\" the breakpoint goes away",
            params: r#"{"before"?: n (how many steps stay before it; leave out to remove it)}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty() || !s.doc.future.is_empty(),
            run: |s, p| {
                let before = p.raw("before").and_then(Value::as_f64).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1e9) as usize);
                s.set_breakpoint(before)?;
                Ok(json!({ "held": s.doc.future.len().saturating_sub(1) }))
            },
        },
        CommandSpec {
            id: "merge_steps",
            label: "Merge Modeling Steps",
            menu: "",
            shortcut: Some("⌥⇧M"),
            help: "Bake the history into one step (undo brings it back; it can't be split again once the project is closed)",
            params: r#"{"keep_sketches": false, "keep_variables": true}"#,
            modifies: true,
            enabled: |s| s.doc.history.iter().any(|h| !h.quiet) && s.doc.future.is_empty() && s.active_sketch.is_none(),
            run: |s, p| {
                s.merge_steps(p.bool_or("keep_sketches", false), p.bool_or("keep_variables", true))?;
                Ok(json!({ "steps": s.doc.history.len() }))
            },
        },
        CommandSpec {
            id: "restore_merged",
            label: "Merged Steps",
            menu: "",
            shortcut: None,
            help: "The model a Merge Modeling Steps baked (the merged step replays it)",
            params: r#"{"snapshot": {…}}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let snapshot: Value = p.parse("snapshot")?;
                s.restore_merged(&snapshot)?;
                Ok(json!({ "bodies": s.doc.bodies.len() }))
            },
        },
        CommandSpec {
            id: "edit_sketch_step",
            label: "Edit Sketch",
            menu: "",
            shortcut: None,
            help: "Edit a sketch as it was at its step; the steps after it rebuild when you leave the sketch",
            params: r#"{"step": n (index into the history)}"#,
            modifies: true,
            enabled: |s| !s.doc.history.is_empty(),
            run: |s, p| {
                let step = p.f64("step")?;
                let sketch = s.edit_sketch_step(step.max(0.0) as usize)?;
                Ok(json!({ "sketch": sketch.0 }))
            },
        },
        CommandSpec {
            id: "select",
            label: "Select",
            menu: "",
            shortcut: None,
            help: "Select items, faces, edges or sketch regions",
            params: r#"{"picks": [{"kind":"face","body":3,"face":5} | {"kind":"edge",…} | {"kind":"body",…} | {"kind":"region","sketch":2,"region":0} | {"kind":"sketch_entity",…}], "add": false, "toggle": false (with add: a pick already selected is taken out)}"#,
            modifies: false,
            enabled: always,
            run: select,
        },
        CommandSpec {
            id: "select_all",
            label: "Select All",
            menu: "Edit/Select All",
            shortcut: Some("⌘A"),
            help: "Select every visible body",
            params: "{}",
            modifies: false,
            enabled: always,
            run: |s, _| {
                s.selection = s.doc.bodies.iter().filter(|b| b.visible).map(|b| Pick::Body { body: b.id }).collect();
                Ok(json!({ "selected": s.selection.len() }))
            },
        },
        CommandSpec {
            id: "deselect_all",
            label: "Deselect All",
            menu: "",
            shortcut: Some("⇧⌘A"),
            help: "Clear the selection",
            params: "{}",
            modifies: false,
            enabled: always,
            run: |s, _| {
                s.selection.clear();
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "delete",
            label: "Delete",
            menu: "Edit/Delete",
            shortcut: Some("⌫"),
            help: "Delete the selected items or sketch elements",
            params: r#"{"items": [ids]?}  (default: the selection)"#,
            modifies: true,
            enabled: always,
            run: delete,
        },
        CommandSpec {
            id: "rename",
            label: "Rename",
            menu: "Edit/Rename",
            shortcut: Some("⌘R"),
            help: "Rename a body or sketch",
            params: r#"{"item": id, "name": "Bracket"}"#,
            modifies: true,
            enabled: always,
            run: rename,
        },
        CommandSpec {
            id: "new_folder",
            label: "New Folder",
            menu: "",
            shortcut: None,
            help: "Add an Items folder (\"Folder 01\"…), with the given items in it, inside another folder or at an item's place",
            params: r#"{"name": "…"?, "items": [ids]?, "parent": folder id?, "at": id?}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let items = if p.has("items") { p.ids("items")? } else { Vec::new() };
                let parent = p.opt_id("parent").filter(|f| s.doc.folder(*f).is_some() && !items.iter().any(|i| s.doc.is_within(*f, *i)));
                let at = p.opt_id("at").map(|i| s.doc.item_key(i));
                let name = p
                    .opt_str("name")
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && n.len() <= 200)
                    .map_or_else(|| s.doc.next_folder_name(), str::to_string);
                let id = s.doc.alloc_id();
                for f in &mut s.doc.folders {
                    f.items.retain(|i| !items.contains(i));
                }
                let items: Vec<_> = items.into_iter().filter(|i| s.doc.has_item(*i) || s.doc.folder(*i).is_some()).collect();
                s.doc.folders.push(shapecraft_doc::Folder { id, name, items });
                // As observed (s24-021): with a folder picked, the new one goes inside it.
                if let Some(f) = parent.and_then(|f| s.doc.folder_mut(f)) {
                    f.items.push(id);
                }
                if let Some(k) = at {
                    s.doc.set_item_key(id, k);
                }
                Ok(json!({ "folder": id.0 }))
            },
        },
        CommandSpec {
            id: "move_to_folder",
            label: "Move to Folder",
            menu: "",
            shortcut: None,
            help: "Put items in a folder, or take them out (folder: null)",
            params: r#"{"items": [ids], "folder": id | null}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let items = p.ids("items")?;
                let folder = p.opt_id("folder");
                if let Some(f) = folder
                    && s.doc.folder(f).is_none()
                {
                    return Err(EngineError::BadParams(format!("no folder {}", f.0)));
                }
                if let Some(f) = folder
                    && items.iter().any(|i| s.doc.is_within(f, *i))
                {
                    return Err(EngineError::BadParams("a folder can't go inside itself".into()));
                }
                for f in &mut s.doc.folders {
                    f.items.retain(|i| !items.contains(i));
                }
                if let Some(target) = folder.and_then(|f| s.doc.folders.iter().position(|x| x.id == f)) {
                    let keep: Vec<_> = items.into_iter().filter(|i| s.doc.has_item(*i) || s.doc.folder(*i).is_some()).collect();
                    s.doc.folders[target].items.extend(keep);
                }
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "place_item",
            label: "Move in Items",
            menu: "",
            shortcut: None,
            help: "Put an item (or folder) at a place in the Items list: in a folder (or none), between two rows",
            params: r#"{"item": id, "folder": id | null, "after": id?, "before": id?}  (after/before: the rows it lands between)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let item = p.id("item")?;
                if !s.doc.has_item(item) && s.doc.folder(item).is_none() {
                    return Err(EngineError::BadParams(format!("no item {}", item.0)));
                }
                let folder = p.opt_id("folder");
                if let Some(f) = folder {
                    if s.doc.folder(f).is_none() {
                        return Err(EngineError::BadParams(format!("no folder {}", f.0)));
                    }
                    if s.doc.is_within(f, item) {
                        return Err(EngineError::BadParams("a folder can't go inside itself".into()));
                    }
                }
                let (after, before) = (p.opt_id("after").map(|i| s.doc.item_key(i)), p.opt_id("before").map(|i| s.doc.item_key(i)));
                let key = match (after, before) {
                    (Some(a), Some(b)) => (a + b) / 2.0,
                    (Some(a), None) => a + 1.0,
                    (None, Some(b)) => b - 1.0,
                    (None, None) => s.doc.item_key(item),
                };
                for f in &mut s.doc.folders {
                    f.items.retain(|i| *i != item);
                }
                if let Some(f) = folder.and_then(|f| s.doc.folder_mut(f)) {
                    f.items.push(item);
                }
                s.doc.set_item_key(item, key);
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "rename_project",
            label: "Rename Project…",
            menu: "File/Rename Project…",
            shortcut: Some("⌥⌘R"),
            help: "Rename the project",
            params: r#"{"name": "Bracket"}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let name = p.str("name")?.trim();
                if name.is_empty() || name.len() > 200 {
                    return Err(EngineError::BadParams("name must be 1–200 characters".into()));
                }
                s.doc.name = name.to_string();
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "set_appearance",
            label: "Appearance",
            menu: "Item/Appearance",
            shortcut: None,
            help: "Give bodies a look from the library (or your own color), or reset them to grey",
            params: r#"{"items": [ids]?, "appearance": "Aluminum" | {"name", "color": [r,g,b] (0–1), "metal": 0–1, "gloss": 0–1 | "roughness": 0–1, "texture": id?, "scale": 0.05–20?} | null, "custom": material picture id? (instead of appearance)}  (default: the selected bodies)"#,
            modifies: true,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: |s, p| {
                let look: Value = p.parse("appearance").unwrap_or(Value::Null);
                let appearance = if p.has("custom") {
                    let id = p.id("custom")?;
                    Some(
                        super::visual::custom_look(&s.doc, id)
                            .ok_or_else(|| EngineError::BadParams(format!("no custom material {}", id.0)))?,
                    )
                } else {
                    match &look {
                        Value::Null => None,
                        Value::String(name) => Some(
                            shapecraft_doc::Appearance::library()
                                .into_iter()
                                .find(|a| a.full_name().eq_ignore_ascii_case(name) || a.name.eq_ignore_ascii_case(name))
                                .ok_or_else(|| EngineError::BadParams(format!("no appearance called '{name}'")))?,
                        ),
                        v => Some(
                            serde_json::from_value::<shapecraft_doc::Appearance>(roughness_to_gloss(v)?)
                                .map_err(|e| EngineError::BadParams(e.to_string()))?
                                .sanitized()
                                .ok_or_else(|| EngineError::BadParams("appearance values must be finite".into()))?,
                        ),
                    }
                };
                if let Some(t) = appearance.as_ref().and_then(|a| a.texture)
                    && !s.doc.decal_images.iter().any(|i| i.id == t && i.material)
                {
                    return Err(EngineError::BadParams(format!("no custom material {}", t.0)));
                }
                let ids = if p.has("items") {
                    p.ids("items")?
                } else {
                    let mut v = Vec::new();
                    for b in s.selection.iter().filter_map(Pick::body) {
                        if !v.contains(&b) {
                            v.push(b);
                        }
                    }
                    v
                };
                if ids.is_empty() {
                    return Err(EngineError::NotAvailable("select bodies to change their look".into()));
                }
                for id in &ids {
                    let b = s.doc.body_mut(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
                    b.appearance = appearance.clone();
                }
                Ok(json!({ "changed": ids.len() }))
            },
        },
        CommandSpec {
            id: "set_face_color",
            label: "Face Color",
            menu: "Item/Face Color",
            shortcut: None,
            help: "Paint faces a color of their own (or back to their body's)",
            params: r#"{"body": id, "faces": [n], "color": [r,g,b] (0–1) | null}  (default: the selected faces)"#,
            modifies: true,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: |s, p| {
                let color: Option<[f32; 3]> = match p.parse::<Value>("color").unwrap_or(Value::Null) {
                    Value::Null => None,
                    v => {
                        let c: [f32; 3] = serde_json::from_value(v).map_err(|e| EngineError::BadParams(e.to_string()))?;
                        if !c.iter().all(|x| x.is_finite()) {
                            return Err(EngineError::BadParams("color must be finite".into()));
                        }
                        Some(c.map(|x| x.clamp(0.0, 1.0)))
                    }
                };
                let mut targets: Vec<(shapecraft_doc::ItemId, u32)> = Vec::new();
                if p.has("body") {
                    let b = p.id("body")?;
                    for f in p.u32s("faces")? {
                        targets.push((b, f));
                    }
                } else {
                    for pk in &s.selection {
                        if let Pick::Face { body, face } = *pk {
                            targets.push((body, face));
                        }
                    }
                }
                if targets.is_empty() {
                    return Err(EngineError::NotAvailable("select faces to color".into()));
                }
                for (b, f) in &targets {
                    let body = s.doc.body_mut(*b).ok_or_else(|| EngineError::BadParams(format!("no body {}", b.0)))?;
                    body.face_colors.retain(|(x, _)| x != f);
                    if let Some(c) = color {
                        if body.face_colors.len() >= 10_000 {
                            return Err(EngineError::BadParams("too many face colors".into()));
                        }
                        body.face_colors.push((*f, c));
                    }
                }
                Ok(json!({ "changed": targets.len() }))
            },
        },
        CommandSpec {
            id: "show_all",
            label: "Show Hidden Bodies",
            menu: "Item/Show Hidden Bodies",
            shortcut: None,
            help: "Make every hidden body visible again",
            params: "{}",
            modifies: true,
            enabled: |s| s.doc.bodies.iter().any(|b| !b.visible),
            run: |s, _| {
                let mut n = 0;
                for b in s.doc.bodies.iter_mut().filter(|b| !b.visible) {
                    b.visible = true;
                    n += 1;
                }
                Ok(json!({ "shown": n }))
            },
        },
        CommandSpec {
            id: "show_hide",
            label: "Show/Hide",
            menu: "Item/Show/Hide",
            shortcut: Some("⇧⌘H"),
            help: "Toggle visibility of items",
            params: r#"{"items": [ids]?, "visible": bool?}"#,
            modifies: true,
            enabled: always,
            run: show_hide,
        },
        CommandSpec {
            id: "save_view",
            label: "Save View",
            menu: "View/Saved Views",
            shortcut: None,
            help: "Save a camera view to the project (slots 1–8); without \"view\" the slot is cleared",
            params: r#"{"slot": 1-8, "view": {"target": [x,y,z], "yaw": rad, "pitch": rad, "distance": d, "ortho": 0-1, "roll": rad}}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let slot = p.f64("slot")?;
                if !(1.0..=8.0).contains(&slot) {
                    return Err(EngineError::BadParams("slot must be 1–8".into()));
                }
                let view = match p.has("view") {
                    false => None,
                    true => {
                        let v: shapecraft_doc::SavedView = p.parse("view")?;
                        let ok = [v.yaw, v.pitch, v.distance, v.ortho, v.roll].iter().chain(&v.target).all(|x| x.is_finite())
                            && v.distance > 0.0;
                        if !ok {
                            return Err(EngineError::BadParams("view has bad numbers".into()));
                        }
                        Some(v)
                    }
                };
                s.doc.saved_views[slot as usize - 1] = view;
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "set_units",
            label: "Units",
            menu: "View/Units",
            shortcut: None,
            help: "Change the display units (geometry is unchanged)",
            params: r#"{"units": "Millimeter"|"Centimeter"|"Meter"|"Inch"|"Foot"}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                s.doc.units = p.parse("units")?;
                Ok(Value::Null)
            },
        },
    ]
}

fn select(s: &mut Session, p: &Params) -> Result<Value> {
    let picks: Vec<Pick> = p.parse("picks")?;
    let add = p.bool_or("add", false);
    let toggle = add && p.bool_or("toggle", false);
    if !add {
        s.selection.clear();
    }
    for pick in picks {
        match s.selection.iter().position(|x| *x == pick) {
            Some(i) if toggle => {
                s.selection.remove(i);
            }
            Some(_) => {}
            None => s.selection.push(pick),
        }
    }
    s.prune_selection();
    Ok(json!({ "selected": s.selection.len() }))
}

fn targets(s: &Session, p: &Params) -> Result<Vec<shapecraft_doc::ItemId>> {
    if p.has("items") {
        return p.ids("items");
    }
    let mut ids = Vec::new();
    for pick in &s.selection {
        if let Some(id) = match *pick {
            Pick::Body { body } => Some(body),
            Pick::Sketch { sketch } => Some(sketch),
            Pick::Plane { plane } => Some(plane),
            Pick::Axis { axis } => Some(axis),
            Pick::Image { image } => Some(image),
            _ => None,
        } && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn delete(s: &mut Session, p: &Params) -> Result<Value> {
    // Sketch elements first: remove from the highest index down so indices stay valid.
    let mut elements: Vec<(shapecraft_doc::ItemId, u32)> = s
        .selection
        .iter()
        .filter_map(|p| match *p {
            Pick::SketchEntity { sketch, entity } => Some((sketch, entity)),
            _ => None,
        })
        .collect();
    let mut removed = 0;
    if !p.has("items") && !elements.is_empty() {
        elements.sort_by_key(|e| std::cmp::Reverse(e.1));
        elements.dedup();
        for (sk, i) in elements {
            if let Some(sketch) = s.doc.sketch_mut(sk)
                && (i as usize) < sketch.entities.len()
            {
                sketch.entities.remove(i as usize);
                removed += 1;
            }
        }
    }
    for id in targets(s, p)? {
        if s.doc.remove_item(id) {
            removed += 1;
        }
    }
    // Faces: taken out of their body, the neighbours closing over them (Delete Face).
    let mut by_body: Vec<(shapecraft_doc::ItemId, Vec<u32>)> = Vec::new();
    if !p.has("items") {
        for pick in &s.selection {
            if let Pick::Face { body, face } = *pick {
                match by_body.iter_mut().find(|(b, _)| *b == body) {
                    Some((_, faces)) => faces.push(face),
                    None => by_body.push((body, vec![face])),
                }
            }
        }
    }
    for (body, faces) in by_body {
        let Some(b) = s.doc.body(body) else { continue };
        let shape = s.kernel.remove_faces(&b.shape, &faces).map_err(|_| {
            EngineError::NotAvailable("The face can't be deleted because the resulting hole in the body can't be filled.".into())
        })?;
        super::model::set_body_shape(s, body, shape)?;
        removed += 1;
    }
    if removed == 0 {
        return Err(EngineError::BadParams("nothing to delete".into()));
    }
    s.selection.clear();
    Ok(json!({ "deleted": removed }))
}

fn rename(s: &mut Session, p: &Params) -> Result<Value> {
    let id = p.id("item")?;
    let name = p.str("name")?.trim();
    if name.is_empty() || name.len() > 200 {
        return Err(EngineError::BadParams("name must be 1–200 characters".into()));
    }
    if let Some(b) = s.doc.body_mut(id) {
        b.name = name.to_string();
    } else if let Some(sk) = s.doc.sketch_mut(id) {
        sk.name = name.to_string();
    } else if let Some(pl) = s.doc.plane_mut(id) {
        pl.name = name.to_string();
    } else if let Some(ax) = s.doc.axis_mut(id) {
        ax.name = name.to_string();
    } else if let Some(im) = s.doc.image_mut(id) {
        im.name = name.to_string();
    } else if let Some(f) = s.doc.folder_mut(id) {
        f.name = name.to_string();
    } else {
        return Err(EngineError::BadParams(format!("no item {}", id.0)));
    }
    Ok(Value::Null)
}

fn show_hide(s: &mut Session, p: &Params) -> Result<Value> {
    let ids = targets(s, p)?;
    if ids.is_empty() {
        return Err(EngineError::BadParams("select something to show or hide".into()));
    }
    let mut explicit = p.has("visible").then(|| p.bool_or("visible", true));
    // A folder shows or hides everything in it (hidden when anything in it shows).
    let mut expanded = Vec::new();
    for id in ids {
        match s.doc.folder(id) {
            Some(_) => {
                let inside = s.doc.folder_items_deep(id);
                let any_visible = inside.iter().any(|i| item_visible(&s.doc, *i));
                explicit = explicit.or(Some(!any_visible));
                expanded.extend(inside);
            }
            None => expanded.push(id),
        }
    }
    for id in expanded {
        if let Some(b) = s.doc.body_mut(id) {
            b.visible = explicit.unwrap_or(!b.visible);
        } else if let Some(sk) = s.doc.sketch_mut(id) {
            sk.visible = explicit.unwrap_or(!sk.visible);
        } else if let Some(pl) = s.doc.plane_mut(id) {
            pl.visible = explicit.unwrap_or(!pl.visible);
        } else if let Some(ax) = s.doc.axis_mut(id) {
            ax.visible = explicit.unwrap_or(!ax.visible);
        } else if let Some(im) = s.doc.image_mut(id) {
            im.visible = explicit.unwrap_or(!im.visible);
        }
    }
    s.selection.retain(|pick| pick.body().is_none_or(|b| s.doc.body(b).is_some_and(|b| b.visible)));
    Ok(Value::Null)
}

fn item_visible(doc: &shapecraft_doc::Document, id: shapecraft_doc::ItemId) -> bool {
    doc.body(id)
        .map(|b| b.visible)
        .or_else(|| doc.sketch(id).map(|s| s.visible))
        .or_else(|| doc.plane(id).map(|p| p.visible))
        .or_else(|| doc.axes.iter().find(|a| a.id == id).map(|a| a.visible))
        .or_else(|| doc.image(id).map(|i| i.visible))
        .unwrap_or(false)
}

/// A material given by roughness (as the reference's material editor has it) is stored as gloss = 1 − roughness.
/// Keys a material doesn't have are refused rather than quietly dropped.
fn roughness_to_gloss(v: &Value) -> Result<Value> {
    let mut v = v.clone();
    if let Some(m) = v.as_object_mut() {
        if let Some(k) =
            m.keys().find(|k| !matches!(k.as_str(), "name" | "finish" | "color" | "metal" | "gloss" | "roughness" | "texture" | "scale"))
        {
            return Err(EngineError::BadParams(format!("unknown parameter '{k}' in appearance")));
        }
        if let Some(r) = m.remove("roughness") {
            let r = r.as_f64().filter(|r| r.is_finite()).ok_or_else(|| EngineError::BadParams("roughness must be a number".into()))?;
            m.insert("gloss".into(), Value::from((1.0 - r).clamp(0.0, 1.0)));
        }
    }
    Ok(v)
}
