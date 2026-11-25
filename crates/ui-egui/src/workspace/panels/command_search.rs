//! ⌘F command search, and running a command by id the way the toolbar does.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::icons::{self, Icon};
use crate::menubar::AppAction;
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::export::Format;
use crate::workspace::toolbar::UiAction;

/// Bookkeeping commands that only scripts, a panel or the History sidebar use stay out of the list: they act on something
/// that panel picked, and have nothing to act on here.
fn searchable(id: &str) -> bool {
    const ELSEWHERE: &[&str] = &[
        "select",
        "inspect",
        "exit_sketching",
        "set_units",
        "edit_step",
        "edit_sketch_step",
        "delete_step",
        "suppress_step",
        "rename_step",
        "set_breakpoint",
        "set_entity",
        "set_variable",
        "delete_variable",
        "save_view",
        "annotate",
        "delete_annotation",
        "update_annotation",
        "delete_drawing",
        "update_drawing",
        "image_settings",
        "set_face_color",
        "delete_constraint",
        "offset_body_edge",
        "constrain",
        "restore_merged",
        "select_text",
        "break_curve",
        "change_sketch_plane",
        "duplicate_sketch",
        "move_to_folder",
        "place_item",
        "plane_size",
        "add_drawing_view",
        "update_drawing_view",
        "delete_drawing_view",
        "rename_decal_image",
        "delete_decal_image",
        "place_decal",
        "update_decal",
        "delete_decal",
        "rename_environment",
        "delete_environment",
        "visual_settings",
    ];
    !ELSEWHERE.contains(&id)
}

