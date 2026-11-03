//! The Drawings workspace, as observed: a workspace switcher sliding in from the left, a Create Drawing flow
//! (pick bodies, then Drawing Preferences), the sheet drawn as a navy blueprint, a Properties sidebar and an
//! export sheet.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::{ItemId, Pick};
use shapecraft_engine::drawing::{self, Drawing, Mark, SCALES};

use super::Workspace;
use super::tools::Tool;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

pub const SHEETS: [&str; 7] = ["A4", "A3", "A2", "A1", "A0", "Letter", "Tabloid"];

/// The open drawing: which one, its sheet as last made (with the key it was made for), and whether the
/// Properties sidebar shows.
pub struct DrawingWs {
    pub id: ItemId,
    pub cache: Option<(u64, Result<Drawing, String>)>,
    pub props: bool,
    /// The annotation tool running, the points picked so far (sheet mm), a note being typed (where, text, focus),
    /// and the annotation clicked.
    pub tool: Option<super::annotate::DrawTool>,
    pub picks: Vec<glam::DVec2>,
    pub note: Option<(glam::DVec2, String, bool)>,
    pub selected: Option<usize>,
    /// The Views tool running, the view selected, a sheet made with ghost views for it (and its key), and the view
    /// being dragged (with where the drag began, sheet mm).
    pub view_tool: Option<super::sheet_views::ViewTool>,
    pub view_sel: Option<String>,
    pub preview: Option<(u64, Drawing)>,
    pub drag: Option<(String, glam::DVec2)>,
    /// A dimension's Prefix / Tolerances / Suffix card (opened by double-clicking it).
    pub dim_card: Option<super::annotate::DimCard>,
}

/// The Drawing Preferences dialog's fields.
#[derive(Clone)]
pub struct Prefs {
    pub bodies: Vec<ItemId>,
    pub name: String,
    pub landscape: bool,
    pub sheet: usize,
    pub scale: Option<f64>,
    pub four_views: bool,
}

/// The export sheet's choices.
#[derive(Clone, Copy)]
pub struct ExportChoice {
    pub pdf: bool,
    pub dxf: bool,
    pub svg: bool,
    pub dwg: bool,
}

fn key(ws: &Workspace, id: ItemId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ws.session.revision.hash(&mut h);
    if let Some(d) = ws.session.doc.drawing(id) {
        serde_json::to_string(d).unwrap_or_default().hash(&mut h);
    }
    h.finish()
}

impl Workspace {
    /// Opens a drawing in the Drawings workspace.
    pub fn open_drawing_public(&mut self, id: ItemId) {
        self.open_drawing(id);
    }

    /// Opens a drawing in the Drawings workspace.
    pub(super) fn open_drawing(&mut self, id: ItemId) {
        self.visualizing = false;
        self.tool = Tool::Idle;
        self.session.selection.clear();
        self.drawings = Some(DrawingWs {
            id,
            cache: None,
            props: true,
            tool: None,
            picks: Vec::new(),
            note: None,
            selected: None,
            view_tool: None,
            view_sel: None,
            preview: None,
            drag: None,
            dim_card: None,
        });
        self.switcher = false;
    }

    /// Starts Create Drawing: straight to the preferences with bodies selected, else pick them first.
    pub(super) fn begin_create_drawing(&mut self) {
        self.switcher = false;
        self.drawings = None;
        self.visualizing = false;
        let bodies: Vec<ItemId> = self.session.selection.iter().filter_map(Pick::body).collect();
        if bodies.is_empty() {
            self.creating_drawing = true;
            self.tool = Tool::Idle;
        } else {
            self.drawing_prefs = Some(prefs_for(self, bodies));
        }
    }

    /// The sheet for the open drawing, remade when the model or its settings change.
    fn current_sheet(&mut self) -> Option<&Result<Drawing, String>> {
        let id = self.drawings.as_ref()?.id;
        let k = key(self, id);
        if self.drawings.as_ref().and_then(|d| d.cache.as_ref()).is_none_or(|(ck, _)| *ck != k) {
            let made = match self.session.doc.drawing(id) {
                Some(spec) => drawing::make(&self.session, spec).map_err(|e| e.to_string()),
                None => Err("This drawing was deleted".to_string()),
            };
            if let Some(d) = &mut self.drawings {
                d.cache = Some((k, made));
            }
        }
        self.drawings.as_ref().and_then(|d| d.cache.as_ref()).map(|(_, r)| r)
    }
}

