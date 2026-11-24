//! View ▸ Calibrate Actual Size (#62): a bank card drawn on the screen, resized until it matches a real one held
//! against the glass. Screens don't reliably say how big they are, and a person with a card can.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use super::camera::{CARD_MM, clamp_points_per_mm};
use crate::theme::{self, Tokens, radius};

/// Draws the card while it's open. Done keeps the size (and zooms to actual size when that's what opened it).
pub(crate) fn card(ws: &mut Workspace, ui: &mut Ui) {
    let Some((mut per_mm, then_apply)) = ws.calibrating else { return };
    let full = ui.ctx().content_rect();
    let (cw, ch) = ((CARD_MM.0 * per_mm) as f32, (CARD_MM.1 * per_mm) as f32);
    let w = (cw + 120.0).max(460.0).min(full.width() - 24.0);
    let h = (ch + 170.0).min(full.height() - 24.0);
    let r = Rect::from_center_size(full.center(), vec2(w, h));
    let (mut done, mut cancel) = (false, ui.input(|i| i.key_pressed(egui::Key::Escape)));
    let id = ui.scope_id().with("calibrate-actual-size");
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(full.min).show(ui.ctx(), |ui| {
        let _ = ui.allocate_rect(full, Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(r.translate(vec2(0.0, 6.0)), radius(12), Color32::from_black_alpha(90));
        p.rect_filled(r, radius(12), Tokens::panel());
        p.rect_stroke(r, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
        let _ = ui.interact(r, id.with("card"), Sense::click());
        p.text(
            pos2(r.left() + 20.0, r.top() + 26.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("Calibrate Actual Size"),
            theme::strong(15.0),
            Tokens::text_strong(),
        );
        let mut job = egui::text::LayoutJob::simple(
            "Hold a bank card against the screen and drag the blue card's right edge (or use − and +) until both are the same size.".into(),
            theme::body(12.5),
            Tokens::text_dim(),
            r.width() - 40.0,
        );
        job.halign = egui::Align::LEFT;
        let g = ui.fonts_mut(|f| f.layout_job(job));
        let text_h = g.size().y;
        p.galley(pos2(r.left() + 20.0, r.top() + 46.0), g, Tokens::text_dim());

        // The card, its left edge fixed so the right one follows the pointer.
        let left = r.left() + 20.0;
        let top = r.top() + 62.0 + text_h;
        let shown = Rect::from_min_size(pos2(left, top), vec2(cw, ch));
        let corner = (3.18 * per_mm).clamp(2.0, 30.0) as u8;
        p.rect_filled(shown, radius(corner), Tokens::accent().gamma_multiply(0.35));
        p.rect_stroke(shown, radius(corner), Stroke::new(1.5, Tokens::accent()), StrokeKind::Inside);
        // Centimetre ticks along the top edge, for a ruler instead of a card.
        for mm in 0..=(CARD_MM.0 as i32) {
            let x = left + (mm as f64 * per_mm) as f32;
            let len = if mm % 10 == 0 {
                10.0
            } else if mm % 5 == 0 {
                6.0
            } else {
                3.0
            };
            p.line_segment([pos2(x, top), pos2(x, top + len)], Stroke::new(1.0, Tokens::text()));
            if mm % 10 == 0 && mm > 0 {
                p.text(pos2(x, top + 12.0), Align2::CENTER_TOP, format!("{}", mm / 10), theme::body(10.0), Tokens::text());
            }
        }
        p.text(shown.center() + vec2(0.0, 8.0), Align2::CENTER_CENTER, "85.6 × 54 mm", theme::body(12.0), Tokens::text_strong());
        let edge = Rect::from_min_max(pos2(shown.right() - 14.0, shown.top()), pos2(shown.right() + 14.0, shown.bottom()));
        let drag = ui.interact(edge.union(shown), id.with("drag"), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if drag.dragged() {
            per_mm += drag.drag_delta().x as f64 / CARD_MM.0;
        }

        // − 1 mm = … points + under the card, then Cancel / Done.
        let row = r.bottom() - 30.0;
        let small = |x: f32, label: &str| {
            let b = Rect::from_center_size(pos2(x, row), vec2(30.0, 28.0));
            let resp = ui.interact(b, id.with(label), Sense::click());
            p.rect_filled(b, radius(7), if resp.hovered() { Tokens::button_hover() } else { Tokens::button() });
            p.text(b.center(), Align2::CENTER_CENTER, label, theme::strong(15.0), Tokens::text_strong());
            resp.clicked()
        };
        if small(r.left() + 35.0, "−") {
            per_mm *= 0.995;
        }
        if small(r.left() + 71.0, "+") {
            per_mm *= 1.005;
        }
        let note = format!("1 mm = {per_mm:.2} points");
        p.text(pos2(r.left() + 96.0, row), Align2::LEFT_CENTER, note, theme::body(12.0), Tokens::text_dim());
        let button = |right: f32, label: &str, primary: bool| {
            let g = p.layout_no_wrap(label.to_string(), theme::strong(13.0), Color32::WHITE);
            let b = Rect::from_min_max(pos2(right - g.size().x - 28.0, row - 15.0), pos2(right, row + 15.0));
            let resp = ui.interact(b, id.with(("button", label)), Sense::click());
            let fill = if primary { Tokens::accent() } else { Tokens::button() };
            p.rect_filled(b, radius(8), if resp.hovered() { fill.gamma_multiply(1.15) } else { fill });
            p.text(
                b.center(),
                Align2::CENTER_CENTER,
                label,
                theme::strong(13.0),
                if primary { Color32::WHITE } else { Tokens::text_strong() },
            );
            (resp.clicked(), b.left() - 10.0)
        };
        let (ok, x) = button(r.right() - 20.0, "Done", true);
        done |= ok;
        cancel |= button(x, "Cancel", false).0;
    });
    let per_mm = clamp_points_per_mm(per_mm);
    ws.calibrating = if done || cancel { None } else { Some((per_mm, then_apply)) };
    if done {
        ws.settings.screen_points_per_mm = per_mm;
        if then_apply {
            ws.actual_size();
        }
    }
}
