//! Import Preferences: before a STEP, IGES or Parasolid file comes in, a
//! sheet asks what to optimize the import for. Quality cleans the geometry up, Speed takes it as it is (with a
//! warning), Custom picks the clean-ups one by one. "Don't show this again" keeps the choice and skips the sheet.

use std::path::PathBuf;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::Workspace;
use super::export::{segmented, switch};
use crate::theme::{self, Tokens, radius};

/// The sheet while it's open: the file waiting, and the choices (seeded from the settings).
pub struct ImportPrefs {
    pub path: PathBuf,
    /// 0 Quality, 1 Speed, 2 Custom.
    pub mode: usize,
    /// Custom's switches: simplify, repair, edges, sew.
    pub custom: [bool; 4],
    pub dont_show: bool,
}

pub const MODES: [&str; 3] = ["Quality", "Speed", "Custom"];

/// Custom's rows: title and what it does.
const ROWS: [(&str, &str); 4] = [
    ("Simplify Geometry", "Merge faces and edges that lie on one surface or curve. Fewer, larger faces; small details can change."),
    ("Healing", "Close small gaps, drop tiny edges and faces, and turn faces that point the wrong way."),
    ("Accurate Edge Computation", "Work each edge out again from the faces it joins, so they meet exactly."),
    ("Sewing", "Join faces that touch into closed bodies, so they can be modeled on afterwards."),
];

/// Files the sheet comes up for: the ones carrying exact geometry that may need repair.
pub fn wants_sheet(path: &std::path::Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "step" | "stp" | "iges" | "igs" | "x_t" | "x_b")
}

/// The import command's "heal" for a mode and Custom's switches.
pub fn heal_param(mode: usize, custom: [bool; 4]) -> Value {
    match mode {
        1 => json!("speed"),
        2 => json!({ "simplify": custom[0], "repair": custom[1], "edges": custom[2], "sew": custom[3] }),
        _ => json!("quality"),
    }
}

/// The settings' mode name back to its index (anything unknown is Quality).
pub fn mode_index(name: &str) -> usize {
    MODES.iter().position(|m| m.eq_ignore_ascii_case(name)).unwrap_or(0)
}

