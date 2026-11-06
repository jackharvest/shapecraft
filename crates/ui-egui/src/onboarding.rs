//! The first-run question, as 26.200 asks it (observed at a fresh install, session 11): which device the model is
//! worked with. The answer picks the navigation preset; Preferences changes it later.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::theme::{self, Tokens, radius};

/// The choices: (shown name, stored value, the navigation preset it takes).
pub const DEVICES: [(&str, &str, &str); 2] =
    [("Mouse and Keyboard", "mouse", "Blender"), ("Trackpad and Keyboard", "trackpad", "ShapeCraft Default")];

/// Draws the question over everything; returns the device picked.
pub fn show(ui: &mut Ui) -> Option<usize> {
    let full = ui.ctx().content_rect();
    let mut picked = None;
    egui::Area::new(ui.scope_id().with("onboarding")).fixed_pos(full.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (_, _) = ui.allocate_exact_size(full.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, Color32::from_black_alpha(150));
        let card = Rect::from_center_size(full.center(), vec2(560.0, 330.0));
        p.rect_filled(card.translate(vec2(0.0, 6.0)).expand(2.0), radius(14), Color32::from_black_alpha(90));
        p.rect_filled(card, radius(12), theme::c(0x232324));
        p.rect_stroke(card, radius(12), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
        p.text(
            pos2(card.center().x, card.top() + 44.0),
            Align2::CENTER_CENTER,
            "Which device do you want to use for modeling?",
            theme::strong(17.0),
            Tokens::text_strong(),
        );
        p.text(
            pos2(card.center().x, card.top() + 70.0),
            Align2::CENTER_CENTER,
            "You can change it any time in Preferences.",
            theme::body(12.5),
            Tokens::text_dim(),
        );
        for (i, (name, ..)) in DEVICES.iter().enumerate() {
            let tile = Rect::from_min_size(pos2(card.left() + 40.0 + i as f32 * 250.0, card.top() + 100.0), vec2(230.0, 190.0));
            let resp = ui.interact(tile, ui.scope_id().with(("device", i)), Sense::click());
            p.rect_filled(tile, radius(10), if resp.hovered() { theme::c(0x34343a) } else { theme::c(0x2c2c2e) });
            p.rect_stroke(
                tile,
                radius(10),
                Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { Tokens::accent() } else { theme::c(0x3e3e42) }),
                StrokeKind::Inside,
            );
            let art = Rect::from_center_size(pos2(tile.center().x, tile.top() + 78.0), vec2(150.0, 100.0));
            device_art(&p, art, i == 0);
            p.text(pos2(tile.center().x, tile.bottom() - 26.0), Align2::CENTER_CENTER, *name, theme::strong(13.5), Tokens::text_strong());
            if resp.clicked() {
                picked = Some(i);
            }
        }
    });
    picked
}

/// Our own pictures: a keyboard with a mouse, or with a trackpad.
fn device_art(p: &egui::Painter, r: Rect, mouse: bool) {
    let ink = theme::c(0xbdbdc2);
    let key = Rect::from_min_size(pos2(r.left(), r.center().y - 22.0), vec2(96.0, 44.0));
    p.rect_stroke(key, radius(5), Stroke::new(1.6, ink), StrokeKind::Inside);
    for row in 0..3 {
        for col in 0..7 {
            let k = Rect::from_min_size(pos2(key.left() + 6.0 + col as f32 * 12.5, key.top() + 6.0 + row as f32 * 11.0), vec2(9.0, 7.0));
            p.rect_filled(k, radius(1), Color32::from_white_alpha(60));
        }
    }
    if mouse {
        let m = Rect::from_min_size(pos2(r.right() - 36.0, r.center().y - 26.0), vec2(30.0, 48.0));
        p.rect_stroke(m, radius(15), Stroke::new(1.6, ink), StrokeKind::Inside);
        p.line_segment([pos2(m.center().x, m.top() + 4.0), pos2(m.center().x, m.top() + 18.0)], Stroke::new(1.4, ink));
    } else {
        let t = Rect::from_min_size(pos2(r.right() - 46.0, r.center().y - 24.0), vec2(44.0, 48.0));
        p.rect_stroke(t, radius(6), Stroke::new(1.6, ink), StrokeKind::Inside);
        for dx in [-6.0, 6.0] {
            p.circle_filled(pos2(t.center().x + dx, t.center().y), 3.0, Color32::from_white_alpha(90));
        }
    }
}

/// A tablet's choices: (shown name, stored value). Each device's input is told apart by itself either way; this
/// picks which controls are shown, and is the walk-through.
pub const TABLET_DEVICES: [(&str, &str); 3] = [("Apple Pencil", "pencil"), ("Fingers", "touch"), ("Keyboard and Trackpad", "keyboard")];

/// What each tablet choice does, as (gesture, effect).
const TABLET_CONTROLS: [&[(&str, &str)]; 3] = [
    &[
        ("Pencil tap", "Pick and press. Hovering lights what a tap picks."),
        ("Pencil drag", "Draw, and drag handles and arrows."),
        ("One finger drag", "Turn the view, even while drawing."),
        ("Two fingers", "Pan, and pinch to zoom."),
        ("Two / three-finger tap", "Undo / Redo."),
        ("Finger hold", "More options (like a right-click)."),
    ],
    &[
        ("Tap", "Pick and press. Tap point to point to draw."),
        ("One finger drag", "Turn the view, or drag handles and arrows."),
        ("Two fingers", "Pan, and pinch to zoom."),
        ("Two / three-finger tap", "Undo / Redo."),
        ("Hold", "More options (like a right-click)."),
    ],
    &[
        ("Click", "Pick, draw, and drag handles."),
        ("Two-finger scroll", "Turn the view."),
        ("Pinch", "Zoom."),
        ("Secondary click", "More options."),
        ("Keyboard", "Every shortcut, as on the computer."),
    ],
];

