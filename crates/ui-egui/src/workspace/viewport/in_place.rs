//! Tools that work in place in the view: Text (its dialog and handles), Sketch Fillet's handle, and the
//! "Cancel / Done" buttons at the bottom.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use glam::DVec2;
use shapecraft_doc::{SketchEntity, format_length};

use super::paint::{axis_pill, leader_dim};
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

impl Workspace {
    /// Text, 26.200 (s11-072..074): first the Add Text dialog in the middle of the view (text, font, capital height,
    /// alignment; Cancel / Continue), then the block at the sketch's origin with move arrows, a free-move square and
    /// a turn handle, and Cancel (Esc) / Done (Return) at the bottom.
    pub(super) fn text_field(&mut self, ui: &mut Ui) {
        let Tool::Text { at, .. } = &self.tool else { return };
        if at.is_none() {
            self.text_dialog(ui);
        } else {
            self.text_handles(ui);
        }
    }

    fn text_dialog(&mut self, ui: &mut Ui) {
        let vp = self.viewport_rect;
        let rect = Rect::from_center_size(vp.center(), vec2(320.0, 286.0));
        let p = ui.painter().clone();
        p.rect_filled(rect.expand(1.0), radius(12), theme::c(0x48474a));
        p.rect_filled(rect, radius(11), theme::c(0x232224));
        p.text(rect.min + vec2(16.0, 20.0), Align2::LEFT_CENTER, "Add Text", theme::strong(13.0), Tokens::text_strong());
        let ready = matches!(&self.tool, Tool::Text { text, .. } if !text.trim().is_empty());
        let cont = Rect::from_min_size(pos2(rect.right() - 92.0, rect.top() + 9.0), vec2(76.0, 22.0));
        let cancel = Rect::from_min_size(pos2(cont.left() - 66.0, cont.top()), vec2(62.0, 22.0));
        let blue = if ready { Tokens::accent() } else { Tokens::accent().gamma_multiply(0.45) };
        p.rect_filled(cancel, radius(5), theme::c(0x5a595c));
        p.text(cancel.center(), Align2::CENTER_CENTER, "Cancel", theme::body(13.0), Tokens::text_strong());
        p.rect_filled(cont, radius(5), blue);
        p.text(
            cont.center(),
            Align2::CENTER_CENTER,
            "Continue",
            theme::body(13.0),
            if ready { Color32::WHITE } else { Tokens::text_dim() },
        );
        p.line_segment([pos2(rect.left(), rect.top() + 40.0), pos2(rect.right(), rect.top() + 40.0)], Stroke::new(1.0, theme::c(0x3a393c)));
        let card = |r: Rect| p.rect_filled(r, radius(8), theme::c(0x2c2b2e));
        let text_box = Rect::from_min_size(rect.min + vec2(18.0, 54.0), vec2(rect.width() - 36.0, 64.0));
        card(text_box);
        let rows = Rect::from_min_size(pos2(text_box.left(), text_box.bottom() + 8.0), vec2(text_box.width(), 84.0));
        card(rows);
        p.line_segment(
            [pos2(rows.left() + 10.0, rows.center().y), pos2(rows.right(), rows.center().y)],
            Stroke::new(1.0, theme::c(0x3a393c)),
        );
        let align_row = Rect::from_min_size(pos2(rows.left(), rows.bottom() + 8.0), vec2(rows.width(), 42.0));
        card(align_row);
        let font = self.text_font.clone().or_else(shapecraft_doc::text::font_name).unwrap_or_else(|| "System".into());
        p.text(pos2(rows.left() + 10.0, rows.top() + 21.0), Align2::LEFT_CENTER, "Font", theme::body(13.0), Tokens::text_strong());
        // The family, then a ⋯ button that opens the installed families (as observed, s11-070).
        let dots = Rect::from_center_size(pos2(rows.right() - 20.0, rows.top() + 21.0), vec2(24.0, 20.0));
        let font_row = Rect::from_min_max(pos2(rows.left() + 60.0, rows.top() + 8.0), pos2(rows.right() - 4.0, rows.top() + 34.0));
        let font_resp = ui.interact(font_row, ui.scope_id().with("text-font"), Sense::click());
        p.rect_filled(dots, radius(5), if font_resp.hovered() { theme::c(0x5a595c) } else { theme::c(0x3a393c) });
        p.text(dots.center(), Align2::CENTER_CENTER, "⋯", theme::body(13.0), Tokens::text_strong());
        p.text(
            pos2(dots.left() - 8.0, rows.top() + 21.0),
            Align2::RIGHT_CENTER,
            format!("{font}, Regular"),
            theme::body(13.0),
            Tokens::text_strong(),
        );
        if font_resp.clicked() {
            self.font_list = if self.font_list.is_some() { None } else { Some(String::new()) };
        }
        self.font_picker(ui, pos2(rect.right() + 8.0, rect.top()));
        p.text(
            pos2(rows.left() + 10.0, rows.bottom() - 21.0),
            Align2::LEFT_CENTER,
            "Capital Height",
            theme::body(13.0),
            Tokens::text_strong(),
        );
        p.text(pos2(rows.right() - 10.0, rows.bottom() - 21.0), Align2::RIGHT_CENTER, "mm", theme::body(13.0), Tokens::text_strong());
        p.text(
            pos2(align_row.left() + 10.0, align_row.center().y),
            Align2::LEFT_CENTER,
            "Alignment",
            theme::body(13.0),
            Tokens::text_strong(),
        );
        let seg = Rect::from_min_size(pos2(align_row.right() - 136.0, align_row.center().y - 11.0), vec2(126.0, 22.0));
        p.rect_filled(seg, radius(6), theme::c(0x3a393c));
        let (clicked_cancel, clicked_cont) = (
            ui.interact(cancel, ui.scope_id().with("text-cancel"), Sense::click()).clicked(),
            ui.interact(cont, ui.scope_id().with("text-continue"), Sense::click()).clicked(),
        );
        let mut picked_align = None;
        for k in 0..3u8 {
            let cell = Rect::from_min_size(pos2(seg.left() + k as f32 * 42.0, seg.top()), vec2(42.0, 22.0));
            if ui.interact(cell, ui.scope_id().with(("text-align", k)), Sense::click()).clicked() {
                picked_align = Some(k);
            }
        }
        let Tool::Text { text, size, align, .. } = &mut self.tool else { return };
        if let Some(k) = picked_align {
            *align = k;
        }
        for k in 0..3u8 {
            let cell = Rect::from_min_size(pos2(seg.left() + k as f32 * 42.0, seg.top()), vec2(42.0, 22.0));
            if *align == k {
                p.rect_filled(cell.shrink(2.0), radius(5), theme::c(0x6b6a6e));
            }
            // Three bars ranged left, centered or right.
            for (row, w) in [(0, 14.0), (1, 10.0), (2, 14.0), (3, 8.0)] {
                let y = cell.top() + 6.0 + row as f32 * 3.4;
                let x0 = match k {
                    0 => cell.center().x - 7.0,
                    1 => cell.center().x - w / 2.0,
                    _ => cell.center().x + 7.0 - w,
                };
                p.line_segment([pos2(x0, y), pos2(x0 + w, y)], Stroke::new(1.2, Tokens::text_strong()));
            }
        }
        let edit = ui.put(
            text_box.shrink2(vec2(8.0, 6.0)),
            egui::TextEdit::multiline(text)
                .id(ui.scope_id().with("sketch-text"))
                .hint_text("Type here…")
                .frame(egui::Frame::NONE)
                .font(theme::body(13.0)),
        );
        if !ui.memory(|m| m.has_focus(ui.scope_id().with("text-height")) || m.has_focus(ui.scope_id().with("font-search"))) {
            edit.request_focus();
        }
        let mut h = format!("{size}");
        let hr = Rect::from_min_size(pos2(rows.right() - 132.0, rows.bottom() - 32.0), vec2(90.0, 22.0));
        p.rect_filled(hr, radius(5), theme::c(0x3a393c));
        let hresp = ui.put(
            hr.shrink2(vec2(6.0, 2.0)),
            egui::TextEdit::singleline(&mut h)
                .id(ui.scope_id().with("text-height"))
                .align(egui::Align2::RIGHT_CENTER)
                .frame(egui::Frame::NONE)
                .font(theme::body(13.0)),
        );
        if hresp.changed()
            && let Ok(v) = h.trim().parse::<f64>()
            && v.is_finite()
            && v > 0.01
            && v < 10_000.0
        {
            *size = v;
        }
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if clicked_cancel || esc || clicked_cont && ready {
            self.font_list = None;
        }
        if clicked_cancel || esc {
            self.tool = Tool::Idle;
        } else if clicked_cont && ready {
            if let Tool::Text { at, .. } = &mut self.tool {
                *at = Some(DVec2::ZERO);
            }
            self.text_drag = None;
        }
    }

