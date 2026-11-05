//! The toolbar and menu path for every action (`act`, `ui_action`), Escape, and Delete.

use egui::{Pos2, vec2};
use glam::DVec3;
use serde_json::json;
use shapecraft_doc::{ItemId, Pick};
use shapecraft_render::Camera;

use super::toolbar::{Action, UiAction};
use super::tools::{self, Tool};
use super::{RightPop, Search, Section, Workspace, annotate, drawings, sheet_views};

impl Workspace {
    /// The grid's spacing now: the locked one, else what the zoom gives.
    pub(crate) fn grid_size(&self) -> f64 {
        self.grid_locked.unwrap_or_else(|| shapecraft_render::grid_step(self.camera.world_per_pixel(self.size().y)))
    }

    /// The toolbar button / shortcut path for every action.
    pub fn act(&mut self, action: Action, label: &str) {
        self.flyout = None;
        self.flyout_sub = None;
        match action {
            Action::Soon => self.toast(format!("{label} is coming soon")),
            Action::Command(id) => {
                let _ = self.run(id, json!({}));
            }
            Action::Ui(u) => self.ui_action(u),
            Action::Flyout(cat) => {
                self.flyout = Some((cat, Pos2::ZERO));
                self.popup_opened_at = self.now;
            }
            Action::Tool(kind) => self.start_tool(kind),
        }
    }