fn prefs_for(ws: &Workspace, bodies: Vec<ItemId>) -> Prefs {
    let name = bodies.first().and_then(|b| ws.session.doc.body(*b)).map_or_else(|| ws.session.doc.name.clone(), |b| b.name.clone());
    Prefs { bodies, name, landscape: true, sheet: 1, scale: None, four_views: false }
}

/// The sheet: a light canvas with the drawing in white on navy, as observed.
pub(super) fn sheet_view(ws: &mut Workspace, ui: &mut Ui) {
    // The sheet fills the window under the title bar, the sidebars over it (as observed, 115).
    let rect = ws.view_rect;
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme::c(0xededed));
    let Some(made) = ws.current_sheet() else { return };
    let d = match made {
        Ok(d) => d.clone(),
        Err(e) => {
            p.text(rect.center(), Align2::CENTER_CENTER, e, theme::body(13.0), theme::c(0x6b6b70));
            return;
        }
    };
    // Measured (115): 64 px above the sheet, 69 below, centred on the window.
    let avail = Rect::from_min_max(rect.min + vec2(100.0, 64.0), rect.max - vec2(100.0, 69.0));
    let k = (avail.width() / d.sheet.x as f32).min(avail.height() / d.sheet.y as f32).max(0.05);
    let size = vec2(d.sheet.x as f32 * k, d.sheet.y as f32 * k);
    let sheet = Rect::from_center_size(avail.center(), size);
    let to = |q: glam::DVec2| pos2(sheet.left() + q.x as f32 * k, sheet.bottom() - q.y as f32 * k);
    p.rect_filled(sheet, 0.0, theme::c(0x00293f));
    let ink = Color32::WHITE;
    for m in d.marks() {
        match m {
            Mark::Line { a, b, width, dashed } => {
                let stroke = Stroke::new((width as f32 * k * 1.6).max(0.7), if dashed { Color32::from_white_alpha(170) } else { ink });
                if dashed {
                    p.add(egui::Shape::dashed_line(&[to(a), to(b)], stroke, 4.0, 3.0));
                } else {
                    p.line_segment([to(a), to(b)], stroke);
                }
            }
            Mark::Text { at, text, size, mono, center } => {
                let font =
                    if mono { theme::mono((size as f32 * k * 1.25).max(6.0)) } else { theme::body((size as f32 * k * 1.25).max(6.0)) };
                let align = if center { Align2::CENTER_CENTER } else { Align2::LEFT_BOTTOM };
                p.text(to(at), align, text, font, ink);
            }
            Mark::Image { min, max, image } => {
                if let Some(pic) = d.pictures.iter().find(|x| x.id.0 == image).cloned()
                    && let Some(tex) = super::decals::thumb(ws, ui.ctx(), &pic)
                {
                    p.image(tex, Rect::from_two_pos(to(min), to(max)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
            }
        }
    }
    if !super::sheet_views::interact(ws, ui, &d, sheet, k) {
        super::annotate::interact(ws, ui, &d, sheet, k);
        super::annotate::dim_card(ws, ui);
    }
}

/// The Drawings workspace's right column: Properties toggle and Export drawing…, as observed.
pub(super) fn right_column(ws: &mut Workspace, ui: &mut Ui) {
    let vp = ws.viewport_rect;
    let x = vp.right() - 5.0 - Tokens::TOOL_BUTTON;
    let rows = [("Properties", "⌥⌘P", Icon::SidebarToggle), ("Export drawing…", "", Icon::Export)];
    let mut y = vp.top() + 9.0;
    for (i, (label, keys, icon)) in rows.iter().enumerate() {
        let def = super::toolbar::ToolDef { icon: *icon, label, sub: None, keys, action: super::toolbar::Action::Soon, flyout: false };
        let on = i == 0 && ws.drawings.as_ref().is_some_and(|d| d.props);
        let clicked = super::toolbar::group(ui, pos2(x, y), std::slice::from_ref(&def), &[on], false, &format!("dw{i}")).is_some();
        let chip = super::toolbar::chip(ui, pos2(x - 6.0, y + 1.0 + Tokens::TOOL_PITCH / 2.0), &def, true, false);
        let chip_clicked = ui.interact(chip, ui.scope_id().with(("dw-chip", i)), Sense::click()).clicked();
        if clicked || chip_clicked {
            if i == 0 {
                if let Some(d) = &mut ws.drawings {
                    d.props = !d.props;
                }
            } else {
                ws.export_choice = Some(ExportChoice { pdf: true, dxf: false, svg: false, dwg: false });
            }
        }
        y += super::toolbar::group_height(1) + super::toolbar::GROUP_GAP;
    }
    let sep_y = y + 4.0;
    ui.painter().line_segment([pos2(x + 11.0, sep_y), pos2(x + 33.0, sep_y)], Stroke::new(2.0, Tokens::tool_separator()));
}

fn row_label(p: &egui::Painter, at: Pos2, text: &str) {
    p.text(at, Align2::LEFT_CENTER, text, theme::body(12.5), Tokens::text());
}

/// A labelled pop-up of choices; true when one was picked.
#[allow(clippy::too_many_arguments)]
fn pick<T: PartialEq + Clone>(
    ui: &mut Ui,
    p: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    label: &str,
    id: &str,
    current: &mut T,
    options: &[(String, T)],
) -> bool {
    row_label(p, pos2(left, y), label);
    let names: Vec<String> = options.iter().map(|o| o.0.clone()).collect();
    let at = options.iter().position(|(_, v)| v == current);
    match super::popover::pill_select(ui, id, pos2(right, y), 0.0, &names, at).and_then(|i| options.get(i)) {
        Some((_, v)) => {
            *current = v.clone();
            true
        }
        None => false,
    }
}

/// The Properties sidebar: the sheet settings that drive the drawing (and the rest shown as they are).
pub(super) fn properties(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    // A sidebar like the others (it takes the pointer, so the sheet under it doesn't).
    super::panels::panel(ui, r, None);
    let p = ui.painter().clone();
    p.text(pos2(r.center().x, r.top() + 18.0), Align2::CENTER_CENTER, "Properties", theme::body(12.0), Tokens::text_dim());
    let Some(id) = ws.drawings.as_ref().map(|d| d.id) else { return };
    let Some(spec) = ws.session.doc.drawing(id).cloned() else { return };
    let mut y = r.top() + 46.0;
    let left = r.left() + 14.0;
    let right = r.right() - 14.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Sheet", theme::strong(12.5), Tokens::text_strong());
    y += 22.0;
    // Orientation tiles.
    let mut update = serde_json::Map::new();
    for (k, (label, landscape)) in [("Portrait", false), ("Landscape", true)].iter().enumerate() {
        let c = pos2(right - 92.0 + k as f32 * 62.0, y + 12.0);
        let tile = Rect::from_center_size(c, if *landscape { vec2(40.0, 30.0) } else { vec2(26.0, 34.0) });
        let resp = ui.interact(tile.expand(6.0), ui.scope_id().with(("orient", k)), Sense::click());
        let on = spec.landscape == *landscape;
        p.rect_filled(tile, radius(3), theme::c(0x5a5a5e));
        if on {
            p.rect_stroke(tile.expand(3.0), radius(5), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
            p.circle_filled(tile.center(), 5.0, Tokens::accent());
        }
        p.text(pos2(c.x, y + 40.0), Align2::CENTER_CENTER, *label, theme::body(11.5), Tokens::text());
        if resp.clicked() && !on {
            update.insert("landscape".into(), json!(landscape));
        }
    }
    row_label(&p, pos2(left, y + 40.0), "Orientation");
    y += 64.0;
    let mut combo =
        |ui: &mut Ui, y: f32, label: &str, id: &str, current: String, options: Vec<(String, serde_json::Value)>, field: &str| {
            row_label(&p, pos2(left, y), label);
            let names: Vec<String> = options.iter().map(|o| o.0.clone()).collect();
            let at = names.iter().position(|n| *n == current);
            if let Some((_, value)) = super::popover::pill_select(ui, id, pos2(right, y), 0.0, &names, at).and_then(|i| options.get(i)) {
                update.insert(field.into(), value.clone());
            }
        };
    let sheet_label = |s: &str| if s.starts_with('A') { format!("ISO {s}") } else { s.to_string() };
    combo(ui, y, "Sheet Size", "dw-sheet", sheet_label(&spec.sheet), SHEETS.iter().map(|s| (sheet_label(s), json!(s))).collect(), "sheet");
    y += 30.0;
    let scale_text = spec.scale.map_or_else(|| "Auto".to_string(), drawing::scale_label);
    let mut scales = vec![("Auto".to_string(), serde_json::Value::Null)];
    scales.extend(SCALES.iter().map(|s| (drawing::scale_label(*s), json!(s))));
    combo(ui, y, "View Scale", "dw-scale", scale_text, scales, "scale");
    y += 30.0;
    combo(
        ui,
        y,
        "Projection",
        "dw-proj",
        if spec.first_angle { "First Angle".into() } else { "Third Angle".into() },
        vec![("First Angle".into(), json!(true)), ("Third Angle".into(), json!(false))],
        "first_angle",
    );
    y += 30.0;
    // Title block layouts and date formats (observed s14-120).
    let mut sheet_style = spec.style.clone();
    let mut sheet_changed = false;
    let blocks: Vec<(String, String)> = shapecraft_doc::TITLE_BLOCKS.iter().map(|b| (b.to_string(), b.to_string())).collect();
    sheet_changed |= pick(ui, &p, left, right, y, "Title Block", "dw-title", &mut sheet_style.title_block, &blocks);
    y += 30.0;
    let dates: Vec<(String, String)> = shapecraft_doc::DATE_FORMATS.iter().map(|b| (b.to_string(), b.to_string())).collect();
    sheet_changed |= pick(ui, &p, left, right, y, "Date Format", "dw-date", &mut sheet_style.date_format, &dates);
    if sheet_changed {
        update.insert("style".into(), json!(sheet_style));
    }
    y += 34.0;
    p.line_segment([pos2(left, y), pos2(right, y)], Stroke::new(1.0, Tokens::chip_border()));
    y += 18.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Dimensions", theme::strong(12.5), Tokens::text_strong());
    y += 26.0;
    row_label(&p, pos2(left, y), "Units");
    let units = match ws.session.doc.units.suffix() {
        "mm" => "Millimeter",
        "cm" => "Centimeter",
        "m" => "Meter",
        "in" => "Inch",
        "ft" => "Foot",
        other => other,
    };
    let _ = super::popover::pill_select(ui, "dw-units", pos2(right, y), 0.0, &[units.to_string()], Some(0));
    let mut style = spec.style.clone();
    let sep = if style.decimal_separator == "Comma" { "," } else { "." };
    let digits = |n: u8, suffix: &str| if n == 0 { format!("0{suffix}") } else { format!("0{sep}{}{suffix}", &"123456"[..n as usize]) };
    let mut changed = false;
    y += 30.0;
    changed |= pick(
        ui,
        &p,
        left,
        right,
        y,
        "Angle Format",
        "dw-angfmt",
        &mut style.angle_format,
        &[("Decimal".into(), "Decimal".to_string()), ("Degrees, Minutes, Seconds".into(), "Degrees, Minutes, Seconds".to_string())],
    );
    y += 30.0;
    let precisions = |suffix: &str| (0..=4u8).map(|n| (digits(n, suffix), n)).collect::<Vec<_>>();
    changed |= pick(ui, &p, left, right, y, "Length Precision", "dw-lenprec", &mut style.length_precision, &precisions(""));
    y += 30.0;
    changed |= pick(ui, &p, left, right, y, "Angle Precision", "dw-angprec", &mut style.angle_precision, &precisions("°"));
    y += 30.0;
    changed |= pick(
        ui,
        &p,
        left,
        right,
        y,
        "Decimal Separator",
        "dw-sep",
        &mut style.decimal_separator,
        &[("Period".into(), "Period".to_string()), ("Comma".into(), "Comma".to_string())],
    );
    y += 34.0;
    p.line_segment([pos2(left, y), pos2(right, y)], Stroke::new(1.0, Tokens::chip_border()));
    y += 18.0;
    p.text(pos2(left, y), Align2::LEFT_CENTER, "Line Widths", theme::strong(12.5), Tokens::text_strong());
    let pens: Vec<(String, f64)> =
        [0.10, 0.13, 0.15, 0.18, 0.25, 0.35, 0.40, 0.50, 0.70, 1.00].iter().map(|w| (format!("{w:.2} mm").replace('.', sep), *w)).collect();
    let rows: [(&str, &str, &mut f64); 6] = [
        ("Visible Outlines", "dw-w-vis", &mut style.visible),
        ("Hidden Lines", "dw-w-hid", &mut style.hidden),
        ("Dimension Lines", "dw-w-dim", &mut style.dimension),
        ("Centerlines", "dw-w-cl", &mut style.centerline),
        ("Section Lines", "dw-w-sec", &mut style.section),
        ("Detail Marks", "dw-w-det", &mut style.detail),
    ];
    for (label, id, value) in rows {
        y += 30.0;
        changed |= pick(ui, &p, left, right, y, label, id, value, &pens);
    }
    if changed {
        update.insert("style".into(), json!(style));
    }
    if !update.is_empty() {
        update.insert("drawing".into(), json!(id.0));
        let _ = ws.run("update_drawing", serde_json::Value::Object(update));
    }
}

/// The workspace switcher sliding in from the left (everything else dimmed), as observed.
pub(super) fn switcher(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let top = full.top() + Tokens::TITLEBAR_H;
    let dim = Rect::from_min_max(pos2(full.left(), top), full.max);
    let p = ui.painter().clone();
    let panel = Rect::from_min_max(pos2(full.left(), top), pos2(full.left() + 220.0, full.bottom()));
    p.rect_filled(dim, 0.0, Color32::from_black_alpha(110));
    p.rect_filled(panel, 0.0, theme::c(0x2b2b2d));
    p.line_segment([panel.right_top(), panel.right_bottom()], Stroke::new(1.0, theme::c(0x1a1a1a)));
    let mut y = panel.top() + 14.0;
    let row = |ui: &mut Ui, y: f32, icon: Icon, label: &str, on: bool, id: (&str, u64)| -> bool {
        let r = Rect::from_min_size(pos2(panel.left() + 8.0, y), vec2(panel.width() - 16.0, 36.0));
        let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
        if on {
            ui.painter().rect_filled(r, radius(6), theme::c(0x48484b));
        } else if resp.hovered() {
            ui.painter().rect_filled(r, radius(6), theme::c(0x3a3a3c));
        }
        let cell = Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(28.0, 28.0));
        ui.painter().rect_filled(cell, radius(6), theme::c(0x3d3d3f));
        icons::paint(ui.painter(), cell.shrink(4.0), icon, Tokens::icon());
        ui.painter().text(pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, label, theme::body(12.5), Tokens::text());
        resp.clicked()
    };
    let modeling = ws.drawings.is_none() && !ws.visualizing;
    if row(ui, y, Icon::Modeling, "Modeling", modeling, ("sw", 0)) {
        ws.drawings = None;
        ws.visualizing = false;
        ws.switcher = false;
    }
    y += 40.0;
    if row(ui, y, Icon::Visualization, "Visualization", ws.visualizing, ("sw", 1)) {
        ws.drawings = None;
        ws.visualizing = true;
        ws.switcher = false;
    }
    y += 48.0;
    p.text(pos2(panel.left() + 16.0, y), Align2::LEFT_CENTER, "Drawings", theme::strong(10.5), Tokens::text_faint());
    y += 14.0;
    let list: Vec<(ItemId, String)> = ws.session.doc.drawings.iter().map(|d| (d.id, d.name.clone())).collect();
    for (id, name) in list {
        let open = ws.drawings.as_ref().is_some_and(|d| d.id == id);
        if row(ui, y, Icon::Drawings, &name, open, ("sw-dw", id.0)) {
            ws.open_drawing(id);
        }
        y += 40.0;
    }
    if row(ui, y, Icon::Plus, "Create Drawing", false, ("sw", 2)) {
        ws.begin_create_drawing();
    }
    let outside =
        ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| dim.contains(q) && !panel.contains(q)));
    if outside && ws.now - ws.popup_opened_at > 0.15 {
        ws.switcher = false;
    }
}

