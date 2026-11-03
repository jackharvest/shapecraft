//! The right-hand column (sidebar toggles, view, snapping, units) and the orientation view cube.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::DVec3;
use shapecraft_render::StandardView;

use super::right_popover::constraint_settings;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use crate::workspace::toolbar::{self, Action, ToolDef, UiAction};
use crate::workspace::tools::Tool;
use crate::workspace::{RightPop, Workspace};

impl Workspace {
    /// Fillet | Chamfer segmented switch under the banner while the tool runs.
    /// Visualization's right column, as observed: sidebar toggle and view (a strip), then Capture and Generative
    /// Render.
    fn visual_column(&mut self, ui: &mut Ui, x: f32) {
        let vp = self.viewport_rect;
        let gap = toolbar::GROUP_GAP;
        let top = vp.top() + 9.0;
        // The sidebar toggle and the view button each in their own group (as observed, L22).
        let toggle = ToolDef { icon: Icon::SidebarToggle, label: "", sub: None, keys: "", action: Action::Soon, flyout: false };
        if toolbar::group(ui, pos2(x, top), &[toggle], &[self.vis_sidebar], false, "vis-toggle").is_some() {
            self.vis_sidebar = !self.vis_sidebar;
        }
        let view_y = top + toolbar::group_height(1) + gap;
        let view_on = self.right_pop.is_some_and(|(p, ..)| p == RightPop::View);
        let view = ToolDef { icon: Icon::Origin, label: "", sub: None, keys: "", action: Action::Soon, flyout: false };
        if toolbar::group(ui, pos2(x, view_y), &[view], &[view_on], false, "vis-view").is_some() {
            self.right_pop = if view_on { None } else { Some((RightPop::View, pos2(x, view_y), self.now)) };
            self.popup_opened_at = self.now;
        }
        let cube = Rect::from_center_size(pos2(x - 52.0, top + 30.0), vec2(72.0, 72.0));
        self.view_cube(ui, cube);
        // Capture and Generative Render share a group; Generative Render sits on blue.
        let defs = [
            ToolDef {
                icon: Icon::Screenshot,
                label: "Capture",
                sub: None,
                keys: "",
                action: Action::Ui(UiAction::Screenshot),
                flyout: false,
            },
            ToolDef { icon: Icon::CameraSparkle, label: "Generative Render", sub: None, keys: "", action: Action::Soon, flyout: false },
        ];
        let y = view_y + toolbar::group_height(1) + gap;
        if let Some(i) = toolbar::group(ui, pos2(x, y), &defs, &[false, false], false, "vis-capture") {
            self.act(defs[i].action, defs[i].label);
        }
        let blue = Rect::from_min_size(pos2(x, y + 1.0 + Tokens::TOOL_PITCH), vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH)).shrink(3.0);
        ui.painter().rect_filled(blue, radius(7), Tokens::accent());
        icons::paint(ui.painter(), Rect::from_center_size(blue.center(), vec2(28.0, 28.0)), Icon::CameraSparkle, Color32::WHITE);
        for (i, def) in defs.iter().enumerate() {
            toolbar::chip(ui, pos2(x - 6.0, y + 1.0 + Tokens::TOOL_PITCH * (i as f32 + 0.5)), def, true, false);
        }
        let sep_y = y + toolbar::group_height(2) + 10.0;
        ui.painter().line_segment([pos2(x + 11.0, sep_y), pos2(x + 33.0, sep_y)], Stroke::new(2.0, Tokens::tool_separator()));
    }

    pub(super) fn right_column(&mut self, ui: &mut Ui) {
        let vp = self.viewport_rect;
        // 8 px from the History sidebar (measured, L05).
        let x = vp.right() - 5.0 - Tokens::TOOL_BUTTON;
        if self.visualizing {
            self.visual_column(ui, x);
            return;
        }
        if self.drawings.is_some() {
            crate::workspace::drawings::right_column(self, ui);
            return;
        }
        // The snap / grid / origin strip shows beside the History sidebar; with it closed the cube takes
        // the corner (both as observed).
        // The snap / grid size / view strip, with the cube beside it (always, as observed this session).
        let strip_bottom = {
            let strip = Rect::from_min_size(pos2(x, vp.top() + 9.0), vec2(Tokens::TOOL_BUTTON, toolbar::group_height(3)));
            ui.painter().rect_filled(strip, radius(Tokens::TOOL_RADIUS), Tokens::button());
            ui.painter().rect_stroke(strip, radius(Tokens::TOOL_RADIUS), Stroke::new(1.0, Tokens::tool_border()), StrokeKind::Inside);
            let cell = |i: usize| {
                Rect::from_min_size(
                    pos2(x, strip.top() + 1.0 + i as f32 * Tokens::TOOL_PITCH),
                    vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH),
                )
            };
            let pops = [RightPop::Snap, RightPop::Units, RightPop::View];
            for (i, pop) in pops.iter().enumerate() {
                let r = cell(i);
                let resp = ui.interact(r, ui.scope_id().with(("strip", i)), Sense::click());
                let open = self.right_pop.is_some_and(|(p, ..)| p == *pop);
                if open {
                    ui.painter().rect_filled(r.shrink(3.0), radius(7), Tokens::button_active());
                } else if resp.hovered() {
                    ui.painter().rect_filled(r.shrink(3.0), radius(7), Tokens::button_hover());
                }
                if resp.clicked() {
                    self.right_pop = if open { None } else { Some((*pop, r.min, self.now)) };
                    self.popup_opened_at = self.now;
                }
            }
            icons::paint(ui.painter(), Rect::from_center_size(cell(0).center(), vec2(27.0, 27.0)), Icon::Snap, Tokens::icon());
            let step = self.grid_locked.unwrap_or_else(|| shapecraft_render::grid_step(self.camera.world_per_pixel(self.size().y)));
            let units = self.session.doc.units;
            let mut num = format!("{:.3}", step / units.scale());
            while num.contains('.') && (num.ends_with('0') || num.ends_with('.')) {
                num.pop();
            }
            // Each line centred on its own (as observed).
            let c = cell(1).center();
            ui.painter().text(c - vec2(0.0, 6.0), Align2::CENTER_CENTER, num, theme::mono(9.5), Tokens::text());
            ui.painter().text(c + vec2(0.0, 6.0), Align2::CENTER_CENTER, units.suffix(), theme::mono(9.5), Tokens::text());
            icons::paint(ui.painter(), Rect::from_center_size(cell(2).center(), vec2(28.0, 28.0)), Icon::Origin, Tokens::icon());
            let cube = Rect::from_center_size(pos2(x - 52.0, strip.top() + 40.0), vec2(72.0, 72.0));
            self.view_cube(ui, cube);
            strip.bottom()
        };

        // Display Modes, Screenshot, History.
        let defs = [
            ToolDef {
                icon: match self.display {
                    shapecraft_render::DisplayMode::Zebra { .. } => Icon::DisplayZebra,
                    shapecraft_render::DisplayMode::Curvature { .. } => Icon::DisplayCurvature,
                    _ => Icon::DisplayModes,
                },
                label: "Display Modes",
                sub: Some(self.display.label()),
                keys: "",
                action: Action::Ui(UiAction::DisplayModes),
                flyout: false,
            },
            ToolDef {
                icon: Icon::Screenshot,
                label: "Screenshot",
                sub: None,
                keys: "",
                action: Action::Ui(UiAction::Screenshot),
                flyout: false,
            },
            ToolDef {
                icon: Icon::History,
                label: "History",
                sub: None,
                keys: "⌥⌘P",
                action: Action::Ui(UiAction::ToggleHistory),
                flyout: false,
            },
        ];
        // Display Modes and Screenshot share a group, History has its own, then a short line (measured, L05).
        let top = strip_bottom + toolbar::GROUP_GAP;
        let hist_y = top + toolbar::group_height(2) + toolbar::GROUP_GAP;
        if let Some(i) = toolbar::group(ui, pos2(x, top), &defs[..2], &[false, false], false, "right-view") {
            self.act(defs[i].action, defs[i].label);
        }
        if toolbar::group(ui, pos2(x, hist_y), &defs[2..], &[self.show_history], false, "right-history").is_some() {
            self.act(defs[2].action, defs[2].label);
        }
        // A failing step puts a red "!" badge on the History button (observed s14-008).
        if self.session.doc.history.iter().chain(&self.session.doc.future).any(|h| h.error.is_some() && !h.quiet) {
            let c = pos2(x + 22.0 + 6.0, hist_y + 1.0 + Tokens::TOOL_PITCH / 2.0 + 7.0);
            let p = ui.painter();
            p.circle_filled(c, 6.0, theme::c(0xff3c30));
            p.line_segment([c - vec2(0.0, 3.2), c + vec2(0.0, 0.8)], Stroke::new(1.5, Color32::WHITE));
            p.circle_filled(c + vec2(0.0, 3.0), 0.9, Color32::WHITE);
        }
        for (i, def) in defs.iter().enumerate() {
            let cy = if i < 2 { top + 1.0 + Tokens::TOOL_PITCH * (i as f32 + 0.5) } else { hist_y + 1.0 + Tokens::TOOL_PITCH / 2.0 };
            // The label is a button too (as in the reference: clicking "Display Modes" opens its popover).
            let chip = toolbar::chip(ui, pos2(x - 6.0, cy), def, true, false);
            if ui.interact(chip, ui.scope_id().with(("right-chip", i)), Sense::click()).clicked() {
                self.act(def.action, def.label);
            }
        }
        self.display_menu_at = pos2(x, top);
        // The short line under History shows only with a selection or while sketching (D-sel, D-line, L05); an idle
        // view has none (D-idle, s30-100).
        if !self.session.selection.is_empty() || self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. }) {
            let sep_y = hist_y + toolbar::group_height(1) + 10.0;
            ui.painter().line_segment([pos2(x + 11.0, sep_y), pos2(x + 33.0, sep_y)], Stroke::new(2.0, Tokens::tool_separator()));
        }

        // Sketch constraints, as observed (L18): a column of icon buttons in groups (settings; the nine
        // constraints; Disconnect; Lock and Make Construction) centred on the view, each with its chip on the left,
        // ghosted until something it applies to is selected. Placing text hides them (s11-074).
        if (self.session.active_sketch.is_some() || matches!(self.tool, Tool::PickPlane { .. })) && !matches!(self.tool, Tool::Text { .. })
        {
            let icons = [
                Icon::CParallel,
                Icon::CPerpendicular,
                Icon::CTangent,
                Icon::CCoincident,
                Icon::CMidpoint,
                Icon::CConcentric,
                Icon::CHorizVert,
                Icon::CEqual,
                Icon::CSymmetry,
                Icon::Disconnect,
                Icon::Lock,
                Icon::MakeConstruction,
            ];
            let rows = crate::workspace::constraints::CHIPS;
            // With only locked elements picked, Lock reads Unlock (s14-146).
            let unlock = self.picked_all_locked();
            let def = |i: usize| -> ToolDef {
                let (label, keys, kind) = rows[i];
                let label = if kind == "lock" && unlock { "Unlock" } else { label };
                ToolDef { icon: icons[i], label, sub: None, keys, action: Action::Soon, flyout: false }
            };
            let settings =
                ToolDef { icon: Icon::Gear, label: "Constraint Settings", sub: None, keys: "", action: Action::Soon, flyout: false };
            // (first row of CHIPS, count) per group; the settings button stands alone above them.
            let groups: [(usize, usize); 3] = [(0, 9), (9, 1), (10, 2)];
            let gap = toolbar::GROUP_GAP;
            let total = toolbar::group_height(1) + groups.iter().map(|g| toolbar::group_height(g.1) + gap).sum::<f32>();
            let mut y = (vp.center().y - total / 2.0).max(vp.top() + 290.0);
            let mut clicked = None;
            let gear_open = self.constraint_pop.is_some();
            let r = toolbar::chip(ui, pos2(x - 6.0, y + 1.0 + Tokens::TOOL_PITCH / 2.0), &settings, true, false);
            let chip_clicked = ui.interact(r, ui.scope_id().with("constraints"), Sense::click()).clicked();
            if toolbar::group(ui, pos2(x, y), &[settings], &[gear_open], false, "c-settings").is_some() || chip_clicked {
                self.constraint_pop = if gear_open { None } else { Some((pos2(x, y), self.now)) };
            }
            y += toolbar::group_height(1) + gap;
            for (gi, (first, n)) in groups.iter().enumerate() {
                let defs: Vec<ToolDef> = (*first..first + n).map(def).collect();
                let dim: Vec<bool> = (*first..first + n).map(|i| !self.constraint_applies(rows[i].2)).collect();
                if let Some(k) = toolbar::group_full(ui, pos2(x, y), &defs, &[], &[], &dim, false, &format!("c-group{gi}"), None) {
                    clicked = Some(rows[first + k].2);
                }
                for (k, d) in defs.iter().enumerate() {
                    let cy = y + 1.0 + Tokens::TOOL_PITCH * (k as f32 + 0.5);
                    let r = toolbar::chip(ui, pos2(x - 6.0, cy), d, true, dim[k]);
                    if ui.interact(r, ui.scope_id().with(("constraint", d.label)), Sense::click()).clicked() && !dim[k] {
                        clicked = Some(rows[first + k].2);
                    }
                }
                y += toolbar::group_height(*n) + gap;
            }
            if let Some(kind) = clicked {
                self.apply_constraint(kind);
            }
            if let Some((at, opened)) = self.constraint_pop {
                constraint_settings(self, ui, at, opened);
            }
        } else {
            self.constraint_pop = None;
        }
    }

    /// A tutorial keeps only the view cube, in the corner (dragging it orbits, which the lessons teach).
    pub(in crate::workspace) fn tutorial_cube(&mut self, ui: &mut Ui) {
        let vp = self.viewport_rect;
        let cube = Rect::from_center_size(pos2(vp.right() - 62.0, vp.top() + 58.0), vec2(72.0, 72.0));
        self.view_cube(ui, cube);
    }

    /// The orientation cube, as observed: a dark cube with bevelled edges and corners that turns with the camera,
    /// its face names drawn on the faces, and the X/Y/Z axes running out from one corner. Clicking a face picks
    /// that standard view, an edge or corner the angled view between them; a double-click resets to the default.
    /// Dragging it orbits the view like dragging empty space, and the camera stays where it is let go (s30-003/004).
    fn view_cube(&mut self, ui: &mut Ui, r: Rect) {
        let resp = ui.interact(r, ui.scope_id().with("viewcube"), Sense::click_and_drag());
        let delta = resp.drag_delta();
        if resp.dragged_by(egui::PointerButton::Primary) && delta != egui::Vec2::ZERO {
            self.anim = None;
            let (sx, sy) = self.settings.orbit_signs();
            self.camera.orbit(delta.x as f64 * sx, delta.y as f64 * sy);
        }
        let (right, up) = self.camera.basis();
        let back = self.camera.back();
        let s = r.width() as f64 * 0.25;
        let proj = |p: DVec3| r.center() + vec2((p.dot(right) * s) as f32, (-p.dot(up) * s) as f32);
        let k = 0.8; // where the bevels start
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let mut pieces: Vec<(DVec3, Vec<DVec3>)> = Vec::new();
        for n in axes.iter().flat_map(|a| [*a, -*a]) {
            let (u, v) = face_frame(n);
            pieces.push((
                n,
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().map(|&(a, b)| n + u * (a * k) + v * (b * k)).collect(),
            ));
        }
        for i in 0..3 {
            for j in i + 1..3 {
                let t = axes[3 - i - j];
                for (si, sj) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                    let (n1, n2) = (axes[i] * si, axes[j] * sj);
                    let quad = vec![n1 + n2 * k - t * k, n1 + n2 * k + t * k, n2 + n1 * k + t * k, n2 + n1 * k - t * k];
                    pieces.push(((n1 + n2).normalize(), quad));
                }
            }
        }
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let c = DVec3::new(sx, sy, sz);
                    let tri = vec![DVec3::new(sx, sy * k, sz * k), DVec3::new(sx * k, sy, sz * k), DVec3::new(sx * k, sy * k, sz)];
                    pieces.push((c.normalize(), tri));
                }
            }
        }
        let mut visible: Vec<&(DVec3, Vec<DVec3>)> = pieces.iter().filter(|(n, _)| n.dot(back) > 1e-3).collect();
        visible.sort_by(|a, b| a.0.dot(back).total_cmp(&b.0.dot(back)));
        let hover = resp.hover_pos().filter(|_| !resp.dragged());
        let hovered = visible
            .iter()
            .rev()
            .find(|(_, pts)| hover.is_some_and(|h| point_in_poly(h, &pts.iter().map(|p| proj(*p)).collect::<Vec<_>>())))
            .map(|(n, _)| *n);

        // Axes: drawn under the cube where the edge they follow is out of sight, over it otherwise.
        let o = DVec3::splat(-1.12);
        let axis_cols = [theme::c(0xc8413b), theme::c(0x4fa646), theme::c(0x2f6fd0)];
        let paint_axis = |ui: &Ui, i: usize| {
            let d = axes[i];
            if d.dot(back).abs() > 0.97 {
                return; // pointing at the viewer: nothing to see
            }
            ui.painter().line_segment([proj(o), proj(o + d * 2.75)], Stroke::new(1.6, axis_cols[i]));
            ui.painter().text(proj(o + d * 3.15), Align2::CENTER_CENTER, ["X", "Y", "Z"][i], theme::body(11.0), axis_cols[i]);
        };
        let axis_seen = |i: usize| axes.iter().enumerate().any(|(j, a)| j != i && (-*a).dot(back) > 0.0);
        for i in (0..3).filter(|i| !axis_seen(*i)) {
            paint_axis(ui, i);
        }
        let light = DVec3::new(-0.3, -0.4, 1.0).normalize();
        for (n, pts) in &visible {
            let face = n.abs().max_element() > 0.99;
            let base: f64 = if face { 58.0 } else { 82.0 };
            let lum = (base * (0.85 + 0.25 * n.dot(light).max(0.0))).min(255.0) as u8;
            // Light: the faces read #F2F2F2/#E9E9E9/#FCFCFC where the dark ones read #333333/#3B3B3B/#494949 (s23).
            let lum = if theme::is_light() { (242.0 + (lum as f32 - 51.0) * 0.5).clamp(0.0, 255.0) as u8 } else { lum };
            let fill = if hovered == Some(*n) { theme::c(0x3a64c8) } else { Color32::from_gray(lum) };
            let poly: Vec<Pos2> = pts.iter().map(|p| proj(*p)).collect();
            ui.painter().add(Shape::convex_polygon(poly, fill, Stroke::new(0.6, fill)));
            if face && n.dot(back) > 0.2 {
                let label = match (n.x.round() as i32, n.y.round() as i32, n.z.round() as i32) {
                    (0, 0, 1) => "Top",
                    (0, 0, -1) => "Bottom",
                    (0, -1, 0) => "Front",
                    (0, 1, 0) => "Back",
                    (1, 0, 0) => "Right",
                    _ => "Left",
                };
                let (u, v) = face_frame(*n);
                let alpha = ((n.dot(back) - 0.2) / 0.3).clamp(0.0, 1.0);
                paint_on_plane(
                    ui,
                    label,
                    *n * 1.001,
                    u,
                    v,
                    1.5,
                    &proj,
                    theme::themed(Color32::from_gray(176)).gamma_multiply(alpha as f32),
                );
            }
        }
        for i in (0..3).filter(|i| axis_seen(*i)) {
            paint_axis(ui, i);
        }

        // Looking square at a face (orthographic): two round buttons above the cube turn the view a quarter turn,
        // clockwise on the left, counter-clockwise on the right (as observed while sketching face-on).
        if self.camera.is_ortho() && self.anim.is_none() {
            for (dx, turn) in [(-43.0f32, -1.0f64), (43.0, 1.0)] {
                let c = r.center() + vec2(dx, -27.0);
                let b =
                    ui.interact(Rect::from_center_size(c, vec2(22.0, 22.0)), ui.scope_id().with(("cube-turn", dx as i32)), Sense::click());
                let p = ui.painter();
                p.circle_filled(c, 10.5, if b.hovered() { theme::c(0x4a4a4c) } else { theme::c(0x343436) });
                p.circle_stroke(c, 10.5, Stroke::new(1.0, theme::c(0x48484a)));
                // A curved arrow: an arc over the top with its head at the far end.
                let arc: Vec<Pos2> = (0..=12)
                    .map(|i| {
                        let a = std::f32::consts::PI * (1.15 - 0.85 * i as f32 / 12.0);
                        let a = if turn < 0.0 { a } else { std::f32::consts::PI - a };
                        c + vec2(a.cos() * 4.5, -a.sin() * 4.5 + 1.5)
                    })
                    .collect();
                let (end, prev) = (arc[12], arc[10]);
                let dir = (end - prev).normalized();
                let side = vec2(-dir.y, dir.x);
                p.add(Shape::line(arc, Stroke::new(1.6, Color32::WHITE)));
                p.add(Shape::convex_polygon(
                    vec![end + dir * 2.6, end - dir * 1.4 + side * 2.6, end - dir * 1.4 - side * 2.6],
                    Color32::WHITE,
                    Stroke::NONE,
                ));
                if b.clicked() {
                    let mut to = self.camera;
                    to.roll += turn * std::f64::consts::FRAC_PI_2;
                    self.animate_to(to);
                }
            }
        }

        if resp.double_clicked() {
            self.standard_view(StandardView::Default);
        } else if resp.clicked()
            && let Some(n) = hovered
        {
            let view = match (n.x.round() as i32, n.y.round() as i32, n.z.round() as i32) {
                _ if n.abs().max_element() < 0.99 => None,
                (0, 0, 1) => Some(StandardView::Top),
                (0, 0, -1) => Some(StandardView::Bottom),
                (0, -1, 0) => Some(StandardView::Front),
                (0, 1, 0) => Some(StandardView::Back),
                (1, 0, 0) => Some(StandardView::Right),
                _ => Some(StandardView::Left),
            };
            match view {
                Some(v) => self.standard_view(v),
                None => {
                    // An edge or corner goes orthographic too, at the same zoom (side by side, 2026-11-02); orbiting
                    // turns it back to perspective as for the faces.
                    let mut c = shapecraft_render::Camera::facing(n, DVec3::Z, self.camera.target, self.camera.distance);
                    c.ortho = 1.0;
                    self.animate_to(c);
                }
            }
        }
    }
}