    pub fn ui_action(&mut self, u: UiAction) {
        match u {
            UiAction::NearestOrtho => self.nearest_ortho_view(),
            UiAction::RotateView(i) => self.rotate_view(i % 4, i >= 4),
            UiAction::GridPosition(i) => self.grid_plane = (i as usize).min(2),
            UiAction::ToggleGrid => self.view_opts.grid = !self.view_opts.grid,
            UiAction::ToggleAxes => self.view_opts.axes = !self.view_opts.axes,
            UiAction::ToggleEdges => self.view_opts.edges = !self.view_opts.edges,
            UiAction::ToggleHiddenEdges => self.view_opts.hidden_edges = !self.view_opts.hidden_edges,
            UiAction::ToggleDecals => self.view_opts.decals = !self.view_opts.decals,
            UiAction::ToggleSketches => self.view_opts.sketches = !self.view_opts.sketches,
            UiAction::Units(i) => {
                if let Some(name) = ["Millimeter", "Centimeter", "Meter", "Inch", "Foot"].get(i as usize) {
                    let _ = self.run("set_units", json!({ "units": name }));
                }
            }
            UiAction::FeetArchitectural(on) => self.feet_architectural = on,
            UiAction::DegreesFractional(on) => self.degrees_fractional = on,
            UiAction::LockGrid => {
                let step = self.grid_size();
                self.grid_locked = if self.grid_locked.is_some() { None } else { Some(step) };
            }
            UiAction::CollapseAll => self.items_expanded.clear(),
            UiAction::ToggleItems => self.show_items = !self.show_items,
            UiAction::ToggleHistory => self.show_history = !self.show_history,
            UiAction::Search => {
                let at = self.viewport_rect.center() - vec2(125.0, 60.0);
                self.search = Some(Search { query: String::new(), at, selected: 0, opened_frame: true, at_pointer: false });
            }
            UiAction::ToolDone => {
                // A live step is already applied: Done just ends the tool.
                if self.is_live() && self.numeric.is_none() {
                    self.finish_live();
                } else {
                    self.commit_tool();
                    if self.is_live() {
                        self.finish_live();
                    }
                }
            }
            UiAction::ToolCancel => {
                // Abandons the tool, taking back a step it already made (Align, observed 389; a pulled Extrude
                // or Offset Face too: Cancel never keeps the result). With the picks from before the step; outside
                // sketches nothing stays picked after a live result is taken back.
                if self.is_live() && !matches!(self.tool, Tool::Idle) {
                    self.session.cancel_last_step();
                    if self.session.active_sketch.is_none() {
                        self.session.selection.clear();
                    }
                }
                self.numeric = None;
                self.preview = None;
                self.live_step = None;
                self.tool = Tool::Idle;
            }
            UiAction::ToolOption => match &mut self.tool {
                Tool::Mirror(t) if t.split && !t.with.is_empty() => {
                    let t = tools::MirrorTool { keep: !t.keep, ..t.clone() };
                    self.variants.insert("Split Keep".into(), usize::from(t.keep));
                    self.apply_split(t);
                }
                Tool::Mirror(t) => {
                    t.keep = !t.keep;
                    if t.split {
                        let keep = t.keep;
                        self.variants.insert("Split Keep".into(), usize::from(keep));
                    }
                }
                Tool::Translate { copy, .. } => *copy = !*copy,
                Tool::Move(_) => {
                    let off = self.auto_orient();
                    self.variants.insert(super::AUTO_ORIENT.into(), usize::from(off));
                }
                Tool::CircularPattern(t) => {
                    t.uniform = !t.uniform;
                    if self.is_live() {
                        self.commit_tool();
                    }
                }
                Tool::Loft(t) => {
                    t.ruled = !t.ruled;
                    // A loft on screen is made again with straight or smooth walls.
                    if self.is_live() {
                        self.commit_tool();
                    }
                }
                Tool::RotateAxis(t) => t.copy = !t.copy,
                Tool::Boolean(t) => t.next_keep(),
                Tool::Onto(t) if !t.project => {
                    let t = tools::OntoTool { flip: !t.flip, ..t.clone() };
                    self.apply_onto(t);
                }
                _ => {}
            },
            UiAction::ExitSketching => {
                self.tool = Tool::Idle;
                let _ = self.run("exit_sketching", json!({}));
            }
            UiAction::DeselectAll => {
                self.session.selection.clear();
                self.tool = Tool::Idle;
            }
            UiAction::Copy => self.copy_items(),
            UiAction::Paste => self.paste_items(),
            UiAction::Modeling => {
                self.drawings = None;
                self.visualizing = false;
            }
            UiAction::Visualization => {
                self.drawings = None;
                self.visualizing = !self.visualizing;
                self.tool = Tool::Idle;
            }
            UiAction::Drawings => {
                self.switcher = !self.switcher;
                self.popup_opened_at = self.now;
            }
            UiAction::CreateDrawing => self.begin_create_drawing(),
            UiAction::DrawingNext => {
                let bodies: Vec<ItemId> = self.session.selection.iter().filter_map(Pick::body).collect();
                if bodies.is_empty() {
                    self.toast("Select the bodies to include");
                } else {
                    self.creating_drawing = false;
                    self.begin_create_drawing();
                }
            }
            UiAction::DrawingCancel => self.creating_drawing = false,
            UiAction::DrawingProps => {
                if let Some(d) = &mut self.drawings {
                    d.props = !d.props;
                }
            }
            UiAction::ShowDrawingProps => {
                if let Some(d) = &mut self.drawings {
                    d.props = true;
                }
            }
            UiAction::VisTab(k) => {
                if self.visualizing {
                    self.vis_tab = usize::from(k.min(2));
                }
            }
            UiAction::ExportDrawing => self.export_choice = Some(drawings::ExportChoice { pdf: true, dxf: false, svg: false, dwg: false }),
            UiAction::DisplayModes => {
                self.right_pop = match self.right_pop {
                    Some((RightPop::Display, ..)) => None,
                    _ => Some((RightPop::Display, self.display_menu_at, self.now)),
                };
                self.popup_opened_at = self.now;
            }
            UiAction::Measure => {
                self.measuring = !self.measuring;
                self.measure_points.clear();
            }
            UiAction::TogglePinned => self.view_opts.pinned = !self.view_opts.pinned,
            UiAction::SelectThrough => {
                // From the menu: under the pointer, else the middle of the view.
                let at = self.cursor.unwrap_or_else(|| self.viewport_rect.center());
                let through = self.picks_through(at);
                if through.len() > 1 {
                    self.through = Some((at, through));
                    self.popup_opened_at = self.now;
                }
            }
            UiAction::ShowHiddenBodies => self.show_hidden_bodies(),
            UiAction::InvertVisibility => self.invert_body_visibility(),
            UiAction::ZoomToSelection => self.zoom_to_selection(),
            UiAction::RevealInItems => self.show_items = true,
            UiAction::RenameProject => self.renaming = Some((None, self.session.doc.name.clone(), true)),
            UiAction::Visualized => self.toggle_visualized(),
            UiAction::MeasureMode(i) => {
                self.variants.insert("Measure mode".into(), (i as usize).min(2));
                self.measuring = true;
                self.measure_points.clear();
            }
            UiAction::EditSection => {
                if let Some(s) = self.section.as_mut() {
                    s.editing = true;
                } else {
                    self.ui_action(UiAction::SectionView);
                }
            }
            UiAction::Isolate => {
                self.isolated = match self.isolated {
                    Some(_) => None,
                    None => {
                        let ids: Vec<ItemId> = self.session.selection.iter().filter_map(Pick::body).collect();
                        (!ids.is_empty()).then_some(ids)
                    }
                };
                if self.isolated.is_none() && self.session.selection.is_empty() {
                    self.toast("Select bodies to isolate");
                }
            }
            UiAction::Rename => {
                let item = self.session.selection.iter().find_map(Pick::item);
                if let Some(id) = item {
                    let name = self.session.doc.item_name(id).unwrap_or_default().to_string();
                    self.renaming = Some((Some(id), name, true));
                    self.show_items = true;
                }
            }
            UiAction::Shortcuts => {
                self.show_shortcuts = !self.show_shortcuts;
                self.help_open = false;
                self.popup_opened_at = self.now;
            }
            UiAction::ConstructionMode => self.construction_mode = !self.construction_mode,
            UiAction::ViewTool(i) => {
                if let Some(t) = sheet_views::ViewTool::from_index(i) {
                    self.start_view_tool(t);
                }
            }
            UiAction::ViewDone | UiAction::ViewCancel => self.end_view_tool(),
            UiAction::DrawTool(i) => {
                if let Some(t) = annotate::DrawTool::from_index(i) {
                    self.start_draw_tool(t);
                }
            }
            UiAction::Screenshot => self.wants_capture = true,
            UiAction::Noop => {}
            UiAction::InsertImage => self.wants_image = true,
            UiAction::ImportMaterial => self.wants_material = true,
            UiAction::ImportDecal => self.wants_decal = true,
            UiAction::Variables => {
                let mut rows: Vec<(String, String, Option<String>, shapecraft_doc::VarKind)> =
                    self.session.doc.variables.iter().map(|v| (v.name.clone(), v.expr.clone(), Some(v.name.clone()), v.kind)).collect();
                if rows.is_empty() {
                    rows.push(("var1".into(), "10 mm".into(), None, shapecraft_doc::VarKind::Length));
                }
                self.variables_dialog = Some(rows);
            }
            UiAction::Help => {
                self.help_open = !self.help_open;
                self.popup_opened_at = self.now;
            }
            UiAction::Preferences => {
                self.prefs_open = true;
                self.help_open = false;
                self.popup_opened_at = self.now;
            }
            UiAction::DeleteSection => {
                if let Some(mode) = self.section.and_then(|s| s.flat) {
                    self.display = mode;
                }
                self.section = None;
            }
            UiAction::SectionOnly => {
                if let Some(s) = &mut self.section {
                    s.only = !s.only;
                }
            }
            // 2D Section View: square on to the cut, orthographic, wireframe, the cut faces only (s16-024).
            UiAction::Section2D => {
                if let Some(mut s) = self.section {
                    match s.flat.take() {
                        Some(mode) => self.display = mode,
                        None => {
                            s.flat = Some(self.display);
                            self.display = shapecraft_render::DisplayMode::Wireframe;
                            let up = if s.normal.z.abs() > 0.9 { DVec3::Y } else { DVec3::Z };
                            // From the side that is cut away, looking at what's kept.
                            self.animate_to(Camera::facing(s.normal, up, s.origin(), self.camera.distance));
                        }
                    }
                    self.section = Some(s);
                }
            }
            UiAction::NavPresets => {
                self.nav_presets = Some(self.settings.preset().name);
                self.help_open = false;
                self.popup_opened_at = self.now;
            }
            // 26.200: with no flat face picked, Section View first asks for its reference (s11-117).
            UiAction::SectionView
                if self.section.is_none()
                    && self.section_pick.is_none()
                    && !matches!(self.session.selection.as_slice(), [Pick::Face { .. }]) =>
            {
                self.section_pick = Some(None);
            }
            UiAction::SectionView if self.section_pick.is_some() => self.section_pick = None,
            UiAction::SectionView => {
                if let [Pick::Face { body, face }] = self.session.selection.as_slice()
                    && self.section.is_none()
                    && let Some(fp) =
                        self.session.doc.body(*body).and_then(|b| self.session.kernel.face_plane(&b.shape, *face).ok().flatten())
                {
                    // Cut on the face's plane, keeping what's behind it.
                    self.section = Some(Section::new(fp.origin, -fp.normal));
                    return;
                }
                self.section = match self.section {
                    // On: reopens it for editing; editing: done (Delete Section takes it away, s16-021).
                    Some(s) => Some(Section { editing: !s.editing, ..s }),
                    None => {
                        // Cut across the model, removing the half that faces the camera.
                        let center = self.session.doc.bounds().map(|(a, b)| (a + b) / 2.0).unwrap_or(self.camera.target);
                        let back = self.camera.back();
                        let normal = if back.x.abs() > back.y.abs() { DVec3::X * back.x.signum() } else { DVec3::Y * back.y.signum() };
                        Some(Section::new(center, normal))
                    }
                };
            }
        }
    }

