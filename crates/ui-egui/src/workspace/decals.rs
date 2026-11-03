//! Decals in Visualization, as observed (s13-004..016): the library's cards, a picture dragged from its card onto a
//! body, a click on a placed decal picking it (its centre handle drags it over the surface), and its Properties.

use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::{DECAL_SCALE, DecalImage, ItemId, Pick, Projection};

use super::Workspace;
use super::visual::{Slider, checkbox, done_button, slider};
use crate::theme::{self, Tokens, radius};

/// The picture's thumbnail as an egui texture, made once per picture.
pub(super) fn thumb(ws: &mut Workspace, ctx: &egui::Context, img: &DecalImage) -> Option<egui::TextureId> {
    let stale = ws.decal_thumbs.get(&img.id.0).is_none_or(|(px, _)| !std::sync::Arc::ptr_eq(px, &img.pixels));
    if stale {
        let (w, h, ref px) = *img.pixels;
        if w == 0 || h == 0 || px.len() != (w * h * 4) as usize {
            return None;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], px);
        let tex = ctx.load_texture(format!("decal-{}", img.id.0), image, egui::TextureOptions::LINEAR);
        ws.decal_thumbs.insert(img.id.0, (img.pixels.clone(), tex));
    }
    ws.decal_thumbs.get(&img.id.0).map(|(_, t)| t.id())
}

/// Draws a picture fitted into `rect`, keeping its shape.
fn paint_thumb(p: &egui::Painter, rect: Rect, tex: egui::TextureId, aspect: f64, tint: Color32) {
    let a = aspect.clamp(0.05, 20.0) as f32;
    let size = if a >= 1.0 { vec2(rect.width(), rect.width() / a) } else { vec2(rect.height() * a, rect.height()) };
    let r = Rect::from_center_size(rect.center(), size.min(rect.size()));
    p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
}

/// The Decal section of the Material tab: its pictures as cards (two columns), each dragged onto a body to place it.
/// Right-click deletes one. With none yet, the import tile and Import… show (the reference's sample pictures are its
/// own, so they aren't copied).
pub(super) fn library(ws: &mut Workspace, ui: &mut Ui, w: f32) -> bool {
    cards(ws, ui, w, false)
}

/// The Custom section's materials (s13-034): each picture a "Standard / <name>" card; a click puts it on the
/// selected bodies (every body when none is), a right-click renames or deletes it.
pub(super) fn custom_materials(ws: &mut Workspace, ui: &mut Ui, w: f32) -> bool {
    cards(ws, ui, w, true)
}