/// The tablet's first-run question, then the controls of the device picked. Returns it once Start Modeling is pressed.
pub fn tablet(ui: &mut Ui, choice: &mut Option<usize>) -> Option<usize> {
    let full = ui.ctx().content_rect();
    let mut done = None;
    egui::Area::new(ui.scope_id().with("onboarding-tablet")).fixed_pos(full.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (_, _) = ui.allocate_exact_size(full.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, Color32::from_black_alpha(150));
        let rows = choice.map_or(0, |c| TABLET_CONTROLS[c].len());
        let card =
            Rect::from_center_size(full.center(), vec2(780.0, 330.0 + if choice.is_some() { rows as f32 * 30.0 + 70.0 } else { 0.0 }));
        p.rect_filled(card.translate(vec2(0.0, 6.0)).expand(2.0), radius(14), Color32::from_black_alpha(90));
        p.rect_filled(card, radius(12), theme::c(0x232324));
        p.rect_stroke(card, radius(12), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
        p.text(
            pos2(card.center().x, card.top() + 44.0),
            Align2::CENTER_CENTER,
            "How do you want to use ShapeCraft on this tablet?",
            theme::strong(19.0),
            Tokens::text_strong(),
        );
        p.text(
            pos2(card.center().x, card.top() + 72.0),
            Align2::CENTER_CENTER,
            "Use any of them any time: the pencil, fingers and a trackpad are told apart by themselves.",
            theme::body(13.0),
            Tokens::text_dim(),
        );
        for (i, (name, _)) in TABLET_DEVICES.iter().enumerate() {
            let tile = Rect::from_min_size(pos2(card.left() + 40.0 + i as f32 * 240.0, card.top() + 100.0), vec2(220.0, 190.0));
            let resp = ui.interact(tile, ui.scope_id().with(("tablet-device", i)), Sense::click());
            let on = *choice == Some(i);
            let lit = on || resp.hovered();
            p.rect_filled(
                tile,
                radius(10),
                if on {
                    theme::c(0x1f3550)
                } else if lit {
                    theme::c(0x34343a)
                } else {
                    theme::c(0x2c2c2e)
                },
            );
            p.rect_stroke(
                tile,
                radius(10),
                Stroke::new(if lit { 2.0 } else { 1.0 }, if lit { Tokens::accent() } else { theme::c(0x3e3e42) }),
                StrokeKind::Inside,
            );
            let art = Rect::from_center_size(pos2(tile.center().x, tile.top() + 78.0), vec2(150.0, 100.0));
            match i {
                0 => pencil_art(&p, art),
                1 => finger_art(&p, art),
                _ => device_art(&p, art, false),
            }
            p.text(pos2(tile.center().x, tile.bottom() - 26.0), Align2::CENTER_CENTER, *name, theme::strong(14.5), Tokens::text_strong());
            if resp.clicked() {
                *choice = Some(i);
            }
        }
        if let Some(c) = *choice {
            let mut y = card.top() + 322.0;
            for (gesture, what) in TABLET_CONTROLS[c] {
                p.text(pos2(card.left() + 60.0, y), Align2::LEFT_CENTER, *gesture, theme::strong(14.0), Tokens::text_strong());
                p.text(pos2(card.left() + 280.0, y), Align2::LEFT_CENTER, *what, theme::body(14.0), Tokens::text_dim());
                y += 30.0;
            }
            let go = Rect::from_min_size(pos2(card.left() + 40.0, card.bottom() - 68.0), vec2(card.width() - 80.0, 44.0));
            let resp = ui.interact(go, ui.scope_id().with("tablet-go"), Sense::click());
            p.rect_filled(go, radius(10), if resp.hovered() { theme::c(0x2f7cf6) } else { Tokens::accent() });
            p.text(go.center(), Align2::CENTER_CENTER, "Start Modeling", theme::strong(15.0), Color32::WHITE);
            if resp.clicked() {
                done = Some(c);
            }
        }
    });
    done
}

/// Our own picture of a pencil, slanted, tip at the bottom left.
fn pencil_art(p: &egui::Painter, r: Rect) {
    let ink = Stroke::new(1.6, theme::c(0xbdbdc2));
    let (tip, d) = (pos2(r.center().x - 36.0, r.center().y + 36.0), vec2(1.0, -1.0).normalized());
    let n = vec2(d.y, -d.x) * 7.0;
    let (cone, end) = (tip + d * 20.0, tip + d * 104.0);
    p.add(egui::Shape::closed_line(vec![tip, cone + n, end + n, end - n, cone - n], ink));
    p.line_segment([cone + n, cone - n], ink);
    p.circle_filled(tip + d * 5.0, 2.2, theme::c(0xbdbdc2));
}

/// Our own picture of a fingertip touching the glass.
fn finger_art(p: &egui::Painter, r: Rect) {
    let ink = Stroke::new(1.6, theme::c(0xbdbdc2));
    let c = r.center();
    let finger = Rect::from_min_max(pos2(c.x - 13.0, c.y - 44.0), pos2(c.x + 13.0, c.y + 22.0));
    p.rect_stroke(finger, egui::CornerRadius { nw: 13, ne: 13, sw: 4, se: 4 }, ink, StrokeKind::Inside);
    p.line_segment([pos2(c.x - 7.0, c.y - 34.0), pos2(c.x + 7.0, c.y - 34.0)], Stroke::new(1.2, Color32::from_white_alpha(80)));
    for k in 1..=2 {
        p.circle_stroke(pos2(c.x, c.y + 34.0), 8.0 * k as f32, Stroke::new(1.2, Color32::from_white_alpha(110 - 40 * k)));
    }
}