    /// Show Hidden Bodies: every hidden body shows again.
    pub(super) fn show_hidden_bodies(&mut self) {
        let hidden: Vec<u64> = self.session.doc.bodies.iter().filter(|b| !b.visible).map(|b| b.id.0).collect();
        if !hidden.is_empty() {
            let _ = self.run("show_hide", json!({ "items": hidden, "visible": true }));
        }
    }

    /// Invert Body Visibility: what's shown hides and what's hidden shows.
    pub(super) fn invert_body_visibility(&mut self) {
        let all: Vec<u64> = self.session.doc.bodies.iter().map(|b| b.id.0).collect();
        if !all.is_empty() {
            let _ = self.run("show_hide", json!({ "items": all }));
        }
    }

    /// Escape: leaves the running tool, then clears the selection.
    pub fn escape(&mut self) {
        if self.section_pick.take().is_some() {
            return;
        }
        if self.creating_drawing {
            self.creating_drawing = false;
            return;
        }
        // A typed value (or a length fixed with Tab) goes first, keeping the segment (s11-015) or the running tool
        // (26.202, s39-of-esc1: Offset Face's field closes, the tool stays).
        if self.line_len.take().is_some() | self.numeric.take().is_some() {
            return;
        }
        // An open context menu closes first (as observed).
        if self.context_menu.take().is_some() {
            return;
        }
        if self.search.take().is_some()
            || self.flyout_sub.take().is_some()
            || self.flyout.take().is_some()
            || self.right_pop.take().is_some()
            || self.constraint_pop.take().is_some()
        {
            return;
        }
        // In a sketch, Escape peels one layer at a time (reference 26.202, s36): an armed tool, then the
        // selection, then the sketch itself. A tool armed in a sketch with nothing drawn yet leaves it at once.
        let sketch_empty = self.session.active_sketch.and_then(|id| self.session.doc.sketch(id)).map(|sk| sk.entities.is_empty());
        match self.tool {
            Tool::Idle if sketch_empty.is_some() && self.session.selection.is_empty() => {
                self.ui_action(UiAction::ExitSketching);
            }
            Tool::Idle => {
                self.session.selection.clear();
            }
            // As observed: Escape finishes the spline being drawn and keeps the tool; the next one leaves it.
            Tool::Spline { ref points, control } if !points.is_empty() => {
                let points = points.clone();
                self.finish_spline(points, control);
            }
            Tool::Line { .. }
            | Tool::Rectangle { .. }
            | Tool::Circle { .. }
            | Tool::Arc { .. }
            | Tool::Polygon { .. }
            | Tool::Ellipse { .. }
            | Tool::Spline { .. }
                if sketch_empty == Some(true) =>
            {
                self.ui_action(UiAction::ExitSketching);
            }
            Tool::PickPlane { .. } => {
                self.pending_tool = None;
                self.tool = Tool::Idle;
            }
            // A tool holding a live result (Cancel / Done showing): Escape is Cancel, taking its step back.
            _ if self.is_live() => self.ui_action(UiAction::ToolCancel),
            _ => {
                self.tool = Tool::Idle;
            }
        }
    }

