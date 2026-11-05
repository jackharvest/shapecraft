//! Keyboard handling: typed values, custom shortcuts, and every key the workspace answers.

use serde_json::json;
use shapecraft_doc::Pick;
use shapecraft_render::StandardView;

use super::toolbar::{ToolKind, UiAction};
use super::tools::{self, Tool};
use super::{Workspace, area, shortcuts};

impl Workspace {
    pub(super) fn keyboard(&mut self, ui: &mut egui::Ui) {
        // Tab while typing a value goes to the tool's next field (a rectangle's height), not to egui's focus
        // traversal, which would hand the keyboard to some text field and swallow the rest of the value.
        if self.numeric.is_some()
            && ui.input(|i| i.key_pressed(egui::Key::Tab))
            && let Some(id) = ui.memory(|m| m.focused())
        {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        if self.keys_blocked
            || self.search.is_some()
            || self.renaming.is_some()
            || self.variables_dialog.is_some()
            || self.prefs_open
            || self.nav_presets.is_some()
            || self.help_open
            || self.badge_menu.is_some()
            || ui.ctx().egui_wants_keyboard_input()
        {
            if let Some(t) = self.clip_out.take() {
                ui.ctx().copy_text(t);
            }
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for ev in events {
            match ev {
                // ⌘C / ⌘V reach us as Copy / Paste (text fields have had their turn above).
                egui::Event::Copy if self.drawings.is_none() && !self.visualizing && !self.session.selection.is_empty() => {
                    self.copy_items()
                }
                egui::Event::Paste(text) if self.drawings.is_none() && !self.visualizing => {
                    self.paste_text(&text);
                }
                egui::Event::Text(text) if self.tool.takes_numbers() || self.dim_edit.is_some() => {
                    for ch in text.chars() {
                        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == ',' {
                            self.numeric.get_or_insert_with(String::new).push(if ch == ',' { '.' } else { ch });
                        }
                    }
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    // A customised key runs its command first (Keyboard Shortcuts ▸ Save Changes).
                    let combo = shortcuts::combo(key, modifiers);
                    let custom = self.settings.custom_keys.iter().find(|(_, k)| *k == combo).map(|(label, _)| label.clone());
                    match custom {
                        Some(label) if !self.show_shortcuts => shortcuts::run_label(self, &label),
                        _ => self.key(key, modifiers),
                    }
                }
                _ => {}
            }
        }
        if let Some(t) = self.clip_out.take() {
            ui.ctx().copy_text(t);
        }
    }

    fn key(&mut self, key: egui::Key, m: egui::Modifiers) {
        use egui::Key;
        // While a box is dragged, Tab cycles its filter and B / F / E pick one, as the banner shows.
        if self.area.is_some() {
            match key {
                Key::Tab => self.area_filter = (self.area_filter + 1) % area::FILTERS.len(),
                Key::B => self.area_filter = 1,
                Key::F => self.area_filter = 2,
                Key::E => self.area_filter = 3,
                _ => {}
            }
            return;
        }
        let cmd = m.command;
        let typing = self.numeric.is_some();
        // The Drawings sheet takes no modeling keys: Escape (or ⇧⌘\) goes back, ⌘ shortcuts still work.
        if let Some(d) = &mut self.drawings
            && !cmd
        {
            match key {
                // Escape steps back: the tool, then the selected annotation or view, then out of the drawing.
                Key::Escape | Key::Enter if d.view_tool.is_some() => {
                    d.view_tool = None;
                    d.preview = None;
                }
                Key::Escape if d.tool.is_some() => {
                    d.tool = None;
                    d.picks.clear();
                }
                Key::Escape if d.selected.is_some() => d.selected = None,
                Key::Escape if d.view_sel.is_some() => d.view_sel = None,
                Key::Escape => self.drawings = None,
                Key::Delete | Key::Backspace => {
                    let id = d.id;
                    if let Some(i) = d.selected.take() {
                        let _ = self.run("delete_annotation", json!({ "drawing": id.0, "index": i }));
                    } else if let Some(v) = d.view_sel.take() {
                        let _ = self.run("delete_drawing_view", json!({ "drawing": id.0, "view": v }));
                    }
                }
                _ => {}
            }
            return;
        }
        // Visualization: Escape closes the environment settings or lets go of the decal; Delete takes it off.
        if self.visualizing && !cmd {
            match key {
                Key::Escape if self.env_settings => {
                    self.env_settings = false;
                    return;
                }
                Key::Escape if self.decal_sel.is_some() => {
                    self.decal_sel = None;
                    return;
                }
                Key::Delete | Key::Backspace if self.decal_sel.is_some() => {
                    if let Some(d) = self.decal_sel.take() {
                        let _ = self.run("delete_decal", json!({ "decal": d.0 }));
                    }
                    return;
                }
                _ => {}
            }
        }
        // ⇧ + letter in a sketch adds a constraint (the chips' shortcuts).
        if m.shift && !cmd && !m.alt && !m.ctrl && !typing && self.session.active_sketch.is_some() {
            let kind = match key {
                Key::A => Some("parallel"),
                Key::P => Some("perpendicular"),
                Key::T => Some("tangent"),
                Key::N => Some("coincident"),
                Key::M => Some("midpoint"),
                Key::C => Some("concentric"),
                Key::V => Some("horizontal_vertical"),
                Key::E => Some("equal"),
                Key::S => Some("symmetry"),
                Key::L => Some("lock"),
                _ => None,
            };
            if let Some(kind) = kind {
                self.apply_constraint(kind);
                return;
            }
        }
        match key {
            Key::Escape if self.show_shortcuts => self.show_shortcuts = false,
            Key::Escape if self.dim_edit.is_some() => {
                self.dim_edit = None;
                self.numeric = None;
            }
            Key::Enter if self.dim_edit.is_some() => {
                if let (Some(edit), Some(v)) = (self.dim_edit.take(), self.numeric.take().and_then(|t| self.eval_typed(&t))) {
                    self.apply_dim(edit, v);
                }
            }
            Key::Escape => self.escape(),
            // Return with nothing to finish opens Search Commands at the pointer (26.202, s36).
            Key::Enter if !typing && !self.is_live() && self.nothing_to_finish() => {
                let at = self.cursor.filter(|c| self.viewport_rect.contains(*c)).unwrap_or(self.viewport_rect.center());
                self.search = Some(super::Search {
                    query: String::new(),
                    at: at - egui::vec2(20.0, 20.0),
                    selected: 0,
                    opened_frame: true,
                    at_pointer: true,
                });
            }
            // A result already applied and nothing typed: Return is Done (the button's ↵; 26.202, s40-004).
            Key::Enter if !typing && self.numeric.is_none() && self.is_live() && !matches!(self.tool, Tool::Idle) => {
                self.live_step = None;
                self.tool = Tool::Idle;
            }
            Key::Enter => {
                if matches!(self.tool, Tool::Revolve(_) | Tool::RotateAxis(_) | Tool::CircularPattern(_))
                    || matches!(&self.tool, Tool::EdgePlane(t) if t.through)
                    || matches!(&self.tool, Tool::Move(t) if matches!(t.handle, tools::Handle::Ring(_)))
                {
                    self.numeric_angle = self.numeric.as_deref().and_then(|n| n.trim().parse::<f64>().ok()).filter(|a| a.is_finite());
                }
                self.commit_tool();
            }
            Key::Backspace if typing && self.numeric.as_deref() == Some("") && self.dim_edit.is_some() => {}
            Key::Backspace if typing => {
                if let Some(n) = &mut self.numeric {
                    n.pop();
                    if n.is_empty() {
                        self.numeric = None;
                    }
                }
            }
            Key::Tab if typing => self.next_numeric_field(),
            Key::Backspace | Key::Delete if matches!(&self.tool, Tool::Spline { points, .. } if !points.is_empty()) => self.escape(),
            Key::Backspace | Key::Delete => self.delete_selection(),
            Key::Z if cmd && m.shift => {
                let _ = self.run("redo", json!({}));
            }
            Key::Z if cmd => {
                self.tool = Tool::Idle;
                let _ = self.run("undo", json!({}));
            }
            Key::Backslash if cmd && m.shift => self.ui_action(UiAction::Drawings),
            Key::F if cmd => self.ui_action(UiAction::Search),
            Key::V if cmd && m.shift => self.ui_action(UiAction::Variables),
            Key::A if cmd && m.shift => self.ui_action(UiAction::DeselectAll),
            Key::A if cmd => {
                let _ = self.run("select_all", json!({}));
            }
            Key::P if cmd && m.alt => self.show_history = !self.show_history,
            Key::R if cmd && m.alt => self.renaming = Some((None, self.session.doc.name.clone(), true)),
            Key::R if cmd => self.ui_action(UiAction::Rename),
            Key::C if cmd && m.shift => self.copy_shot = true,
            Key::Z if m.alt && !cmd => self.display = shapecraft_render::DisplayMode::Zebra { vertical: false, scale: 100 },
            Key::C if m.alt && !cmd => self.display = shapecraft_render::DisplayMode::Curvature { scale: 100 },
            Key::W if m.alt && !cmd => self.display = shapecraft_render::DisplayMode::Wireframe,
            Key::X if m.alt && !cmd => self.display = shapecraft_render::DisplayMode::XRay,
            Key::S if m.alt && !cmd => self.display = shapecraft_render::DisplayMode::Shaded,
            Key::S if cmd && m.alt => self.show_items = !self.show_items,
            Key::U if cmd => self.start_tool(ToolKind::Union),
            Key::B if cmd => self.start_tool(ToolKind::Subtract),
            Key::I if cmd && !m.shift => self.start_tool(ToolKind::Intersect),
            Key::Num1 if cmd => self.standard_view(StandardView::Default),
            Key::Num2 if cmd => self.standard_view(StandardView::Front),
            Key::Num3 if cmd => self.standard_view(StandardView::Back),
            Key::Num4 if cmd => self.standard_view(StandardView::Top),
            Key::Num5 if cmd => self.standard_view(StandardView::Bottom),
            Key::Num6 if cmd => self.standard_view(StandardView::Right),
            Key::Num7 if cmd => self.standard_view(StandardView::Left),
            Key::Num8 if cmd => self.nearest_ortho_view(),
            // ⌘ arrows turn the view 15°, ⇧⌘ arrows 45° (the reference's shortcut list).
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown if cmd => {
                let dir = match key {
                    Key::ArrowLeft => 0,
                    Key::ArrowRight => 1,
                    Key::ArrowUp => 2,
                    _ => 3,
                };
                self.rotate_view(dir, m.shift);
            }
            Key::H if cmd && m.alt => self.show_history = !self.show_history,
            Key::H if cmd && m.shift => {
                let bodies: Vec<u64> = self
                    .session
                    .selection
                    .iter()
                    .filter_map(|p| p.body())
                    .map(|b| b.0)
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if !bodies.is_empty() {
                    let _ = self.run("show_hide", json!({ "items": bodies }));
                }
            }
            Key::H if m.alt && m.shift => self.invert_body_visibility(),
            Key::H if m.alt => {
                let _ = self.run("show_all", json!({}));
            }
            Key::I if m.alt && m.shift => self.show_items = true,
            Key::L if m.alt && !cmd && !m.shift => self.items_expanded.clear(),
            // Merge Modeling Steps opens under the History title.
            Key::M if m.alt && m.shift && !cmd => {
                self.show_history = true;
                self.history.merge = Some((false, true, self.now));
            }
            Key::S if cmd && m.shift => {
                if let Some(c) = self.cursor {
                    let through = self.picks_through(c);
                    if through.len() > 1 {
                        self.through = Some((c, through));
                        self.popup_opened_at = self.now;
                    }
                }
            }
            Key::V if m.alt && !cmd => self.toggle_visualized(),
            _ if cmd || m.alt || m.ctrl => {}
            // Space starts a sketch on the face (or construction plane) under the pointer, else on the ground plane,
            // even while a tool the pick started on its own (Offset Face, Move) waits to be pulled (s24-011/012).
            Key::Space
                if matches!(self.tool, Tool::Idle | Tool::Await { .. })
                    || self.tool.is_sketch_tool()
                    || (self.predicted && self.live_step.is_none()) =>
            {
                // Over a sidebar (no pointer in the view) Space does nothing (s24 matrix).
                if !self.cursor.is_some_and(|c| self.viewport_rect.contains(c)) {
                    return;
                }
                // Over a sketch's region (outside it): that sketch opens, no tool armed (s24 matrix).
                if let Some(Pick::Region { sketch, .. }) = self.hover
                    && self.session.active_sketch != Some(sketch)
                {
                    if self.predicted {
                        self.tool = Tool::Idle;
                        self.predicted = false;
                        self.preview = None;
                    }
                    if self.open_sketch_in_place(sketch) {
                        self.tool = Tool::Idle;
                    }
                    return;
                }
                // A curved face has no plane of its own: the origin plane facing the view most (s24-uw-curved-space).
                let facing = || {
                    let back = self.camera.back();
                    let (x, y, z) = (back.x.abs(), back.y.abs(), back.z.abs());
                    if z >= x && z >= y {
                        (json!({ "plane": "xy" }), Some((glam::DVec3::ZERO, glam::DVec3::Z)))
                    } else if y >= x {
                        (json!({ "plane": "xz" }), Some((glam::DVec3::ZERO, glam::DVec3::Y)))
                    } else {
                        (json!({ "plane": "yz" }), Some((glam::DVec3::ZERO, glam::DVec3::X)))
                    }
                };
                let (params, on) = match self.hover {
                    Some(Pick::Face { body, face }) => {
                        let fp = self.session.doc.body(body).and_then(|b| self.session.kernel.face_plane(&b.shape, face).ok().flatten());
                        match fp {
                            Some(fp) => (json!({ "body": body.0, "face": face }), Some((fp.origin, fp.normal))),
                            None => facing(),
                        }
                    }
                    Some(Pick::Plane { plane }) => {
                        let pl = self.session.doc.plane(plane).map(|p| (p.plane.origin, p.plane.normal()));
                        (json!({ "construction": plane.0 }), pl)
                    }
                    None if self.session.active_sketch.is_none() => (json!({ "plane": "xy" }), Some((glam::DVec3::ZERO, glam::DVec3::Z))),
                    _ => return,
                };
                if self.predicted {
                    self.tool = Tool::Idle;
                    self.predicted = false;
                    self.preview = None;
                }
                // A shown sketch already on that plane opens, with no tool armed; else a new sketch, Line armed.
                match on.and_then(|(o, n)| self.shown_sketch_on(o, n)) {
                    Some(sk) if self.session.active_sketch != Some(sk) => {
                        if self.open_sketch_in_place(sk) {
                            self.tool = Tool::Idle;
                        }
                    }
                    Some(_) => {}
                    None => {
                        self.start_sketch_on(params);
                    }
                }
            }
            // Hotkey mode: X opens command search.
            Key::X if !typing => self.ui_action(UiAction::Search),
            // Preferences ▸ Single Key Action: Off leaves letters alone.
            _ if self.settings.single_key_action == "Off" && !matches!(key, Key::Minus | Key::Plus | Key::Equals) => {}
            Key::L => self.start_tool(ToolKind::Line),
            Key::R => self.start_tool(ToolKind::Rectangle),
            Key::C => self.start_tool(ToolKind::Circle),
            Key::G => self.start_tool(ToolKind::Polygon),
            Key::A => self.start_tool(ToolKind::Arc),
            Key::I => self.start_tool(ToolKind::Spline),
            // Modeling tools from inside a sketch leave it first and wait for their pick (s24 matrix); a region picked in
            // the sketch stays a profile for Extrude.
            Key::E | Key::F | Key::H | Key::N | Key::V | Key::W
                if self.session.active_sketch.is_some() && self.session.selection_kind() != shapecraft_engine::SelectionKind::Regions =>
            {
                self.ui_action(UiAction::ExitSketching);
                self.session.selection.clear();
                self.start_tool(match key {
                    Key::E => ToolKind::Extrude,
                    Key::F => ToolKind::Fillet,
                    Key::H => ToolKind::Shell,
                    Key::N => ToolKind::Translate,
                    Key::V => ToolKind::Revolve,
                    _ => ToolKind::Sweep,
                });
            }
            Key::E => self.start_tool(ToolKind::Extrude),
            Key::F => self.start_tool(ToolKind::Fillet),
            Key::H => self.start_tool(ToolKind::Shell),
            // With nothing picked it waits for a pick, in a sketch too (as observed).
            Key::M => self.start_tool(ToolKind::Move),
            Key::N => self.start_tool(ToolKind::Translate),
            Key::Minus | Key::Plus | Key::Equals
                if matches!(self.tool, Tool::Pattern(_) | Tool::CircularPattern(_)) && self.numeric.is_none() =>
            {
                let step = |c: u32| if key == Key::Minus { c.saturating_sub(1).max(2) } else { (c + 1).min(100) };
                match &mut self.tool {
                    Tool::Pattern(t) => t.count = step(t.count),
                    Tool::CircularPattern(t) => t.count = step(t.count),
                    _ => {}
                }
                self.preview = None;
            }
            Key::V
                if self.session.active_sketch.is_none() || self.session.selection_kind() == shapecraft_engine::SelectionKind::Regions =>
            {
                self.start_tool(ToolKind::Revolve)
            }
            // S starts Scale in the variant last picked (Non-uniform until then), as observed.
            Key::S => self.start_tool(if self.variant("Scale") == 0 { ToolKind::Scale } else { ToolKind::ScaleNonUniform }),
            // Trim, Project and Offset Edge answer anywhere: outside a sketch they wait for their pick (s24 matrix).
            Key::T => self.start_tool(ToolKind::Trim),
            Key::P => self.start_tool(ToolKind::Project),
            Key::O => self.start_tool(ToolKind::OffsetEdge),
            Key::W => self.start_tool(ToolKind::Sweep),
            _ => {}
        }
    }

    /// No tool running, or a sketch tool armed with nothing drawn yet: Return has nothing to apply.
    fn nothing_to_finish(&self) -> bool {
        match &self.tool {
            Tool::Idle => self.drawings.is_none() && !self.visualizing,
            Tool::Line { first: None, last: None }
            | Tool::Rectangle { first: None }
            | Tool::Circle { center: None }
            | Tool::Polygon { center: None }
            | Tool::Arc { a: None, b: None }
            | Tool::Ellipse { center: None, major: None }
            | Tool::Rect3 { a: None, b: None } => true,
            Tool::Spline { points, .. } => points.is_empty(),
            _ => false,
        }
    }
}