/// A face's right and up directions seen from outside (names read upright on the sides, X to the right on top).
fn face_frame(n: DVec3) -> (DVec3, DVec3) {
    if n.z.abs() > 0.5 { (DVec3::X, DVec3::Y * n.z.signum()) } else { (DVec3::Z.cross(n), DVec3::Z) }
}

/// Paints text lying on a 3D plane through `center` (spanned by `u` right and `v` up), `width` world units wide
/// at most: the text is tessellated flat and its vertices mapped through `proj`.
fn paint_on_plane(ui: &Ui, text: &str, center: DVec3, u: DVec3, v: DVec3, width: f64, proj: &dyn Fn(DVec3) -> Pos2, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::strong(12.0), color);
    let size = galley.size();
    let scale = (width / size.x.max(1.0) as f64).min(0.9 / size.y.max(1.0) as f64);
    let shape = egui::epaint::ClippedShape::new(Rect::EVERYTHING, Shape::galley(Pos2::ZERO, galley, color));
    let ppp = ui.ctx().pixels_per_point();
    for prim in ui.ctx().tessellate(vec![shape], ppp) {
        if let egui::epaint::Primitive::Mesh(mut mesh) = prim.primitive {
            for vert in &mut mesh.vertices {
                let x = (vert.pos.x - size.x / 2.0) as f64 * scale;
                let y = (vert.pos.y - size.y / 2.0) as f64 * scale;
                vert.pos = proj(center + u * x - v * y);
            }
            ui.painter().add(Shape::mesh(mesh));
        }
    }
}

fn point_in_poly(p: Pos2, q: &[Pos2]) -> bool {
    let n = q.len();
    let mut sign = 0.0f32;
    for i in 0..n {
        let a = q[i];
        let b = q[(i + 1) % n];
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}