fn cards(ws: &mut Workspace, ui: &mut Ui, w: f32, material: bool) -> bool {
    let images: Vec<DecalImage> = ws.session.doc.decal_images.iter().filter(|i| i.material == material && !i.sheet).cloned().collect();
    if images.is_empty() {
        return false;
    }
    let cw = (w - 14.0) / 2.0;
    for pair in images.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for img in pair {
                let tall = if material { 30.0 } else { 18.0 };
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(cw, cw + tall), if material { Sense::click() } else { Sense::click_and_drag() });
                let p = ui.painter().clone();
                p.rect_filled(rect, radius(6), if resp.hovered() { theme::c(0x48484b) } else { theme::c(0x3a3a3c) });
                if material {
                    // The picture as a round swatch, like the library's balls.
                    let c = pos2(rect.center().x, rect.top() + cw * 0.5);
                    if let Some(tex) = thumb(ws, ui.ctx(), img) {
                        let r = cw * 0.42;
                        let mut mesh = egui::Mesh::with_texture(tex);
                        let n = 48;
                        mesh.vertices.push(egui::epaint::Vertex { pos: c, uv: pos2(0.5, 0.5), color: Color32::WHITE });
                        for k in 0..=n {
                            let a = k as f32 / n as f32 * std::f32::consts::TAU;
                            let d = vec2(a.cos(), a.sin());
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: c + d * r,
                                uv: pos2(0.5 + d.x * 0.5, 0.5 + d.y * 0.5),
                                color: Color32::WHITE,
                            });
                            if k > 0 {
                                mesh.add_triangle(0, k as u32, k as u32 + 1);
                            }
                        }
                        p.add(mesh);
                        // A soft shade so it reads as a ball.
                        p.circle_stroke(c, r - 2.0, egui::Stroke::new(4.0, Color32::from_black_alpha(50)));
                    }
                    p.text(
                        pos2(rect.left() + 8.0, rect.bottom() - 22.0),
                        Align2::LEFT_CENTER,
                        "Standard",
                        theme::body(9.5),
                        Tokens::text_dim(),
                    );
                } else {
                    let art = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(cw - 20.0, cw - 20.0));
                    if let Some(tex) = thumb(ws, ui.ctx(), img) {
                        paint_thumb(&p, art, tex, img.aspect(), Color32::WHITE);
                    }
                }
                let name_y = rect.bottom() - if material { 9.0 } else { 12.0 };
                if ws.picture_rename.as_ref().is_some_and(|(id, _)| *id == img.id) {
                    let mut done = false;
                    if let Some((_, text)) = ws.picture_rename.as_mut() {
                        let field = Rect::from_min_max(pos2(rect.left() + 4.0, name_y - 9.0), pos2(rect.right() - 4.0, name_y + 9.0));
                        let r = ui.put(field, egui::TextEdit::singleline(text).font(theme::strong(11.0)));
                        r.request_focus();
                        done = r.lost_focus();
                    }
                    if done && let Some((id, text)) = ws.picture_rename.take() {
                        let _ = ws.run("rename_decal_image", json!({ "image": id.0, "name": text }));
                    }
                } else {
                    p.text(
                        pos2(rect.left() + 8.0, name_y),
                        Align2::LEFT_CENTER,
                        &img.name,
                        theme::strong(if material { 11.5 } else { 11.0 }),
                        Tokens::text_strong(),
                    );
                }
                if !material && resp.drag_started() {
                    ws.decal_drag = Some(img.id);
                }
                if material && resp.clicked() {
                    let ids: Vec<u64> = super::visual::targets(ws).iter().map(|i| i.0).collect();
                    let _ = ws.run("set_appearance", json!({ "items": ids, "custom": img.id.0 }));
                    ws.vis_changing = false;
                }
                resp.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        ws.picture_rename = Some((img.id, img.name.clone()));
                        ui.close();
                    }
                    if ui.button("Delete").clicked() {
                        let _ = ws.run("delete_decal_image", json!({ "image": img.id.0 }));
                        ui.close();
                    }
                });
            }
        });
        ui.add_space(6.0);
    }
    true
}

/// While a picture is dragged from its card: it follows the pointer on a blue square (s13-008); let go over a body,
/// it is placed there, centred on that point and picked.
pub(super) fn drag_frame(ws: &mut Workspace, ui: &Ui) {
    let Some(image) = ws.decal_drag else { return };
    let (pos, down) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.primary_down()));
    if down {
        if let Some(p) = pos {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("decal-drag")));
            let sq = Rect::from_center_size(p, vec2(64.0, 64.0));
            painter.rect_filled(sq, radius(4), Color32::from_rgba_unmultiplied(70, 100, 230, 110));
            if let Some(img) = ws.session.doc.decal_images.iter().find(|i| i.id == image).cloned()
                && let Some(tex) = thumb(ws, ui.ctx(), &img)
            {
                paint_thumb(&painter, sq.shrink(8.0), tex, img.aspect(), Color32::WHITE);
            }
        }
        return;
    }
    ws.decal_drag = None;
    let Some(p) = pos.filter(|p| ws.view_rect.contains(*p)) else { return };
    if let Some((Pick::Face { body, face }, at)) = ws.pick_at(p)
        && let Some(out) = ws.run("place_decal", json!({ "image": image.0, "body": body.0, "face": face, "point": [at.x, at.y, at.z] }))
    {
        ws.decal_sel = out["decal"].as_u64().map(ItemId);
        ws.session.selection.clear();
    }
}

impl Workspace {
    /// The decal under the pointer: the last placed one whose picture isn't see-through there.
    pub(super) fn decal_at(&self, c: Pos2) -> Option<ItemId> {
        let Some((Pick::Face { body, face }, at)) = self.pick_at(c) else { return None };
        let doc = &self.session.doc;
        doc.decals.iter().rev().filter(|d| d.body == body).find_map(|d| {
            let img = doc.decal_images.iter().find(|i| i.id == d.image)?;
            let (u, v) = d.uv_at(at, face, img.aspect())?;
            let (w, h, ref px) = *img.pixels;
            let (x, y) = (((u * w as f64) as u32).min(w.saturating_sub(1)), ((v * h as f64) as u32).min(h.saturating_sub(1)));
            let alpha = px.get(((y * w + x) * 4 + 3) as usize).copied().unwrap_or(0);
            (alpha > 16).then_some(d.id)
        })
    }

