//! Import Project (⇧⌘P), as observed (s14-150..152): a sheet with a sidebar (Search, Recents) and the projects as
//! a two-column grid of cards (thumbnail, name, when it changed); picking one enables Import, which brings its
//! items into this project in a "<name> (Imported)" folder. The cloud rows (Shared with Me, Drafts, Spaces) are
//! left out: everything here is local.

use std::path::PathBuf;
use std::time::SystemTime;

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use super::Workspace;
use crate::theme::{self, Tokens, radius};

pub struct ImportSheet {
    /// (name, folder, thumbnail, changed).
    pub projects: Vec<(String, PathBuf, Option<PathBuf>, SystemTime)>,
    pub selected: Option<usize>,
    pub query: String,
    pub scroll: f32,
    thumbs: Vec<Option<egui::TextureHandle>>,
}

impl ImportSheet {
    pub fn new(projects: Vec<(String, PathBuf, Option<PathBuf>, SystemTime)>) -> Self {
        let n = projects.len();
        Self { projects, selected: None, query: String::new(), scroll: 0.0, thumbs: (0..n).map(|_| None).collect() }
    }
}

/// "3:43 PM" today, else "10/23/26", as the cards show it.
fn when(t: SystemTime) -> String {
    use shapecraft_doc::date::{local_clock, local_date};
    let (y, mo, d) = local_date(t);
    if (y, mo, d) == local_date(SystemTime::now()) {
        let (h, m) = local_clock(t);
        return format!("{}:{m:02} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" });
    }
    format!("{mo}/{d}/{:02}", y.rem_euclid(100))
}