pub(super) fn dialog(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut d) = ws.import_prefs.take() else { return };
    // About 560 wide, 640 tall, centred.
    let r = Rect::from_center_size(full.center(), vec2(560.0, 640.0));
    let p = ui.painter().clone();
    p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
    p.rect_filled(r.translate(vec2(0.0, 6.0)).expand(2.0), radius(12), Color32::from_black_alpha(90));
    p.rect_filled(r, radius(10), theme::c(0x1e1e1e));
    p.rect_stroke(r, radius(10), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
    p.text(r.left_top() + vec2(29.0, 29.0), Align2::LEFT_CENTER, "Import Preferences", theme::strong(15.0), Tokens::text_strong());
    let x0 = r.left() + 18.0;
    let x1 = r.right() - 18.0;
    // "Optimize Import for" and the three segments, in a rounded box.
    let head = Rect::from_min_max(pos2(x0, r.top() + 56.0), pos2(x1, r.top() + 98.0));
    p.rect_filled(head, radius(7), theme::c(0x262626));
    p.text(head.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "Optimize Import for", theme::body(13.0), Tokens::text_strong());
    if let Some(k) = segmented(ui, &p, head.right() - 10.0, head.center().y, ("import-mode", 0), &MODES, d.mode) {
        d.mode = k;
    }
    let mut y = head.bottom() + 12.0;
    match d.mode {
        1 => {
            // Speed: a warning box with a yellow triangle.
            let b = Rect::from_min_max(pos2(x0, y), pos2(x1, y + 64.0));
            p.rect_filled(b, radius(7), theme::c(0x262626));
            let c = pos2(b.left() + 24.0, b.top() + 26.0);
            p.add(egui::Shape::convex_polygon(
                vec![c + vec2(0.0, -11.0), c + vec2(12.0, 10.0), c + vec2(-12.0, 10.0)],
                theme::c(0xf5c518),
                Stroke::NONE,
            ));
            p.text(c + vec2(0.0, 2.0), Align2::CENTER_CENTER, "!", theme::strong(12.0), Color32::BLACK);
            let tx = b.left() + 46.0;
            p.text(
                pos2(tx, b.top() + 18.0),
                Align2::LEFT_CENTER,
                "Warning: Some Geometry May Not Import Cleanly",
                theme::strong(13.0),
                Tokens::text_strong(),
            );
            let g = p.layout(
                "Speed skips every repair of the file's geometry. Bodies may come in with gaps, wrong edges or other faults that stop later modeling.".into(),
                theme::body(11.0),
                Tokens::text_dim(),
                b.right() - tx - 12.0,
            );
            p.galley(pos2(tx, b.top() + 30.0), g, Tokens::text_dim());
        }
        2 => {
            // Custom: one switch per clean-up, rows split by thin lines.
            let row_h = 56.0;
            let b = Rect::from_min_max(pos2(x0, y), pos2(x1, y + row_h * ROWS.len() as f32));
            p.rect_filled(b, radius(7), theme::c(0x262626));
            for (k, (title, what)) in ROWS.iter().enumerate() {
                let top = b.top() + row_h * k as f32;
                if k > 0 {
                    p.line_segment([pos2(b.left() + 10.0, top), pos2(b.right(), top)], Stroke::new(1.0, theme::c(0x343436)));
                }
                p.text(pos2(b.left() + 12.0, top + 16.0), Align2::LEFT_CENTER, *title, theme::strong(13.0), Tokens::text_strong());
                let g = p.layout((*what).into(), theme::body(11.0), Tokens::text_dim(), b.width() - 90.0);
                p.galley(pos2(b.left() + 12.0, top + 27.0), g, Tokens::text_dim());
                switch(ui, &p, b.right() - 12.0, top + row_h / 2.0, ("import-custom", k), &mut d.custom[k]);
            }
            y = b.bottom();
        }
        _ => {}
    }
    let _ = y;
    // Footer: "Don't show this again" with its switch, then Cancel and Import.
    let fy = r.bottom() - 30.0;
    p.line_segment([pos2(r.left(), fy - 30.0), pos2(r.right(), fy - 30.0)], Stroke::new(1.0, theme::c(0x343436)));
    p.text(pos2(r.left() + 20.0, fy), Align2::LEFT_CENTER, "Don't show this again", theme::body(13.0), Tokens::text_strong());
    switch(ui, &p, r.left() + 210.0, fy, ("import-dont-show", 0), &mut d.dont_show);
    let go = Rect::from_min_size(pos2(r.right() - 20.0 - 60.0, fy - 10.5), vec2(60.0, 21.0));
    let cancel = Rect::from_min_size(pos2(go.left() - 8.0 - 62.0, fy - 10.5), vec2(62.0, 21.0));
    let cr = ui.interact(cancel, ui.scope_id().with("import-cancel"), Sense::click());
    p.rect_filled(cancel, radius(5), if cr.hovered() { theme::c(0x6a6a6a) } else { theme::c(0x565656) });
    p.text(cancel.center(), Align2::CENTER_CENTER, "Cancel", theme::body(13.0), Color32::WHITE);
    let gr = ui.interact(go, ui.scope_id().with("import-go"), Sense::click());
    p.rect_filled(go, radius(5), if gr.hovered() { theme::c(0x3a62e8) } else { theme::c(0x2f55d4) });
    p.text(go.center(), Align2::CENTER_CENTER, "Import", theme::body(13.0), Color32::WHITE);
    let keys = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter)));
    if cr.clicked() || keys.0 {
        return;
    }
    if gr.clicked() || keys.1 {
        ws.settings.import_mode = MODES[d.mode].to_string();
        ws.settings.import_custom = d.custom;
        ws.settings.import_skip_prefs = d.dont_show;
        ws.import_file_go = Some((d.path, heal_param(d.mode, d.custom)));
        return;
    }
    ws.import_prefs = Some(d);
}