    /// The picked decal's centre on screen.
    fn decal_handle_pos(&self) -> Option<Pos2> {
        let d = self.session.doc.decals.iter().find(|d| Some(d.id) == self.decal_sel)?;
        let (s, _) = self.camera.project(d.center, self.size())?;
        Some(self.view_rect.min + vec2(s.x as f32, s.y as f32))
    }

    /// The picked decal's handle (a white ring at its centre): dragging it slides the decal over whatever body face
    /// is under the pointer, as one undo step. Returns whether the drag was the handle's.
    pub(super) fn decal_drags(&mut self, ui: &Ui, resp: &egui::Response) -> bool {
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = ui.input(|i| i.pointer.press_origin());
            self.decal_handle = matches!((origin, self.decal_handle_pos()), (Some(o), Some(h)) if o.distance(h) < 14.0);
            self.live_step = None;
        }
        if !self.decal_handle {
            return false;
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            self.decal_handle = false;
            self.live_step = None;
            return true;
        }
        if let (Some(id), Some(p)) = (self.decal_sel, ui.input(|i| i.pointer.interact_pos()))
            && let Some((Pick::Face { body, face }, at)) = self.pick_at(p)
        {
            self.live_run("update_decal", json!({ "decal": id.0, "body": body.0, "face": face, "point": [at.x, at.y, at.z] }));
        }
        true
    }

    /// Runs a Visualization change; while a slider or the handle is being dragged, each change replaces the last, so
    /// one undo takes the whole drag back. A change back to where it started is quietly ignored.
    pub(super) fn live_run(&mut self, id: &'static str, params: serde_json::Value) {
        if self.is_live() {
            if self.session.amend(id, &params).is_ok() {
                self.live_step = Some(self.session.revision);
            }
        } else if self.run(id, params).is_some() {
            self.live_step = Some(self.session.revision);
        }
    }

    /// The white ring at the picked decal's centre (s13-010), drawn over the view.
    pub(super) fn paint_decal_handle(&self, ui: &Ui) {
        let Some(h) = self.decal_handle_pos() else { return };
        if !self.view_rect.contains(h) {
            return;
        }
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, ui.scope_id().with("decal-handle")));
        p.circle_filled(h, 8.0, Color32::from_black_alpha(60));
        p.circle_filled(h, 6.5, if self.decal_handle { theme::c(0x3cd0f2) } else { Color32::WHITE });
        p.circle_filled(h, 3.5, theme::c(0x1c1c1e));
    }
}