    /// The installed font families beside the Add Text dialog: a search field and the list; a click picks one.
    fn font_picker(&mut self, ui: &mut Ui, at: Pos2) {
        let Some(mut query) = self.font_list.clone() else { return };
        let rect = Rect::from_min_size(at, vec2(240.0, 320.0));
        let p = ui.painter().clone();
        p.rect_filled(rect.expand(1.0), radius(10), theme::c(0x48474a));
        p.rect_filled(rect, radius(9), theme::c(0x232224));
        let field = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(rect.width() - 20.0, 24.0));
        p.rect_filled(field, radius(6), theme::c(0x3a393c));
        let id = ui.scope_id().with("font-search");
        let resp = ui.put(
            field.shrink2(vec2(8.0, 3.0)),
            egui::TextEdit::singleline(&mut query).id(id).hint_text("Search fonts").frame(egui::Frame::NONE).font(theme::body(13.0)),
        );
        if resp.changed() {
            self.font_list = Some(query.clone());
        }
        let q = query.to_lowercase();
        let names: Vec<&String> = shapecraft_doc::text::families().iter().filter(|n| n.to_lowercase().contains(&q)).collect();
        let list = Rect::from_min_max(pos2(rect.left() + 4.0, field.bottom() + 6.0), rect.max - vec2(4.0, 6.0));
        let mut picked = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            egui::ScrollArea::vertical().id_salt("font-list").auto_shrink([false, false]).show_rows(ui, 24.0, names.len(), |ui, range| {
                for name in &names[range] {
                    let (r, resp) = ui.allocate_exact_size(vec2(list.width() - 12.0, 24.0), Sense::click());
                    let chosen = self.text_font.as_deref() == Some(name.as_str());
                    if resp.hovered() || chosen {
                        ui.painter().rect_filled(r, radius(5), if chosen { Tokens::accent() } else { theme::c(0x3a393c) });
                    }
                    ui.painter().text(
                        r.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        name,
                        theme::body(13.0),
                        Tokens::text_strong(),
                    );
                    if resp.clicked() {
                        picked = Some((*name).clone());
                    }
                }
            });
        });
        if names.is_empty() {
            p.text(list.center_top() + vec2(0.0, 20.0), Align2::CENTER_CENTER, "No fonts found", theme::body(12.0), Tokens::text_dim());
        }
        if let Some(name) = picked {
            self.text_font = Some(name);
            self.font_list = None;
        }
    }

    /// The first picked corner's arc: its corner, the bisector into the arc's middle (unit, plane coordinates), and
    /// how far the middle sits from the corner per unit of radius.
    fn fillet_frame(&self) -> Option<(DVec2, DVec2, f64)> {
        let (Tool::SketchFillet { corners, .. }, Some(sk)) = (&self.tool, self.session.active_sketch()) else { return None };
        let p = *corners.first()?;
        let far: Vec<DVec2> = sk
            .entities
            .iter()
            .filter_map(|e| match *e {
                SketchEntity::Line { a, b } if a.distance(p) < 1e-7 => Some(b),
                SketchEntity::Line { a, b } if b.distance(p) < 1e-7 => Some(a),
                _ => None,
            })
            .collect();
        let [fa, fb] = far[..] else { return None };
        let (u, v) = ((fa - p).try_normalize()?, (fb - p).try_normalize()?);
        let half = u.dot(v).clamp(-1.0, 1.0).acos() / 2.0;
        let bis = (u + v).try_normalize()?;
        (half.sin() > 1e-6).then(|| (p, bis, 1.0 / half.sin() - 1.0))
    }

    /// Sketch Fillet's ↖↘ handle on the first corner's arc with "R …" on a leader; dragging it along the bisector
    /// sets the radius. Cancel / Done at the bottom.
    pub(super) fn sketch_fillet_handle(&mut self, ui: &mut Ui) {
        let Tool::SketchFillet { radius, corners, .. } = &self.tool else { return };
        let (radius, any) = (*radius, !corners.is_empty());
        if let (Some((p, bis, k)), Some(pl)) = (self.fillet_frame(), self.sketch_plane()) {
            let r = self.numeric.as_deref().and_then(crate::workspace::tools::parse_length).map(f64::abs).unwrap_or(radius);
            let mid = p + bis * r * k;
            if let (Some(sm), Some(sc)) = (self.to_screen(pl.to_world(mid)), self.to_screen(pl.to_world(mid + bis * r))) {
                let painter = ui.painter().clone();
                let d = (sm - sc).normalized();
                let n = vec2(-d.y, d.x);
                let hot = self.fillet_drag || self.cursor.is_some_and(|c| c.distance(sm) < 14.0);
                let col = if hot { Tokens::accent() } else { Color32::WHITE };
                for s in [-1.0f32, 1.0] {
                    let tip = sm + d * 10.0 * s;
                    painter.line_segment([sm, tip], Stroke::new(2.5, col));
                    painter.add(egui::Shape::convex_polygon(vec![tip + d * 3.0 * s, tip + n * 4.0, tip - n * 4.0], col, Stroke::NONE));
                }
                let text = format!("R {}", format_length(r, self.session.doc.units));
                leader_dim(&painter, sm + n * 40.0, sm + n * 14.0, &text, self.numeric.as_deref());
                let _ = n;
                let (pressed, down, released) =
                    ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released()));
                if pressed && self.cursor.is_some_and(|c| c.distance(sm) < 14.0) {
                    self.fillet_drag = true;
                }
                if self.fillet_drag
                    && down
                    && let Some(c) = self.cursor
                    && let Some(hit) = self.sketch_cursor_raw(c)
                {
                    let along = (pl.to_local(hit) - p).dot(bis);
                    if let Tool::SketchFillet { radius, .. } = &mut self.tool {
                        *radius = (along / k).max(1e-3);
                    }
                }
                if released {
                    self.fillet_drag = false;
                }
            }
        }
        match self.bottom_actions(ui, any) {
            Some("Cancel") => self.tool = Tool::Idle,
            Some(_) => self.commit_tool(),
            None => {}
        }
    }

    /// "Cancel Esc" (and "Done ↩" when `done`) at the bottom middle of the view, as tools that apply in place show
    /// them in 26.200 (Text, Offset Edge). Returns the label clicked.
    pub(in crate::workspace) fn bottom_actions(&mut self, ui: &mut Ui, done: bool) -> Option<&'static str> {
        self.confirm = Some(done || self.confirm == Some(true));
        // A click on the toolbar's ✓ / ✕ (Preferences ▸ Done / Cancel Buttons) lands here, for the same handling.
        let cell = self.confirm_click.take().filter(|c| *c == "Cancel" || done);
        if self.settings.confirm_buttons == "Toolbar" {
            return cell;
        }
        let p = ui.painter().clone();
        let vp = self.viewport_rect;
        let y = vp.bottom() - 26.0;
        let items: &[(&'static str, &str, f32)] =
            if done { &[("Cancel", "Esc", 80.0), ("Done", "↩", 68.0)] } else { &[("Cancel", "Esc", 80.0)] };
        self.seen_bottom = items.iter().map(|i| i.0.to_string()).collect();
        let total: f32 = items.iter().map(|i| i.2).sum::<f32>() + 8.0 * (items.len() as f32 - 1.0);
        let mut x = vp.center().x - total / 2.0;
        let mut hit = None;
        for &(label, key, w) in items {
            let r = Rect::from_min_size(pos2(x, y - 12.0), vec2(w, 24.0));
            let over = self.cursor.is_some_and(|c| r.contains(c));
            p.rect_filled(r, radius(6), if over { Tokens::chip_hover() } else { Tokens::chip() });
            p.text(pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, label, theme::body(13.0), Tokens::text_strong());
            let cap = Rect::from_min_size(pos2(r.right() - 24.0, r.top() + 4.0), vec2(18.0, 16.0));
            p.rect_filled(cap, radius(3), theme::c(0x48474a));
            if key == "↩" {
                // Drawn: the font may not have the return glyph.
                let c = cap.center();
                let st = Stroke::new(1.0, Tokens::text_dim());
                p.line_segment([c + vec2(4.0, -4.0), c + vec2(4.0, 1.5)], st);
                p.line_segment([c + vec2(4.0, 1.5), c + vec2(-4.0, 1.5)], st);
                p.add(egui::Shape::convex_polygon(
                    vec![c + vec2(-5.0, 1.5), c + vec2(-2.0, -1.0), c + vec2(-2.0, 4.0)],
                    Tokens::text_dim(),
                    Stroke::NONE,
                ));
            } else {
                p.text(cap.center(), Align2::CENTER_CENTER, key, theme::body(9.0), Tokens::text_dim());
            }
            if ui.interact(r, ui.scope_id().with(("bottom-action", label)), Sense::click()).clicked() {
                hit = Some(label);
            }
            x += w + 8.0;
        }
        hit.or(cell)
    }

    /// Screen points of the text handles: (origin, up-arrow tip, right-arrow tip, free-move square, turn handle).
    pub(super) fn text_handle_points(&self) -> Option<[Pos2; 5]> {
        let (Tool::Text { at: Some(at), angle, .. }, Some(pl)) = (&self.tool, self.sketch_plane()) else { return None };
        let o = self.to_screen(pl.to_world(*at))?;
        let ux = self.to_screen(pl.to_world(*at + DVec2::from_angle(*angle)))? - o;
        let uy = self.to_screen(pl.to_world(*at + DVec2::from_angle(*angle + std::f64::consts::FRAC_PI_2)))? - o;
        let (ux, uy) = (ux.normalized(), uy.normalized());
        Some([o, o + uy * 52.0, o + ux * 52.0, o + (ux + uy) * 30.0, o + (ux + uy) * 58.0])
    }

    fn text_handles(&mut self, ui: &mut Ui) {
        let Some([o, up, right, square, turn]) = self.text_handle_points() else { return };
        let p = ui.painter().clone();
        let white = Color32::WHITE;
        let hover = self.cursor.and_then(|c| {
            [(1usize, up), (2, right), (3, square), (4, turn)].into_iter().find(|(_, q)| q.distance(c) < 12.0).map(|(k, _)| k)
        });
        let hot = |k: usize| self.text_drag.map(|d| d.0) == Some(k) || hover == Some(k);
        p.circle_stroke(o, 4.0, Stroke::new(1.5, white));
        for (k, tip) in [(1, up), (2, right)] {
            let col = if hot(k) { Tokens::accent() } else { white };
            let u = (tip - o).normalized();
            let pp = vec2(-u.y, u.x);
            p.line_segment([o + u * 8.0, tip - u * 8.0], Stroke::new(3.0, col));
            p.add(egui::Shape::convex_polygon(vec![tip, tip - u * 11.0 + pp * 6.0, tip - u * 11.0 - pp * 6.0], col, Stroke::NONE));
        }
        p.rect_filled(Rect::from_center_size(square, vec2(14.0, 14.0)), radius(3), if hot(3) { Tokens::accent() } else { white });
        // The turn handle: a small double-headed diagonal arrow.
        let d = (turn - o).normalized();
        let n = vec2(-d.y, d.x);
        let col = if hot(4) { Tokens::accent() } else { white };
        for s in [-1.0f32, 1.0] {
            let tip = turn + n * 8.0 * s;
            p.line_segment([turn, tip], Stroke::new(2.0, col));
            p.add(egui::Shape::convex_polygon(vec![tip + n * 3.0 * s, tip - d * 4.0, tip + d * 4.0], col, Stroke::NONE));
        }
        if let Some((k, start_at)) = self.text_drag.map(|d| (d.0, d.1))
            && let (Tool::Text { at: Some(at), .. }, true) = (&self.tool, k == 1 || k == 2)
        {
            let moved = format_length(at.distance(start_at), self.session.doc.units);
            axis_pill(
                &p,
                if k == 1 { up } else { right } + (if k == 1 { up } else { right } - o).normalized() * 26.0,
                (right - o).normalized(),
                &moved,
                true,
            );
        }
        // Dragging: arrows move along their axis, the square anywhere, the turn handle turns about the origin.
        let (pressed, down, released) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released()));
        if pressed
            && let Some(k) = hover
            && let Tool::Text { at: Some(at), angle, .. } = &self.tool
        {
            self.text_drag = Some((k, *at, *angle));
        }
        if let (Some((k, at0, angle0)), true, Some(c)) = (self.text_drag, down, self.cursor)
            && let (Some(pl), Some(hit)) = (self.sketch_plane(), self.sketch_cursor_raw(c))
        {
            let local = pl.to_local(hit);
            if let Tool::Text { at: Some(at), angle, .. } = &mut self.tool {
                match k {
                    1 | 2 => {
                        let axis = DVec2::from_angle(angle0 + if k == 1 { std::f64::consts::FRAC_PI_2 } else { 0.0 });
                        *at = at0 + axis * (local - at0).dot(axis);
                    }
                    3 => *at = local,
                    _ => *angle = angle0 + ((local - at0).to_angle() - std::f64::consts::FRAC_PI_4),
                }
            }
        }
        if released {
            self.text_drag = None;
        }
        match self.bottom_actions(ui, true) {
            Some("Cancel") => self.tool = Tool::Idle,
            Some(_) => self.commit_tool(),
            None => {}
        }
    }
}