/// A centered dark dialog with a title; returns its rect.
pub(super) fn dialog(ui: &mut Ui, full: Rect, size: egui::Vec2, title: &str) -> Rect {
    let p = ui.painter();
    p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
    let r = Rect::from_center_size(full.center(), size);
    p.rect_filled(r.translate(vec2(0.0, 6.0)), radius(12), Color32::from_black_alpha(90));
    p.rect_filled(r, radius(11), theme::c(0x2a2a2c));
    p.rect_stroke(r, radius(11), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
    p.text(pos2(r.left() + 16.0, r.top() + 20.0), Align2::LEFT_CENTER, title, theme::strong(13.0), Tokens::text_strong());
    r
}

pub(super) fn button(ui: &mut Ui, r: Rect, label: &str, primary: bool, id: &str) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
    let fill = match (primary, resp.hovered()) {
        (true, false) => theme::c(0x2f55d4),
        (true, true) => theme::c(0x3a62e8),
        (false, false) => theme::c(0x58585c),
        (false, true) => theme::c(0x68686c),
    };
    ui.painter().rect_filled(r, radius(6), fill);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, theme::body(12.5), Color32::WHITE);
    resp.clicked()
}

/// Drawing Preferences, as observed: name, orientation, sheet size, scale, include four views.
pub(super) fn prefs_dialog(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut prefs) = ws.drawing_prefs.clone() else { return };
    let r = dialog(ui, full, vec2(380.0, 420.0), "Drawing Preferences");
    let p = ui.painter().clone();
    let lx = r.left() + 130.0;
    let mut y = r.top() + 58.0;
    p.text(pos2(lx - 8.0, y), Align2::RIGHT_CENTER, "Name:", theme::body(12.5), Tokens::text());
    ui.put(Rect::from_min_size(pos2(lx, y - 11.0), vec2(r.right() - lx - 18.0, 22.0)), egui::TextEdit::singleline(&mut prefs.name));
    y += 58.0;
    p.text(pos2(lx - 8.0, y), Align2::RIGHT_CENTER, "Orientation:", theme::body(12.5), Tokens::text());
    for (k, (label, landscape)) in [("Portrait", false), ("Landscape", true)].iter().enumerate() {
        let c = pos2(lx + 24.0 + k as f32 * 70.0, y - 4.0);
        let tile = Rect::from_center_size(c, if *landscape { vec2(42.0, 30.0) } else { vec2(28.0, 38.0) });
        let resp = ui.interact(tile.expand(6.0), ui.scope_id().with(("pref-orient", k)), Sense::click());
        p.rect_filled(tile, radius(3), theme::c(0x5a5a5e));
        if prefs.landscape == *landscape {
            p.rect_stroke(tile.expand(3.0), radius(5), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
            p.circle_filled(tile.center(), 5.0, Tokens::accent());
        }
        p.text(pos2(c.x, y + 26.0), Align2::CENTER_CENTER, *label, theme::body(11.5), Tokens::text());
        if resp.clicked() {
            prefs.landscape = *landscape;
        }
    }
    y += 62.0;
    p.text(pos2(lx - 8.0, y), Align2::RIGHT_CENTER, "Sheet Size:", theme::body(12.5), Tokens::text());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(lx, y - 11.0), vec2(200.0, 22.0))));
    let sheet_label = |i: usize| {
        let name = SHEETS[i.min(SHEETS.len() - 1)];
        let (w, h) = shapecraft_doc::DrawingSheet::paper(name, true).unwrap_or((0.0, 0.0));
        format!("{}{name} ({} × {} mm)", if name.starts_with('A') { "ISO " } else { "" }, w.round(), h.round())
    };
    egui::ComboBox::from_id_salt("pref-sheet").selected_text(sheet_label(prefs.sheet)).width(196.0).show_ui(&mut child, |ui| {
        for i in 0..SHEETS.len() {
            ui.selectable_value(&mut prefs.sheet, i, sheet_label(i));
        }
    });
    y += 40.0;
    p.text(pos2(lx - 8.0, y - 7.0), Align2::RIGHT_CENTER, "View-to-", theme::body(12.5), Tokens::text());
    p.text(pos2(lx - 8.0, y + 8.0), Align2::RIGHT_CENTER, "Sheet Scale:", theme::body(12.5), Tokens::text());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(lx, y - 11.0), vec2(120.0, 22.0))));
    let shown = prefs.scale.map_or_else(|| "Auto".to_string(), drawing::scale_label);
    egui::ComboBox::from_id_salt("pref-scale").selected_text(shown).width(100.0).show_ui(&mut child, |ui| {
        ui.selectable_value(&mut prefs.scale, None, "Auto");
        for s in SCALES {
            ui.selectable_value(&mut prefs.scale, Some(s), drawing::scale_label(s));
        }
    });
    y += 44.0;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(lx, y - 11.0), vec2(220.0, 22.0))));
    child.checkbox(&mut prefs.four_views, "Include 4 Views");
    p.text(pos2(lx + 26.0, y + 16.0), Align2::LEFT_CENTER, "Front, Left, Top, Isometric", theme::body(10.5), Tokens::text_dim());
    let foot = r.bottom() - 30.0;
    p.rect_filled(
        Rect::from_min_max(pos2(r.left() + 1.0, foot - 22.0), pos2(r.right() - 1.0, r.bottom() - 1.0)),
        radius(10),
        theme::c(0x242426),
    );
    if button(ui, Rect::from_center_size(pos2(r.left() + 52.0, foot), vec2(72.0, 22.0)), "Cancel", false, "pref-cancel") {
        ws.drawing_prefs = None;
        return;
    }
    if button(ui, Rect::from_center_size(pos2(r.right() - 52.0, foot), vec2(72.0, 22.0)), "Done", true, "pref-done") {
        let ids: Vec<u64> = prefs.bodies.iter().map(|b| b.0).collect();
        let params = json!({
            "name": if prefs.name.trim().is_empty() { "Drawing".to_string() } else { prefs.name.clone() },
            "bodies": ids,
            "sheet": SHEETS[prefs.sheet.min(SHEETS.len() - 1)],
            "landscape": prefs.landscape,
            "scale": prefs.scale,
            "four_views": prefs.four_views,
        });
        ws.drawing_prefs = None;
        if let Some(out) = ws.run("create_drawing", params)
            && let Some(id) = out["drawing"].as_u64()
        {
            ws.open_drawing(ItemId(id));
            // Without the four views the sheet starts empty, in Base View (as observed).
            if !prefs.four_views {
                ws.start_view_tool(super::sheet_views::ViewTool::Base { key: "front", popover: false, at: None });
            }
        }
        return;
    }
    ws.drawing_prefs = Some(prefs);
}