pub(super) fn dialog(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut sheet) = ws.import_sheet.take() else { return };
    // Measured from s14-150: about 860×680, a 240-wide sidebar, cards in two columns.
    let r = Rect::from_center_size(full.center(), vec2(860.0, 680.0));
    let p = ui.painter().clone();
    p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
    p.rect_filled(r.translate(vec2(0.0, 6.0)).expand(2.0), radius(12), Color32::from_black_alpha(90));
    p.rect_filled(r, radius(10), theme::c(0x1e1e1e));
    let side = Rect::from_min_size(r.min, vec2(240.0, r.height()));
    p.rect_filled(side, egui::CornerRadius { nw: 10, sw: 10, ne: 0, se: 0 }, theme::c(0x262626));
    p.line_segment([pos2(side.right(), r.top()), pos2(side.right(), r.bottom())], Stroke::new(1.0, Color32::BLACK));
    p.rect_stroke(r, radius(10), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
    // Sidebar: search field and Recents.
    let search = Rect::from_min_size(side.min + vec2(10.0, 16.0), vec2(220.0, 26.0));
    p.rect_filled(search, radius(6), theme::c(0x333335));
    p.text(search.left_center() + vec2(9.0, 0.0), Align2::LEFT_CENTER, "⌕", theme::body(13.0), Tokens::text_dim());
    let edit =
        egui::TextEdit::singleline(&mut sheet.query).hint_text(crate::i18n::tr("Search")).frame(egui::Frame::NONE).font(theme::body(12.5));
    ui.put(Rect::from_min_max(search.min + vec2(26.0, 4.0), search.max - vec2(6.0, 4.0)), edit);
    let recents = Rect::from_min_size(side.min + vec2(10.0, 54.0), vec2(220.0, 26.0));
    p.rect_filled(recents, radius(6), theme::c(0x3a3a3c));
    p.circle_stroke(recents.left_center() + vec2(16.0, 0.0), 6.0, Stroke::new(1.4, Tokens::icon_blue()));
    p.text(
        recents.left_center() + vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr("Recents"),
        theme::body(13.0),
        Tokens::text_strong(),
    );
    // Header.
    let main = Rect::from_min_max(pos2(side.right(), r.top()), r.max);
    p.text(
        pos2(main.left() + 70.0, r.top() + 29.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr("Recents"),
        theme::strong(13.0),
        Tokens::text_strong(),
    );
    for (k, ch) in ["‹", "›"].iter().enumerate() {
        let b = Rect::from_center_size(pos2(main.left() + 27.0 + 22.0 * k as f32, r.top() + 29.0), vec2(20.0, 20.0));
        p.rect_filled(b, radius(4), theme::c(0x333335));
        p.text(b.center(), Align2::CENTER_CENTER, *ch, theme::body(13.0), Tokens::text_faint());
    }
    p.line_segment([pos2(main.left(), r.top() + 56.0), pos2(main.right(), r.top() + 56.0)], Stroke::new(1.0, theme::c(0x2c2c2c)));
    // The grid, newest first, filtered by the search.
    let q = sheet.query.to_lowercase();
    let mut shown: Vec<usize> =
        (0..sheet.projects.len()).filter(|&i| q.is_empty() || sheet.projects[i].0.to_lowercase().contains(&q)).collect();
    shown.sort_by(|a, b| sheet.projects[*b].3.cmp(&sheet.projects[*a].3));
    let grid = Rect::from_min_max(pos2(main.left() + 18.0, r.top() + 70.0), pos2(main.right() - 18.0, r.bottom() - 56.0));
    let cw = (grid.width() - 20.0) / 2.0;
    let ch = cw * 0.75 + 52.0;
    let rows = shown.len().div_ceil(2);
    let max_scroll = (rows as f32 * (ch + 16.0) - grid.height()).max(0.0);
    if ui.rect_contains_pointer(grid) {
        sheet.scroll = (sheet.scroll - ui.input(|i| i.smooth_scroll_delta.y)).clamp(0.0, max_scroll);
    }
    let gp = p.with_clip_rect(grid);
    if shown.is_empty() {
        p.text(grid.center(), Align2::CENTER_CENTER, crate::i18n::tr("No projects"), theme::strong(13.0), Tokens::text_dim());
    }
    for (k, &i) in shown.iter().enumerate() {
        let (col, row) = ((k % 2) as f32, (k / 2) as f32);
        let card = Rect::from_min_size(pos2(grid.left() + col * (cw + 20.0), grid.top() + row * (ch + 16.0) - sheet.scroll), vec2(cw, ch));
        if card.bottom() < grid.top() || card.top() > grid.bottom() {
            continue;
        }
        let tile = Rect::from_min_size(card.min, vec2(cw, cw * 0.75));
        gp.rect_filled(tile, radius(6), theme::c(0x2b2b2b));
        if sheet.thumbs[i].is_none()
            && let Some(path) = &sheet.projects[i].2
            && let Some(ci) = crate::landing::load_png(path)
        {
            sheet.thumbs[i] = Some(ui.ctx().load_texture(path.to_string_lossy(), ci, egui::TextureOptions::LINEAR));
        }
        match &sheet.thumbs[i] {
            Some(t) => {
                gp.image(t.id(), tile.shrink(6.0), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                gp.rect_stroke(
                    Rect::from_center_size(tile.center(), vec2(22.0, 17.0)),
                    radius(2),
                    Stroke::new(1.5, Tokens::text_faint()),
                    StrokeKind::Inside,
                );
            }
        }
        let picked = sheet.selected == Some(i);
        if picked {
            gp.rect_stroke(tile, radius(6), Stroke::new(3.0, Tokens::accent()), StrokeKind::Outside);
        }
        gp.text(
            pos2(card.center().x, tile.bottom() + 17.0),
            Align2::CENTER_CENTER,
            &sheet.projects[i].0,
            theme::body(12.5),
            Tokens::text_strong(),
        );
        gp.text(
            pos2(card.center().x, tile.bottom() + 34.0),
            Align2::CENTER_CENTER,
            when(sheet.projects[i].3),
            theme::body(10.5),
            Tokens::text_dim(),
        );
        let resp = ui.interact(card.intersect(grid), ui.scope_id().with(("import-card", i)), Sense::click());
        if resp.clicked() {
            sheet.selected = Some(i);
        }
        if resp.double_clicked() {
            sheet.selected = Some(i);
            ws.import_go = true;
        }
    }
    // Cancel / Import.
    p.line_segment([pos2(main.left(), r.bottom() - 46.0), pos2(main.right(), r.bottom() - 46.0)], Stroke::new(1.0, theme::c(0x2c2c2c)));
    let import = Rect::from_min_size(pos2(main.right() - 92.0, r.bottom() - 34.0), vec2(76.0, 22.0));
    let cancel = import.translate(vec2(-86.0, 0.0));
    p.rect_filled(cancel, radius(5), theme::c(0x5b5b5e));
    p.text(cancel.center(), Align2::CENTER_CENTER, crate::i18n::tr("Cancel"), theme::body(12.5), Tokens::text_strong());
    let can = sheet.selected.is_some();
    p.rect_filled(import, radius(5), if can { Tokens::accent() } else { theme::c(0x3a3a3c) });
    p.text(
        import.center(),
        Align2::CENTER_CENTER,
        crate::i18n::tr("Import"),
        theme::body(12.5),
        if can { Color32::WHITE } else { Tokens::text_faint() },
    );
    let cancelled = ui.interact(cancel, ui.scope_id().with("import-cancel"), Sense::click()).clicked()
        || ui.input(|i| i.key_pressed(egui::Key::Escape));
    let go = can
        && (ws.import_go
            || ui.interact(import, ui.scope_id().with("import-go"), Sense::click()).clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Enter)));
    ws.import_go = false;
    if cancelled {
        return;
    }
    if go && let Some(i) = sheet.selected {
        let path = sheet.projects[i].1.clone();
        if ws.run("import_project", json!({ "path": path })).is_some() {
            ws.standard_view(shapecraft_render::StandardView::Default);
        }
        return;
    }
    ws.import_sheet = Some(sheet);
}