/// ⌘F: a popover listing commands with their shortcuts, recent ones first.
pub(super) fn command_search(ws: &mut Workspace, ui: &mut Ui) {
    let Some(search) = ws.search.as_mut() else { return };
    let mut entries: Vec<(&'static str, &'static str, &'static str)> = Vec::new();
    let q = search.query.to_lowercase();
    let registry = shapecraft_engine::registry();
    if q.is_empty() {
        // Only what a person can run from here: the History sidebar's Edit Sketch needs its step, for one.
        for id in ws.recent_commands.iter().filter(|id| searchable(id)) {
            if let Some(c) = registry.iter().find(|c| c.id == *id) {
                entries.push((c.id, c.label, c.shortcut.unwrap_or("")));
            }
        }
    } else {
        // Names starting with the query first, then a word starting with it, then anywhere; each alphabetical
        // (as observed: "e" → Ellipse, Extrude - One-Sided, …).
        let mut found: Vec<(u8, &'static str, &'static str, &'static str)> = Vec::new();
        for c in registry.iter().filter(|c| searchable(c.id)) {
            let l = c.label.to_lowercase();
            let rank = if l.starts_with(&q) {
                0
            } else if l.split([' ', '-', '/']).any(|w| w.starts_with(&q)) {
                1
            } else if l.contains(&q) || c.id.contains(&q) {
                2
            } else {
                continue;
            };
            found.push((rank, c.label, c.id, c.shortcut.unwrap_or("")));
        }
        // What can run now comes first: "Fillet" with an edge selected is the body fillet, not the sketch one.
        let ready = |id: &str| registry.iter().find(|c| c.id == id).is_some_and(|c| (c.enabled)(&ws.session));
        found.sort_by(|a, b| ready(b.2).cmp(&ready(a.2)).then(a.0.cmp(&b.0)).then_with(|| a.1.cmp(b.1)));
        entries.extend(found.into_iter().map(|(_, label, id, keys)| (id, label, keys)));
    }
    entries.truncate(40);
    // Measured (L11): 288 wide, centred on the window, its top 45 px above the middle of the area under the
    // title bar, whatever the pointer; it grows downwards, four rows at most, then it scrolls.
    let w = 288.0;
    let view = ws.view_rect;
    let at = if search.at_pointer {
        // Kept inside the view.
        pos2(
            search.at.x.clamp(view.left(), (view.right() - w).max(view.left())),
            search.at.y.clamp(view.top(), (view.bottom() - 160.0).max(view.top())),
        )
    } else {
        pos2(view.center().x - w / 2.0, view.center().y - 45.0)
    };
    let typing = !search.query.is_empty();
    search.selected = search.selected.min(entries.len().saturating_sub(1));
    let first = search.selected.saturating_sub(3);
    let shown_rows = entries.len().min(4);
    let mut run: Option<&'static str> = None;
    let mut close = false;
    // The click that opened it (the Search button) isn't a click outside.
    let fresh = search.opened_frame;
    let shown = egui::Area::new(ui.scope_id().with("search")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let h = 41.0 + 28.0 * shown_rows as f32 + 5.0;
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        let p = ui.painter();
        let shadow = egui::epaint::Shadow {
            offset: [0, 5],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(if theme::is_light() { 34 } else { 60 }),
        };
        p.add(shadow.as_shape(rect, radius(8)));
        p.rect_filled(rect, radius(8), theme::c(0x2a292a));
        p.rect_stroke(rect, radius(8), Stroke::new(1.0, theme::c(0x444344)), StrokeKind::Inside);
        icons::paint(p, Rect::from_center_size(rect.min + vec2(22.0, 19.0), vec2(18.0, 18.0)), Icon::Search, Tokens::text_dim());
        let font = theme::body(14.5);
        let field = Rect::from_min_size(rect.min + vec2(38.0, 10.0), vec2(w - 70.0, 22.0));
        let edit = egui::TextEdit::singleline(&mut search.query)
            .hint_text(crate::i18n::tr("Search Commands"))
            .frame(egui::Frame::NONE)
            .font(font.clone())
            .desired_width(field.width());
        let resp = ui.put(field, edit);
        if search.opened_frame {
            resp.request_focus();
            search.opened_frame = false;
        }
        let p = ui.painter();
        // The first match's name completes what's typed, in grey (as observed).
        if typing
            && let Some((_, label, _)) = entries.first()
            && label.to_lowercase().starts_with(&search.query.to_lowercase())
            && label.len() > search.query.len()
            && let Some(rest) = label.get(search.query.len()..)
        {
            let typed_w = p.layout_no_wrap(search.query.clone(), font.clone(), Tokens::text()).size().x;
            p.text(
                pos2(field.left() + 2.0 + typed_w, field.center().y - 2.0),
                Align2::LEFT_CENTER,
                rest,
                font.clone(),
                Tokens::text_faint(),
            );
        }
        if typing {
            let c = pos2(rect.right() - 20.0, rect.top() + 19.0);
            let clear = ui.interact(Rect::from_center_size(c, vec2(18.0, 18.0)), ui.scope_id().with("search-clear"), Sense::click());
            let p = ui.painter();
            p.circle_filled(c, 7.0, theme::c(0x8e8d8f));
            for d in [vec2(2.6, 2.6), vec2(2.6, -2.6)] {
                p.line_segment([c - d, c + d], Stroke::new(1.5, theme::c(0x2a292a)));
            }
            if clear.clicked() {
                search.query.clear();
                search.selected = 0;
            }
        }
        let p = ui.painter();
        p.line_segment([pos2(rect.left(), rect.top() + 38.5), pos2(rect.right(), rect.top() + 38.5)], Stroke::new(1.0, theme::c(0x5f5e5f)));
        for (slot, (i, (id, label, keys))) in entries.iter().enumerate().skip(first).take(4).enumerate() {
            let row = Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 41.0 + 28.0 * slot as f32), vec2(w - 8.0, 28.0));
            let r = ui.interact(row, ui.scope_id().with(("cmd", i)), Sense::click());
            let p = ui.painter();
            let lit = typing && i == search.selected;
            if lit {
                p.rect_filled(row.shrink2(vec2(0.0, 1.0)), radius(5), theme::c(0x2d5bd6));
            } else if r.hovered() {
                p.rect_filled(row.shrink2(vec2(0.0, 1.0)), radius(5), theme::c(0x3a393c));
            }
            let ink = if lit { Color32::WHITE } else { Tokens::text() };
            if let Some(icon) = crate::workspace::toolbar::icon_for_label(label) {
                icons::paint(p, Rect::from_center_size(pos2(rect.left() + 21.0, row.center().y), vec2(18.0, 18.0)), icon, ink);
            }
            p.text(pos2(rect.left() + 38.0, row.center().y), Align2::LEFT_CENTER, crate::i18n::tr_s(*label), theme::body(13.0), ink);
            p.text(
                pos2(rect.right() - 12.0, row.center().y),
                Align2::RIGHT_CENTER,
                theme::key_text(keys),
                theme::body(11.5),
                Tokens::text_faint(),
            );
            if r.clicked() {
                run = Some(id);
            }
        }
        if entries.len() > 4 {
            // A thin scroll thumb on the right, as observed.
            let track = Rect::from_min_max(pos2(rect.right() - 7.0, rect.top() + 42.0), pos2(rect.right() - 3.0, rect.bottom() - 4.0));
            let frac = 4.0 / entries.len() as f32;
            let start = first as f32 / entries.len() as f32;
            let thumb =
                Rect::from_min_size(pos2(track.left(), track.top() + track.height() * start), vec2(track.width(), track.height() * frac));
            ui.painter().rect_filled(thumb, radius(2), theme::c(0x8e8d8f));
        }
        let (enter, esc, down, up) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
            )
        });
        if down {
            search.selected = (search.selected + 1).min(entries.len().saturating_sub(1));
        }
        if up {
            search.selected = search.selected.saturating_sub(1);
        }
        if enter {
            run = entries.get(search.selected).map(|e| e.0);
            close = run.is_none();
        }
        if esc {
            close = true;
        }
    });
    if !fresh && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !shown.response.rect.contains(p))) {
        close = true;
    }
    if let Some(id) = run {
        ws.search = None;
        run_by_id(ws, id);
    } else if close {
        ws.search = None;
    }
}