/// The export sheet: pick formats, then the app asks where to save.
pub(super) fn export_dialog(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut c) = ws.export_choice else { return };
    let r = dialog(ui, full, vec2(420.0, 260.0), "Export Drawing");
    let p = ui.painter().clone();
    let mut y = r.top() + 60.0;
    for (k, (label, on, enabled)) in
        [("PDF", c.pdf, true), ("DXF from Drawing", c.dxf, true), ("SVG", c.svg, true), ("DWG from Drawing", c.dwg, true)]
            .into_iter()
            .enumerate()
    {
        let mut v = on;
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(r.left() + 24.0, y - 11.0), vec2(300.0, 22.0))));
        child.add_enabled(enabled, egui::Checkbox::new(&mut v, label));
        match k {
            0 => c.pdf = v,
            1 => c.dxf = v,
            2 => c.svg = v,
            3 => c.dwg = v,
            _ => {}
        }
        y += 30.0;
    }
    p.text(pos2(r.left() + 24.0, y + 4.0), Align2::LEFT_CENTER, "Choose one or more formats.", theme::body(11.0), Tokens::text_dim());
    let foot = r.bottom() - 26.0;
    if button(ui, Rect::from_center_size(pos2(r.right() - 140.0, foot), vec2(80.0, 24.0)), "Cancel", false, "exp-cancel") {
        ws.export_choice = None;
        return;
    }
    let any = c.pdf || c.dxf || c.svg || c.dwg;
    if button(ui, Rect::from_center_size(pos2(r.right() - 52.0, foot), vec2(80.0, 24.0)), "Export", any, "exp-go") && any {
        ws.export_choice = None;
        let mut formats = Vec::new();
        if c.pdf {
            formats.push("pdf");
        }
        if c.dxf {
            formats.push("dxf");
        }
        if c.svg {
            formats.push("svg");
        }
        if c.dwg {
            formats.push("dwg");
        }
        ws.wants_drawing_export = Some(formats);
        return;
    }
    ws.export_choice = Some(c);
}