/// The picked decal's Properties (s13-010): its card, Scale, Rotate and Opacity, the projection type and whether it
/// runs onto the neighbouring faces, then Done.
pub(super) fn properties(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    let Some(id) = ws.decal_sel else { return };
    let Some(d) = ws.session.doc.decals.iter().find(|d| d.id == id).cloned() else {
        ws.decal_sel = None;
        return;
    };
    let img = ws.session.doc.decal_images.iter().find(|i| i.id == d.image).cloned();
    let p = ui.painter().clone();
    p.text(pos2(r.center().x, r.top() + 18.0), Align2::CENTER_CENTER, "Properties", theme::body(12.0), Tokens::text_dim());
    let left = r.left() + 14.0;
    let w = r.width() - 28.0;
    let mut y = r.top() + 46.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Decal", theme::strong(12.5), Tokens::text_strong());
    y += 12.0;
    let card = Rect::from_min_size(pos2(left - 2.0, y), vec2(w + 4.0, 54.0));
    p.rect_filled(card, radius(7), theme::c(0x303032));
    if let Some(img) = &img {
        let tile = Rect::from_center_size(pos2(card.left() + 27.0, card.center().y), vec2(36.0, 36.0));
        p.rect_filled(tile, radius(5), theme::c(0x3a3a3c));
        if let Some(tex) = thumb(ws, ui.ctx(), img) {
            paint_thumb(&p, tile.shrink(3.0), tex, img.aspect(), Color32::WHITE);
        }
        p.text(pos2(card.left() + 58.0, card.center().y), Align2::LEFT_CENTER, &img.name, theme::strong(12.0), Tokens::text_strong());
    }
    y = card.bottom() + 26.0;
    let rows = [
        ("Scale", d.scale, DECAL_SCALE, false, "scale"),
        ("Rotate", d.rotate, (-180.0, 180.0), false, "rotate"),
        ("Opacity", d.opacity as f64 * 100.0, (0.0, 100.0), false, "opacity"),
    ];
    for (label, value, range, log, key) in rows {
        let fmt = match key {
            "scale" => format!("{value:.1}x"),
            "rotate" => format!("{value:.0}°"),
            _ => format!("{value:.0}%"),
        };
        let out = slider(ui, Slider { left, top: y, width: w, label, value, range, log, text: &fmt, id: ("decal", key) });
        if out.started {
            ws.live_step = None;
        }
        if let Some(v) = out.value {
            let v = if key == "opacity" { v / 100.0 } else { v };
            ws.live_run("update_decal", json!({ "decal": id.0, key: v }));
        }
        if out.stopped {
            ws.live_step = None;
        }
        y += 50.0;
    }
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Projection", theme::strong(12.5), Tokens::text_strong());
    y += 22.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Type", theme::body(12.0), Tokens::text());
    let label = if d.projection == Projection::Cylindrical { "Cylindrical" } else { "Planar" };
    let pill = Rect::from_min_size(pos2(left + w - 62.0, y - 9.0), vec2(62.0, 18.0));
    if super::visual::pill_menu(ui, pill, label, &["Planar", "Cylindrical"], ("decal-projection", id.0)).is_some_and(|k| k != label) {
        let _ = ws.run("update_decal", json!({ "decal": id.0, "projection": if label == "Planar" { "cylindrical" } else { "planar" } }));
    }
    y += 23.0;
    if checkbox(ui, pos2(left, y), "Extend to adjacent faces", d.extend, ("decal-extend", id.0)) {
        let _ = ws.run("update_decal", json!({ "decal": id.0, "extend": !d.extend }));
    }
    if done_button(ui, r, ("decal-done", id.0)) {
        ws.decal_sel = None;
    }
}

/// A body's decals in its Properties (s13-029): thumbnail, name and a blue Edit that picks the decal. Returns the
/// height used.
pub(super) fn body_rows(ws: &mut Workspace, ui: &mut Ui, r: Rect, top: f32, body: ItemId) -> f32 {
    let decals: Vec<(ItemId, ItemId)> = ws.session.doc.decals.iter().filter(|d| d.body == body).map(|d| (d.id, d.image)).collect();
    if decals.is_empty() {
        return 0.0;
    }
    let p = ui.painter().clone();
    let left = r.left() + 14.0;
    let mut y = top;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Decal", theme::strong(12.5), Tokens::text_strong());
    y += 12.0;
    for (id, image) in decals.into_iter().take(4) {
        let card = Rect::from_min_size(pos2(left - 2.0, y), vec2(r.width() - 24.0, 54.0));
        p.rect_filled(card, radius(7), theme::c(0x303032));
        if let Some(img) = ws.session.doc.decal_images.iter().find(|i| i.id == image).cloned() {
            let tile = Rect::from_center_size(pos2(card.left() + 27.0, card.center().y), vec2(36.0, 36.0));
            p.rect_filled(tile, radius(5), theme::c(0x3a3a3c));
            if let Some(tex) = thumb(ws, ui.ctx(), &img) {
                paint_thumb(&p, tile.shrink(3.0), tex, img.aspect(), Color32::WHITE);
            }
            p.text(pos2(card.left() + 58.0, card.center().y), Align2::LEFT_CENTER, &img.name, theme::strong(12.0), Tokens::text_strong());
        }
        let edit = Rect::from_center_size(pos2(card.right() - 24.0, card.center().y), vec2(40.0, 22.0));
        let er = ui.interact(edit, ui.scope_id().with(("decal-edit", id.0)), Sense::click());
        p.text(
            edit.center(),
            Align2::CENTER_CENTER,
            "Edit",
            theme::body(12.0),
            if er.hovered() { theme::c(0x6fa0ff) } else { theme::c(0x3f86ff) },
        );
        if er.clicked() {
            ws.decal_sel = Some(id);
            ws.session.selection.clear();
        }
        y = card.bottom() + 6.0;
    }
    y + 16.0 - top
}
