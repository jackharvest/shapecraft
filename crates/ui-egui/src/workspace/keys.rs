//! Keyboard handling: typed values, custom shortcuts, and every key the workspace answers.

use serde_json::json;
use shapecraft_doc::Pick;
use shapecraft_render::StandardView;

use super::toolbar::{Action, ToolKind, UiAction};
use super::tools::{self, Tool};
use super::{Workspace, area, shortcuts};

impl Workspace {
    /// Whether a popup or window that takes the keyboard is open.
    pub(super) fn popup_takes_keys(&self) -> bool {
        self.keys_blocked
            || self.importing.is_some()
            || self.working.is_some()
            || self.search.is_some()
            || self.renaming.is_some()
            || self.variables_dialog.is_some()
            || self.var_menu.is_some()
            || self.var_create.is_some()
            || self.prefs_open
            || self.nav_presets.is_some()
            || self.help_open
            || self.badge_menu.is_some()
    }

    pub(super) fn keyboard(&mut self, ui: &mut egui::Ui) {
        // Tab while typing a value goes to the tool's next field (a rectangle's height), not to egui's focus
        // traversal, which would hand the keyboard to some text field and swallow the rest of the value. Before
        // anything is typed it would swallow the value to come.
        if (self.numeric.is_some() || self.takes_values())
            && ui.input(|i| i.key_pressed(egui::Key::Tab))
            && let Some(id) = ui.memory(|m| m.focused())
        {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        // (A popup open when the frame began had this frame's keys, even if one of them just closed it: the Escape
        // that closes Search doesn't also leave the sketch.)
        if self.popup_had_keys || self.popup_takes_keys() || ui.ctx().egui_wants_keyboard_input() {
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
                egui::Event::Text(text) if self.takes_values() || self.dim_edit.is_some() => {
                    for ch in text.chars() {
                        self.type_numeric(ch);
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

    /// One typed character into the value field. A digit, point, minus or "(" opens it (a letter alone doesn't, as
    /// observed: letters are shortcuts); once open it takes anything an expression holds: variable names, units,
    /// operators, brackets, feet ' and inches ". A value filled from a clicked pill is replaced by the first key.
    pub(super) fn type_numeric(&mut self, ch: char) {
        let opens = ch.is_ascii_digit() || matches!(ch, '.' | '-' | ',' | '(');
        let open = self.numeric.is_some();
        let more = open && (ch.is_ascii_alphanumeric() || " +-*/^()_.,'\"°×÷−".contains(ch));
        if opens || more {
            if std::mem::take(&mut self.numeric_fresh) {
                self.numeric = None;
            }
            let t = self.numeric.get_or_insert_with(String::new);
            // A leading space does nothing; a field is capped like any typed text.
            if !(ch == ' ' && t.is_empty()) && t.len() < 256 {
                t.push(ch);
            }
            self.numeric_error = None;
        }
    }

    /// The variable picked in the value box's list goes in where the typing is (a value filled from a pill is
    /// replaced) (#11).
    pub(super) fn insert_variable(&mut self, name: &str) {
        if std::mem::take(&mut self.numeric_fresh) {
            self.numeric = None;
        }
        let t = self.numeric.get_or_insert_with(String::new);
        if t.len() + name.len() <= 256 {
            t.push_str(name);
        }
        self.numeric_error = None;
    }

    /// Makes the variable the value box named before it existed, its value read like a value box's (a bare
    /// length in the project's unit), then applies the box as Return would (#12).
    pub(super) fn create_variable(&mut self, name: &str, value: &str, kind: shapecraft_doc::VarKind) -> Result<(), String> {
        let doc = &self.session.doc;
        let stored = shapecraft_doc::expr::with_units(value, &doc.variables, kind, doc.units.scale(), doc.units.suffix())?;
        let k = kind.label().to_lowercase();
        self.session.execute("set_variable", &json!({ "name": name, "value": stored, "kind": k })).map_err(|e| e.to_string())?;
        self.numeric_error = None;
        self.key(egui::Key::Enter, egui::Modifiers::NONE);
        Ok(())
    }

    pub(super) fn key(&mut self, key: egui::Key, m: egui::Modifiers) {
        use egui::Key;
        // While a box is dragged, Tab cycles its filter and B / F / E pick one, as the banner shows.
        if self.area.is_some() {
            match key {
                Key::Tab => self.area_filter = (self.area_filter + 1) % area::FILTERS.len(),
                Key::B => self.area_filter_key("B"),
                Key::F => self.area_filter_key("F"),
                Key::E => self.area_filter_key("E"),
                Key::P => self.area_filter_key("P"),
                Key::C => self.area_filter_key("C"),
                _ => {}
            }
            return;
        }
        let cmd = m.command;
        let typing = self.numeric.is_some();
        if !typing {
            self.numeric_error = None;
        }
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
            // A value that doesn't work keeps the box open with the reason under it.
            Key::Enter | Key::Tab if typing && !self.typed_ok() => {}
            Key::Enter if self.dim_edit.is_some() => self.commit_dim_edit(),
            Key::Enter if typing && self.section_typing() => self.commit_section_value(),
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
            Key::Enter if !typing && self.numeric.is_none() && self.is_live() && !matches!(self.tool, Tool::Idle) => self.finish_live(),
            // A pattern's open field takes its value and the pattern goes on; the next Return applies it (#96).
            Key::Enter if self.take_pattern_value() => {}
            Key::Enter => self.commit_tool(),
            Key::Backspace if typing && self.numeric.as_deref() == Some("") && self.dim_edit.is_some() => {}
            // A value filled from a clicked pill goes all at once, leaving the field open and empty.
            Key::Backspace if typing && std::mem::take(&mut self.numeric_fresh) => self.numeric = Some(String::new()),
            Key::Backspace if typing => {
                if let Some(n) = &mut self.numeric {
                    n.pop();
                    if n.is_empty() {
                        self.numeric = None;
                    }
                }
            }
            Key::Tab if typing || matches!(self.tool, Tool::Pattern(_) | Tool::CircularPattern(_)) => self.next_numeric_field(),
            Key::Backspace | Key::Delete if matches!(&self.tool, Tool::Spline { points, .. } if !points.is_empty()) => self.escape(),
            Key::Backspace | Key::Delete => self.delete_selection(),
            Key::Z if cmd && m.shift => self.act(Action::Command("redo"), "Redo"),
            Key::Z if cmd => self.act(Action::Command("undo"), "Undo"),
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
            // F2 renames what's picked, as in a file browser (never the project).
            Key::F2 if self.session.selection.iter().all(|p| p.item().is_none()) => self.toast("Select something to rename"),
            Key::F2 => self.ui_action(UiAction::Rename),
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
            // While a value is typed, letters and Space go into it (as observed), not to their shortcuts.
            _ if typing => {}
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
                let (params, on) = match self.hover {
                    Some(Pick::Face { body, face }) => self.sketch_on_face_params(body, face),
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
                let step = |c: u32| if key == Key::Minus { c.saturating_sub(1).max(2) } else { (c + 1).min(tools::PATTERN_MAX_COUNT) };
                match &mut self.tool {
                    Tool::Pattern(t) => t.count = step(t.count),
                    Tool::CircularPattern(t) => t.count = step(t.count),
                    _ => {}
                }
                self.preview = None;
            }
            // + and - zoom in and out, about the pointer when it's over the view (as the wheel does), else the view's
            // middle (#34). Not while a value is being typed or a tool takes one: there "-" starts a negative number.
            Key::Minus | Key::Plus | Key::Equals if !cmd && !typing && !self.takes_values() && self.dim_edit.is_none() => {
                let at = self.cursor.filter(|c| self.viewport_rect.contains(*c)).unwrap_or(self.viewport_rect.center());
                self.anim = None;
                let size = self.size();
                self.camera.zoom_at(if key == Key::Minus { 1.25 } else { 0.8 }, self.local(at), size);
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
            | Tool::Slot { a: None, .. }
            | Tool::Rect3 { a: None, b: None } => true,
            Tool::Spline { points, .. } => points.is_empty(),
            _ => false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use shapecraft_doc::{Units, VarKind, Variable};
    use std::sync::Arc;

    fn workspace() -> Workspace {
        let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Typed values");
        Workspace::new(session, crate::settings::Settings::default())
    }

    fn type_text(ws: &mut Workspace, text: &str) {
        for ch in text.chars() {
            ws.type_numeric(ch);
        }
    }

    /// #5: once the value box is open it takes a variable name and operators, and their letters don't run
    /// shortcuts; a letter alone doesn't open it.
    #[test]
    fn the_value_box_takes_variables_and_letters_stay_out_of_shortcuts() {
        let mut ws = workspace();
        ws.session.doc.variables.push(Variable { name: "box_width".into(), expr: "50 mm".into(), kind: VarKind::Length });
        type_text(&mut ws, "w");
        assert_eq!(ws.numeric, None, "a letter alone is a shortcut, not a value");
        type_text(&mut ws, "2*box_width + 1' 6\"");
        assert_eq!(ws.numeric.as_deref(), Some("2*box_width + 1' 6\""));
        let tool = format!("{:?}", ws.tool);
        ws.key(egui::Key::L, egui::Modifiers::NONE);
        ws.key(egui::Key::Space, egui::Modifiers::NONE);
        assert_eq!(format!("{:?}", ws.tool), tool, "L and Space went into the value, not to Line or Sketch");
        assert!((ws.eval_typed(ws.numeric.as_deref().unwrap()).unwrap() - (100.0 + 457.2)).abs() < 1e-9);
    }

    #[test]
    fn a_value_that_doesnt_work_keeps_the_box_open_with_the_reason() {
        let mut ws = workspace();
        type_text(&mut ws, "20+w");
        assert!(!ws.typed_ok());
        assert_eq!(ws.numeric.as_deref(), Some("20+w"));
        assert!(ws.numeric_error.as_deref().unwrap().contains("'w'"));
        ws.key(egui::Key::Backspace, egui::Modifiers::NONE);
        type_text(&mut ws, "5");
        assert_eq!(ws.numeric_error, None, "typing again clears the reason");
        assert!(ws.typed_ok());
    }

    #[test]
    fn a_bare_length_takes_the_project_unit_but_angles_and_factors_dont() {
        let mut ws = workspace();
        ws.session.doc.units = Units::Inch;
        assert!((ws.eval_typed("2").unwrap() - 50.8).abs() < 1e-9);
        assert!((ws.eval_typed("2 + 1 cm").unwrap() - 60.8).abs() < 1e-9);
        assert!((ws.eval_angle("90 / 2").unwrap() - 45.0).abs() < 1e-9);
        assert!((ws.eval_field("1.5 * 2", VarKind::Number).unwrap() - 3.0).abs() < 1e-9);
        assert_eq!(ws.typed_kind(), VarKind::Length);
    }

    /// A workspace with one 10 × 10 × 5 box, picked, and Linear Pattern running on it.
    fn patterning() -> Workspace {
        let mut ws = workspace();
        let s = &mut ws.session;
        s.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &serde_json::json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("extrude", &serde_json::json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
        s.active_sketch = None;
        let body = s.doc.bodies[0].id;
        s.selection = vec![Pick::Body { body }];
        ws.start_tool(super::super::toolbar::ToolKind::Pattern);
        assert!(matches!(ws.tool, Tool::Pattern(_)));
        ws
    }

    /// #6: Tab goes from the spacing to Quantity instead of finishing; Return applies both.
    #[test]
    fn tab_moves_a_pattern_from_spacing_to_quantity() {
        let mut ws = patterning();
        type_text(&mut ws, "20");
        ws.key(egui::Key::Tab, egui::Modifiers::NONE);
        let Tool::Pattern(t) = &ws.tool else { panic!("Tab finished the pattern: {:?}", ws.tool) };
        assert!(t.count_field);
        assert!((t.spacing - 20.0).abs() < 1e-9, "the typed spacing is kept");
        assert_eq!(ws.numeric.as_deref(), Some("3"), "Quantity opens with the count");
        assert_eq!(ws.typed_kind(), VarKind::Number);
        type_text(&mut ws, "5");
        assert_eq!(ws.numeric.as_deref(), Some("5"), "the first key replaces the count");
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        let Tool::Pattern(t) = &ws.tool else { panic!("Return on the field only takes the value (#96)") };
        assert_eq!((t.count, t.count_field), (5, false));
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert!(matches!(ws.tool, Tool::Idle), "Return applied the pattern");
        assert_eq!(ws.session.doc.bodies.len(), 5, "five copies, the original included");
    }

    /// #6: Tab again goes back to the spacing, keeping the count; Escape closes Quantity.
    #[test]
    fn tab_cycles_back_to_the_spacing_and_escape_closes_quantity() {
        let mut ws = patterning();
        type_text(&mut ws, "15");
        ws.key(egui::Key::Tab, egui::Modifiers::NONE);
        type_text(&mut ws, "4");
        ws.key(egui::Key::Tab, egui::Modifiers::NONE);
        let Tool::Pattern(t) = &ws.tool else { panic!() };
        assert!(!t.count_field);
        assert_eq!(t.count, 4);
        assert_eq!(ws.typed_kind(), VarKind::Length);
        ws.key(egui::Key::Tab, egui::Modifiers::NONE);
        ws.key(egui::Key::Escape, egui::Modifiers::NONE);
        let Tool::Pattern(t) = &ws.tool else { panic!("Escape on the field keeps the tool") };
        assert!(!t.count_field, "digits typed next go to the spacing");
        assert_eq!(t.count, 4);
    }

    /// #96: pulling an arrow with the Quantity field open keeps the typed count (it went back to 3).
    #[test]
    fn pulling_an_arrow_keeps_the_typed_quantity() {
        let mut ws = patterning();
        ws.open_pattern_count();
        type_text(&mut ws, "8");
        ws.pull_pattern_arrow(1, egui::pos2(0.0, 0.0));
        let Tool::Pattern(t) = &ws.tool else { panic!("the pull ended the tool") };
        assert_eq!((t.count, t.axis, t.count_field), (8, 1, false));
        assert!(ws.numeric.is_none());
        type_text(&mut ws, "20");
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(ws.session.doc.bodies.len(), 8, "digits after the pull are the spacing");
    }

    /// #96: Done (a click, no Return) takes the Quantity being typed; it went in as the spacing.
    #[test]
    fn done_takes_the_typed_quantity() {
        let mut ws = patterning();
        type_text(&mut ws, "20");
        ws.open_pattern_count();
        type_text(&mut ws, "5");
        ws.commit_tool();
        assert_eq!(ws.session.doc.bodies.len(), 5);
        let xs: Vec<f64> = (ws.session.doc.bodies.iter()).map(|b| ws.session.kernel.bounds(&b.shape).unwrap().min.x).collect();
        assert!(xs.iter().any(|x| (x - 80.0).abs() < 1e-6), "20 apart, not 5: {xs:?}");
    }

    /// Done takes a typed angle too (only Return did).
    #[test]
    fn done_takes_the_typed_angle_of_a_circular_pattern() {
        let mut ws = patterning();
        ws.start_tool(super::super::toolbar::ToolKind::CircularPattern);
        let Tool::CircularPattern(t) = &mut ws.tool else { panic!() };
        t.pivot = glam::DVec3::new(-30.0, 0.0, 0.0);
        t.set_axis(glam::DVec3::Z, glam::DVec3::X);
        type_text(&mut ws, "90");
        ws.open_pattern_count();
        type_text(&mut ws, "2");
        ws.commit_tool();
        assert_eq!(ws.session.doc.bodies.len(), 2);
        let turned = ws.session.doc.bodies.iter().any(|b| ws.session.kernel.bounds(&b.shape).unwrap().min.y > 20.0);
        assert!(turned, "a quarter turn puts the copy up the Y axis");
        let mut ws = patterning();
        ws.start_tool(super::super::toolbar::ToolKind::CircularPattern);
        let Tool::CircularPattern(t) = &mut ws.tool else { panic!() };
        t.pivot = glam::DVec3::new(-30.0, 0.0, 0.0);
        t.set_axis(glam::DVec3::Z, glam::DVec3::X);
        t.count = 2;
        type_text(&mut ws, "90");
        ws.commit_tool();
        assert!(ws.session.doc.bodies.iter().any(|b| ws.session.kernel.bounds(&b.shape).unwrap().min.y > 20.0));
    }

    /// Done takes a typed angle in Revolve too.
    #[test]
    fn done_takes_the_typed_angle_of_a_revolve() {
        let mut ws = workspace();
        let s = &mut ws.session;
        s.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &serde_json::json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        let sketch = s.active_sketch.take().unwrap();
        s.selection = vec![Pick::Region { sketch, region: 0 }];
        ws.start_tool(super::super::toolbar::ToolKind::Revolve);
        let Tool::Revolve(t) = &mut ws.tool else { panic!("{:?}", ws.tool) };
        t.axis = Some(tools::AxisPick::Line(0));
        type_text(&mut ws, "90");
        ws.commit_tool();
        let v = ws.session.doc.bodies[0].shape.clone();
        let v = ws.session.kernel.mass_props(&v).unwrap().volume;
        assert!((v - std::f64::consts::PI * 1000.0 / 4.0).abs() < 1.0, "a quarter turn: {v}");
    }

    /// #96: Circular Pattern has arrows from the start; a clicked edge is gone around, filling the circle.
    #[test]
    fn a_circular_pattern_goes_around_a_clicked_edge_or_a_pulled_arrow() {
        let mut ws = patterning();
        ws.start_tool(super::super::toolbar::ToolKind::CircularPattern);
        let Tool::CircularPattern(t) = &ws.tool else { panic!() };
        assert!(t.about.is_none() && !t.ready());
        assert!((t.pivot - glam::DVec3::new(5.0, 5.0, 2.5)).length() < 1e-6, "the dot stands in the body's middle");
        assert!(t.request(360.0).is_none(), "nothing to make before an axis");
        let body = t.items[0];
        let n = ws.session.kernel.edge_count(&ws.session.doc.body(body).unwrap().shape).unwrap() as u32;
        let mut t = t.clone();
        let edge = (0..n).find(|e| t.set_about(&ws.session, tools::CircularAxis::Topo(Pick::Edge { body, edge: *e }))).unwrap();
        assert_eq!((t.count, t.angle), (6, 360.0), "a full circle (edge {edge})");
        assert!(t.request(t.angle).is_some() && t.ready());
        // Pulled back to nothing it asks for nothing.
        t.angle = 0.0;
        assert!(t.request(t.angle).is_none() && !t.ready());
    }

    /// #6: a quantity must be a whole number of 2 to 500; a bad one keeps the box open with the reason.
    #[test]
    fn a_bad_quantity_keeps_the_box_open() {
        let mut ws = patterning();
        ws.open_pattern_count();
        for bad in ["1", "2.5", "501"] {
            ws.numeric = Some(bad.into());
            ws.key(egui::Key::Enter, egui::Modifiers::NONE);
            assert_eq!(ws.numeric.as_deref(), Some(bad), "{bad} was taken");
            assert!(ws.numeric_error.as_deref().unwrap().contains("quantity"));
        }
        ws.session.doc.variables.push(Variable { name: "n".into(), expr: "3".into(), kind: VarKind::Number });
        ws.numeric = Some("n * 2".into());
        ws.key(egui::Key::Tab, egui::Modifiers::NONE);
        let Tool::Pattern(t) = &ws.tool else { panic!() };
        assert_eq!(t.count, 6, "a variable works in Quantity too");
    }

    /// #11: a variable picked from the box's list goes in after what's typed, or replaces a value taken from a pill.
    #[test]
    fn a_picked_variable_goes_into_the_value_box() {
        let mut ws = workspace();
        type_text(&mut ws, "20+");
        ws.insert_variable("wall");
        assert_eq!(ws.numeric.as_deref(), Some("20+wall"));
        ws.numeric = Some("15".into());
        ws.numeric_fresh = true;
        ws.insert_variable("wall");
        assert_eq!(ws.numeric.as_deref(), Some("wall"), "a value filled from a pill is replaced");
    }

    /// #12: a name that isn't a variable yet is offered for creating; creating it applies the box.
    #[test]
    fn a_missing_variable_is_created_from_the_box_and_the_value_applies() {
        let mut ws = patterning();
        ws.session.doc.units = Units::Inch;
        type_text(&mut ws, "2*w");
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert_eq!(ws.numeric.as_deref(), Some("2*w"), "the box stays open");
        let doc = &ws.session.doc;
        let missing = shapecraft_doc::expr::missing_variable("2*w", &doc.variables, ws.typed_kind(), doc.units.scale());
        assert_eq!(missing.as_deref(), Some("w"));
        assert!(ws.create_variable("w", "x", VarKind::Length).is_err(), "a value that doesn't work is refused");
        assert!(ws.session.doc.variables.is_empty());
        ws.create_variable("w", "1", VarKind::Length).unwrap();
        let w = &ws.session.doc.variables[0];
        assert!((shapecraft_doc::expr::eval(&w.expr, &[]).unwrap() - 25.4).abs() < 1e-9, "a bare 1 is an inch: {}", w.expr);
        let Tool::Pattern(t) = &ws.tool else { panic!("the pattern goes on with the value") };
        assert!((t.spacing - 50.8).abs() < 1e-9, "2 × 1 inch: {}", t.spacing);
        ws.key(egui::Key::Enter, egui::Modifiers::NONE);
        assert!(matches!(ws.tool, Tool::Idle), "the pattern applied");
        assert_eq!(ws.session.doc.bodies.len(), 3);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod undo_in_visualization {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use std::sync::Arc;

    fn frame(ctx: &egui::Context, ws: &mut Workspace, events: Vec<egui::Event>) {
        let input =
            egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| ws.ui(ui, None));
        out.textures_delta.clear();
    }

    fn undo_key(pressed: bool) -> egui::Event {
        egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND }
    }

    /// ⌘Z in Visualization takes the last modeling step back and stays in Visualization, the window still drawing
    /// (#38: the body went, every panel stayed hidden and the window stopped answering).
    #[test]
    fn undo_in_visualization_keeps_working() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Vis");
            let mut ws = Workspace::new(session, crate::settings::Settings::default());
            ws.session.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
            ws.session.execute("circle", &serde_json::json!({"center": [0, 0], "radius": 5})).unwrap();
            let k = ws.session.active_sketch.unwrap().0;
            ws.session.execute("extrude", &serde_json::json!({"sketch": k, "region": 0, "distance": 10})).unwrap();
            let ctx = egui::Context::default();
            frame(&ctx, &mut ws, Vec::new());
            ws.visualizing = true;
            for _ in 0..3 {
                frame(&ctx, &mut ws, Vec::new());
            }
            let bodies = ws.session.doc.bodies.len();
            frame(&ctx, &mut ws, vec![undo_key(true), undo_key(false)]);
            for _ in 0..5 {
                frame(&ctx, &mut ws, Vec::new());
            }
            let _ = tx.send((bodies, ws.session.doc.bodies.len(), ws.visualizing, matches!(ws.tool, Tool::Idle)));
        });
        let (before, after, still, idle) =
            rx.recv_timeout(std::time::Duration::from_secs(60)).expect("the window kept drawing frames after ⌘Z in Visualization");
        assert_eq!((before, after), (1, 0), "⌘Z took the extrude back");
        assert!(still, "and stayed in Visualization");
        assert!(idle);
    }
    /// #38, as the reporter did it: a material on the body in Visualization, then Ctrl+Z (window menu) twice and
    /// ⌘Z once: the material, then the pick, then the extrude go back, and the window keeps drawing.
    #[test]
    fn undo_after_a_material_in_visualization() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Vis");
            let mut ws = Workspace::new(session, crate::settings::Settings::default());
            ws.session.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
            ws.session.execute("circle", &serde_json::json!({"center": [0, 0], "radius": 5})).unwrap();
            ws.session.execute("circle", &serde_json::json!({"center": [0, 0], "radius": 3})).unwrap();
            let k = ws.session.active_sketch.unwrap().0;
            ws.session.execute("extrude", &serde_json::json!({"sketch": k, "region": 0, "distance": 10})).unwrap();
            let body = ws.session.doc.bodies[0].id;
            let ctx = egui::Context::default();
            frame(&ctx, &mut ws, Vec::new());
            ws.visualizing = true;
            frame(&ctx, &mut ws, Vec::new());
            let _ = ws.run("select", serde_json::json!({ "picks": [{"kind": "body", "body": body.0}] }));
            let _ = ws.run("set_appearance", serde_json::json!({ "items": [body.0], "appearance": "Aluminum" }));
            let had = ws.session.doc.bodies[0].appearance.is_some();
            for _ in 0..3 {
                frame(&ctx, &mut ws, Vec::new());
            }
            for _ in 0..2 {
                ws.act(Action::Command("undo"), "Undo");
                frame(&ctx, &mut ws, Vec::new());
            }
            frame(&ctx, &mut ws, vec![undo_key(true), undo_key(false)]);
            for _ in 0..5 {
                frame(&ctx, &mut ws, Vec::new());
            }
            let _ = tx.send((had, ws.session.doc.bodies.len(), ws.visualizing));
        });
        let (had, after, still) = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("the window kept drawing frames");
        assert!(had, "the material went on");
        assert_eq!(after, 0, "three undos: material, pick, extrude");
        assert!(still);
    }

    /// The same through the window menu's Ctrl+Z (Linux and Windows have no native menu), with the body picked as
    /// when giving it a color: the menu's undo goes through `run`, not the key handler.
    #[test]
    fn menu_undo_in_visualization_with_the_body_picked() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Vis");
            let mut ws = Workspace::new(session, crate::settings::Settings::default());
            ws.session.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
            ws.session.execute("circle", &serde_json::json!({"center": [0, 0], "radius": 5})).unwrap();
            let k = ws.session.active_sketch.unwrap().0;
            ws.session.execute("extrude", &serde_json::json!({"sketch": k, "region": 0, "distance": 10})).unwrap();
            let body = ws.session.doc.bodies[0].id;
            let ctx = egui::Context::default();
            frame(&ctx, &mut ws, Vec::new());
            ws.visualizing = true;
            ws.session.selection = vec![shapecraft_doc::Pick::Body { body }];
            for _ in 0..3 {
                frame(&ctx, &mut ws, Vec::new());
            }
            // The first takes the pick back (a change of selection is a step of its own), the second the extrude.
            ws.act(Action::Command("undo"), "Undo");
            frame(&ctx, &mut ws, Vec::new());
            ws.act(Action::Command("undo"), "Undo");
            for _ in 0..5 {
                frame(&ctx, &mut ws, Vec::new());
            }
            let _ = tx.send((ws.session.doc.bodies.len(), ws.visualizing));
        });
        let (after, still) = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("the window kept drawing frames");
        assert_eq!(after, 0);
        assert!(still);
    }

    fn key(k: egui::Key) -> [egui::Event; 2] {
        let e = |pressed| egui::Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        [e(true), e(false)]
    }

    /// + and - zoom the view in and out (#34); while a circle waits for its size, "-" is the start of a typed value
    /// and leaves the view alone.
    #[test]
    fn plus_and_minus_zoom() {
        let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Zoom");
        let mut ws = Workspace::new(session, crate::settings::Settings::default());
        let ctx = egui::Context::default();
        frame(&ctx, &mut ws, Vec::new());
        let d0 = ws.camera.distance;
        frame(&ctx, &mut ws, key(egui::Key::Plus).to_vec());
        let d1 = ws.camera.distance;
        assert!(d1 < d0 * 0.95, "+ zooms in: {d0} -> {d1}");
        frame(&ctx, &mut ws, key(egui::Key::Minus).to_vec());
        frame(&ctx, &mut ws, key(egui::Key::Minus).to_vec());
        let d2 = ws.camera.distance;
        assert!(d2 > d1 * 1.2, "- zooms out: {d1} -> {d2}");
        ws.session.execute("sketch", &serde_json::json!({"plane": "xy", "look": false})).unwrap();
        ws.tool = Tool::Circle { center: Some(glam::DVec2::ZERO) };
        let d3 = ws.camera.distance;
        frame(&ctx, &mut ws, key(egui::Key::Minus).to_vec());
        assert_eq!(ws.camera.distance, d3, "a circle waiting for its size keeps the view");
    }

    /// Slot: the typed length and radius stay as dimensions and nothing is left picked; Escape in Extend takes back all it
    /// grew, Return keeps it.
    #[test]
    fn slot_keeps_typed_sizes_and_extend_goes_back() {
        use crate::workspace::tools::Tool;
        use glam::DVec2;
        use serde_json::json;
        let mut s = shapecraft_engine::Session::new(std::sync::Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Slot");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        let mut ws = crate::workspace::Workspace::new(s, crate::settings::Settings::default());
        ws.add_slot(false, DVec2::new(2.0, 3.0), DVec2::new(12.0, 3.0), 2.5, Some(10.0), true);
        assert!(matches!(ws.tool, Tool::Slot { middle: false, a: None, b: None, length: None, radius: None }), "ready for the next one");
        assert!(ws.session.selection.is_empty());
        let sk = ws.session.active_sketch().unwrap();
        assert!(
            sk.constraints.iter().any(
                |c| matches!(c, shapecraft_doc::Constraint::Distance { a, value, .. } if a.entity() == 4 && (value - 10.0).abs() < 1e-9)
            )
        );
        assert!(sk.constraints.iter().any(|c| matches!(c, shapecraft_doc::Constraint::Radius { value, .. } if (value - 2.5).abs() < 1e-9)));
        // One undo takes the slot with both its sizes.
        let depth = ws.session.undo_depth();
        ws.add_slot(true, DVec2::new(40.0, 3.0), DVec2::new(45.0, 3.0), 2.5, Some(10.0), true);
        assert_eq!(ws.session.undo_depth(), depth + 1);
        assert!(ws.run("undo", json!({})).is_some());
        assert_eq!(ws.session.active_sketch().unwrap().entities.len(), 5);
        // Return fixes a typed length and radius one by one; Return alone then finishes.
        let typed = |ws: &mut crate::workspace::Workspace, v: &str| {
            ws.numeric = (!v.is_empty()).then(|| v.to_string());
            ws.commit_tool();
        };
        ws.tool = Tool::Slot { middle: false, a: Some(DVec2::new(0.0, 50.0)), b: Some(DVec2::new(9.0, 50.0)), length: None, radius: None };
        typed(&mut ws, "2");
        assert!(matches!(ws.tool, Tool::Slot { radius: Some(r), .. } if (r - 2.0).abs() < 1e-9));
        typed(&mut ws, "12");
        assert!(matches!(ws.tool, Tool::Slot { b: Some(b), length: Some(_), .. } if (b.x - 12.0).abs() < 1e-9));
        typed(&mut ws, "");
        assert_eq!(ws.session.active_sketch().unwrap().entities.len(), 10);
        assert!(ws.run("undo", json!({})).is_some());
        // ⌘Z with a drawing tool: mid-shape nothing, else the last step, and the tool stays armed.
        ws.session.execute("line", &json!({"a": [-50, 3], "b": [-40, 3]})).unwrap();
        let depth = ws.session.undo_depth();
        for (first, left) in [(Some(DVec2::ZERO), depth), (None, depth - 1)] {
            ws.tool = Tool::Rectangle { first };
            ws.act(Action::Command("undo"), "Undo");
            assert_eq!(ws.session.undo_depth(), left);
            assert!(matches!(ws.tool, Tool::Rectangle { first: f } if f == first));
        }
        // A rectangle's typed width is fixed the same way, and the height is next.
        ws.tool = Tool::Rectangle { first: Some(DVec2::ZERO) };
        typed(&mut ws, "12");
        assert_eq!(ws.rect_width, Some(12.0));
        assert!(matches!(ws.tool, Tool::Rectangle { first: Some(_) }));
        ws.rect_width = None;
        ws.tool = Tool::Idle;
        ws.session.execute("line", &json!({"a": [-5, 3], "b": [-3, 3]})).unwrap();
        for keep in [false, true] {
            ws.start_tool(crate::workspace::toolbar::ToolKind::Extend);
            let before = ws.session.active_sketch().unwrap().entities.clone();
            let depth = ws.session.undo_depth();
            for _ in 0..2 {
                let _ = ws.run("extend", json!({"entity": 5, "at": [-3, 3]}));
            }
            assert_eq!(ws.session.undo_depth(), depth + 2);
            if keep {
                ws.commit_tool();
                assert_ne!(ws.session.active_sketch().unwrap().entities, before);
            } else {
                ws.escape();
                assert_eq!(ws.session.active_sketch().unwrap().entities, before);
            }
            assert!(matches!(ws.tool, Tool::Idle));
            assert!(ws.session.active_sketch().is_some(), "still in the sketch");
        }
    }
}