/// Command search entries go through the same paths as the toolbar.
pub(in crate::workspace) fn run_command_id(ws: &mut Workspace, id: &'static str) {
    run_by_id(ws, id);
}

/// The interactive tool a command is run through when picked from search (it needs values a click or a typed
/// number gives), as its toolbar button would start it.
fn tool_for(id: &str) -> Option<crate::workspace::toolbar::ToolKind> {
    use crate::workspace::toolbar::ToolKind as K;
    Some(match id {
        "sketch" => K::Sketch,
        "line" => K::Line,
        "rectangle" => K::Rectangle,
        "circle" => K::Circle,
        "polygon" => K::Polygon,
        "slot" => K::Slot,
        "extend" => K::Extend,
        "arc" => K::Arc,
        "ellipse" => K::Ellipse,
        "spline" => K::Spline,
        "text" => K::Text,
        "trim" => K::Trim,
        "offset_edge" => K::OffsetEdge,
        "project" | "project_sketch" => K::Project,
        "sketch_fillet" => K::SketchFillet,
        "extrude" => K::Extrude,
        "fillet" => K::Fillet,
        "chamfer" => K::Chamfer,
        "shell" => K::Shell,
        "revolve" => K::Revolve,
        "loft" => K::Loft,
        "sweep" => K::Sweep,
        "fill" => K::Fill,
        "offset_face" => K::OffsetFace,
        "replace_face" => K::ReplaceFace,
        "thread" => K::Thread,
        "project_edge" => K::ProjectEdge,
        "split_body" => K::SplitBody,
        "wrap_emboss" => K::WrapEmboss,
        "rotate" | "move_edge" | "move_faces" => K::Move,
        "translate" => K::Translate,
        "mirror" => K::Mirror,
        "pattern" => K::Pattern,
        "pattern_circular" => K::CircularPattern,
        "scale" => K::Scale,
        "align" => K::Align,
        "union" => K::Union,
        "subtract" => K::Subtract,
        "intersect" => K::Intersect,
        "offset_plane" => K::OffsetPlane,
        "plane_3points" => K::Plane3,
        "plane_through_edge" => K::PlaneEdge,
        "plane_on_curve" => K::PlaneCurve,
        "plane_perp_edge" => K::PlanePerpEdge,
        "plane_parallel_face" => K::PlaneParFace,
        "axis_2points" => K::Axis2,
        _ => return None,
    })
}

/// What a command that needs a file, a name or a place is run through when it's picked from search: the window or
/// mode its menu item opens. Run bare, it would only complain about the parameter it didn't get (#98).
fn ui_for(id: &str) -> Option<UiAction> {
    Some(match id {
        "import_file" => UiAction::InsertFile,
        "import_project" => UiAction::InsertProject,
        "insert_image" | "insert_sheet_image" => UiAction::InsertImage,
        "import_decal" => UiAction::ImportDecal,
        "import_material" => UiAction::ImportMaterial,
        "measure" => UiAction::Measure,
        "create_drawing" => UiAction::CreateDrawing,
        "rename" => UiAction::Rename,
        "rename_project" => UiAction::RenameProject,
        _ => return None,
    })
}