    pub fn delete_selection(&mut self) {
        if let Some((sketch, index)) = self.selected_constraint.take() {
            let _ = self.run("delete_constraint", json!({ "sketch": sketch.0, "index": index }));
            return;
        }
        if !self.session.selection.is_empty() {
            // A face that can't be taken out says so in the banner's place, as the reference does (s24 matrix).
            if self.session.selection.iter().any(|p| matches!(p, Pick::Face { .. })) {
                if let Err(e) = self.session.execute("delete", &json!({})) {
                    let text = match e {
                        shapecraft_engine::EngineError::NotAvailable(m) | shapecraft_engine::EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    };
                    self.notice = Some((text, self.now + 3.0));
                    return;
                }
            } else {
                let _ = self.run("delete", json!({}));
            }
            self.tool = Tool::Idle;
        }
    }
}

impl Workspace {
    /// Edit ▸ Copy (⌘C): remembers the selected bodies and construction planes. The system clipboard gets a tag,
    /// so ⌘V pastes them only while nothing else has been copied since.
    pub(super) fn copy_items(&mut self) {
        let mut ids = Vec::new();
        for p in &self.session.selection {
            let id = match *p {
                Pick::Body { body } | Pick::Face { body, .. } | Pick::Edge { body, .. } => body,
                Pick::Plane { plane } => plane,
                _ => continue,
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            self.toast("Select bodies to copy");
            return;
        }
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let tag = format!("ShapeCraft copy {nanos:x}: {}", ids.iter().map(|i| i.0.to_string()).collect::<Vec<_>>().join(","));
        self.clip_out = Some(tag.clone());
        let n = ids.len();
        self.clipboard = (ids, tag);
        self.toast(if n == 1 { "Copied 1 item".to_string() } else { format!("Copied {n} items") });
    }

    /// ⌘V with this text on the system clipboard: pastes when it is the tag of our last copy.
    pub(super) fn paste_text(&mut self, text: &str) -> bool {
        if self.clipboard.0.is_empty() || text != self.clipboard.1 {
            return false;
        }
        self.paste_items();
        true
    }

    /// Edit ▸ Paste: unlinked copies of what was copied, in place, selected (Move them from there).
    pub(super) fn paste_items(&mut self) {
        let doc = &self.session.doc;
        let ids: Vec<u64> =
            self.clipboard.0.iter().filter(|id| doc.body(**id).is_some() || doc.plane(**id).is_some()).map(|id| id.0).collect();
        if ids.is_empty() {
            self.toast(if self.clipboard.0.is_empty() { "Nothing copied yet" } else { "What was copied is gone" });
            return;
        }
        self.tool = Tool::Idle;
        let n = ids.len();
        if self.run("translate", json!({ "items": ids, "by": [0, 0, 0], "copy": true, "linked": false })).is_some() {
            self.toast(if n == 1 { "Pasted in place: 1 item".to_string() } else { format!("Pasted in place: {n} items") });
        }
    }
}