/// The Export dialog's format for an export command.
fn export_for(id: &str) -> Option<Format> {
    Some(match id {
        "export_shapr" => Format::Shapr,
        "export_parasolid" => Format::Parasolid,
        "export_step" => Format::Step,
        "export_stl" => Format::Stl,
        "export_obj" => Format::Obj,
        "export_3mf" => Format::ThreeMf,
        "export_glb" => Format::Glb,
        "export_fbx" => Format::Fbx,
        "export_vrml" => Format::Vrml,
        "export_usdz" => Format::Usdz,
        "export_iges" => Format::Iges,
        "export_sketch" => Format::DxfSketch,
        "export_drawing" => Format::DxfDrawing,
        _ => return None,
    })
}

fn run_by_id(ws: &mut Workspace, id: &'static str) {
    if let Some(format) = export_for(id) {
        ws.app_request = Some(AppAction::ExportAs(format));
    } else if id == "save" {
        ws.app_request = Some(AppAction::Save);
    } else if id == "import_environment" {
        ws.wants_environment = true;
    } else if let Some(u) = ui_for(id) {
        ws.ui_action(u);
    } else if let Some(kind) = tool_for(id) {
        ws.start_tool(kind);
    } else {
        let _ = ws.run(id, json!({}));
    }
}

/// Renaming starts with the whole name selected, so typing replaces it.
pub(crate) fn select_all(ctx: &egui::Context, id: egui::Id, len: usize) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len));
        state.cursor.set_char_range(Some(range));
        state.store(ctx, id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shapecraft_doc::Pick;

    use super::*;
    use crate::workspace::toolbar::{self, Action, Category};

    fn workspace() -> Workspace {
        let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Search");
        Workspace::new(session, crate::settings::Settings::default())
    }

    /// The same with a box, a plane and a sketch, and this picked.
    fn working(pick: usize) -> Workspace {
        let mut ws = workspace();
        let mut run = |id: &str, p: serde_json::Value| ws.session.execute(id, &p).unwrap().value;
        let sketch = run("sketch", json!({ "plane": "xy" }))["sketch"].clone();
        run("rectangle", json!({ "corner": [0, 0], "opposite": [10, 10] }));
        run("extrude", json!({ "sketch": sketch, "region": 0, "distance": 10 }));
        run("offset_plane", json!({ "from": "xy", "distance": 30 }));
        run("sketch", json!({ "plane": "xy" }));
        run("line", json!({ "a": [20, 0], "b": [30, 0] }));
        let body = ws.session.doc.bodies[0].id;
        let pick = match pick {
            0 => None,
            1 => Some(Pick::Body { body }),
            2 => Some(Pick::Face { body, face: 0 }),
            3 => Some(Pick::Edge { body, edge: 0 }),
            _ => Some(Pick::Plane { plane: ws.session.doc.planes[0].id }),
        };
        if pick.is_some() {
            ws.session.execute("exit_sketching", &json!({})).unwrap();
        }
        ws.session.selection = pick.into_iter().collect();
        ws
    }

    /// Nothing a person can pick answers with a script's complaint about a parameter ("'path' must be a string", #98).
    fn complaint(ws: &Workspace) -> Option<String> {
        let mut said = ws.toasts.iter().filter(|t| t.failure.is_some()).map(|t| t.text.clone());
        said.find(|t| t.starts_with('\'') || t.contains(" must be ") || t.contains("missing"))
    }

    #[test]
    fn insert_file_and_project_open_their_windows() {
        for (label, wanted) in [("File", AppAction::Import), ("Project", AppAction::ImportProject)] {
            let mut ws = workspace();
            let def = toolbar::flyout(Category::Insert).iter().find(|d| d.label == label).copied().unwrap();
            ws.act(def.action, def.label);
            assert_eq!(ws.app_request, Some(wanted), "{label}");
            assert_eq!(complaint(&ws), None);
        }
    }

    #[test]
    fn nothing_in_search_asks_for_a_parameter() {
        let mut bad = Vec::new();
        for c in shapecraft_engine::registry().iter().filter(|c| searchable(c.id)) {
            for pick in 0..5 {
                let mut ws = working(pick);
                run_by_id(&mut ws, c.id);
                bad.extend(complaint(&ws).map(|m| (c.id, pick, m)));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn no_button_asks_for_a_parameter() {
        let mut bad = Vec::new();
        for c in [Category::Insert, Category::Construct, Category::Plane, Category::Axis, Category::Transform, Category::Tools] {
            for d in toolbar::flyout(c).iter().filter(|d| matches!(d.action, Action::Command(_))) {
                for pick in 0..5 {
                    let mut ws = working(pick);
                    ws.act(d.action, d.label);
                    bad.extend(complaint(&ws).map(|m| (d.label, pick, m)));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
