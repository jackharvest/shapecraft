//! The Export dialog (⇧⌘E), as observed in 26.200 (s12-101..110): a format list on the left (click picks one,
//! ⌘-click adds), each chosen format's settings on the right, then the file name and the shared switches, and
//! Cancel / Export (n). The account-only rows (Share & Publish) and the formats we can't write (JT, DWG from Sketch)
//! are left out; DWG from Drawing is written as AutoCAD R2000 (crate shapecraft-dwg); Shapr3D and Parasolid are written from the published Parasolid format; Slicer hands a 3MF to an installed slicer (see `slicers`).

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Shapr,
    Parasolid,
    Slicer,
    ThreeMf,
    Step,
    DxfSketch,
    DxfDrawing,
    DwgDrawing,
    Iges,
    Stl,
    Glb,
    Obj,
    Usdz,
    Svg,
    Pdf,
    Image,
}

/// (format, list label, badge, settings title, our own one-line description).
const FORMATS: &[(Format, &str, &str, &str, &str)] = &[
    (Format::Shapr, "Shapr3D", "S3D", "Shapr3D Project", "A Shapr3D project; one that came from Shapr3D keeps its history."),
    (Format::Slicer, "Slicer", "3D", "Open in a Slicer", "A 3MF of the bodies, opened straight in your slicer for printing."),
    (
        Format::ThreeMf,
        "3MF",
        "3MF",
        "3D Manufacturing Format",
        "Meshes of the bodies in one package, for 3D printing and quick prototypes.",
    ),
    (Format::Step, "STEP", "STEP", "STEP (ISO 10303)", "Exact solid bodies that every CAD system and most renderers read."),
    (Format::DxfSketch, "DXF from Sketch", "DXF", "DXF from Sketch", "A sketch's curves, flat in its plane, for laser cutters and 2D CAD."),
    (Format::DxfDrawing, "DXF from Drawing", "DXF", "DXF from Drawing", "The drawing sheet's views and annotations as 2D CAD lines."),
    (Format::Iges, "IGES", "IGES", "IGES", "Exact surfaces and solids for older CAD and CAM software."),
    (
        Format::Parasolid,
        "Parasolid",
        "X_B",
        "Parasolid (binary)",
        "Exact solids for Parasolid-based CAD; several bodies give one file each.",
    ),
    (Format::Stl, "STL", "STL", "STL Triangle Mesh", "A bare triangle mesh for slicers and mesh tools."),
    (Format::Glb, "GLB", "GLB", "Binary glTF", "Meshes with their materials for the web, game engines and AR viewers."),
    (Format::Obj, "OBJ", "OBJ", "Wavefront OBJ", "Meshes with a material file of the body colors."),
    (Format::Usdz, "USDZ", "USDZ", "Universal Scene Description", "Meshes with their materials for AR Quick Look and USD viewers."),
    (Format::DwgDrawing, "DWG from Drawing", "DWG", "DWG from Drawing", "The drawing sheet's views and annotations as an AutoCAD drawing."),
    (Format::Svg, "SVG", "SVG", "SVG from Drawing", "The drawing sheet as a vector image."),
    (Format::Pdf, "PDF", "PDF", "PDF from Drawing", "The drawing sheet, ready to print."),
    (Format::Image, "Image", "PNG", "Image", "The current view as a PNG picture."),
];

/// How many of `FORMATS` are in the Recommended section; the rest are Everything Else.
const RECOMMENDED: usize = 6;

impl Format {
    fn info(self) -> (&'static str, &'static str, &'static str, &'static str) {
        FORMATS.iter().find(|f| f.0 == self).map_or(("", "", "", ""), |f| (f.1, f.2, f.3, f.4))
    }

    /// The file extension and the engine command writing it (`None`: the app handles it, e.g. the view image).
    pub fn ext_and_command(self) -> (&'static str, Option<&'static str>) {
        match self {
            Format::Shapr => ("shapr", Some("export_shapr")),
            Format::Parasolid => ("x_b", Some("export_parasolid")),
            Format::ThreeMf | Format::Slicer => ("3mf", Some("export_3mf")),
            Format::Usdz => ("usdz", Some("export_usdz")),
            Format::Step => ("step", Some("export_step")),
            Format::DxfSketch => ("dxf", Some("export_sketch")),
            Format::DxfDrawing => ("dxf", Some("export_drawing")),
            Format::DwgDrawing => ("dwg", Some("export_drawing")),
            Format::Iges => ("igs", Some("export_iges")),
            Format::Stl => ("stl", Some("export_stl")),
            Format::Glb => ("glb", Some("export_glb")),
            Format::Obj => ("obj", Some("export_obj")),
            Format::Svg => ("svg", Some("export_drawing")),
            Format::Pdf => ("pdf", Some("export_drawing")),
            Format::Image => ("png", None),
        }
    }

    /// Mesh formats export each body on its own when "Save Each 1st-Level Item Separately" is on.
    pub fn per_body(self) -> bool {
        matches!(self, Format::ThreeMf | Format::Step | Format::Iges | Format::Stl | Format::Glb | Format::Obj | Format::Usdz)
    }

    /// Its name in the list ("STEP", "DXF from Sketch").
    pub fn label(self) -> &'static str {
        self.info().0
    }

    pub fn from_label(name: &str) -> Option<Format> {
        FORMATS.iter().find(|f| f.1 == name).map(|f| f.0)
    }

    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext {
            "shapr" => Format::Shapr,
            "x_b" => Format::Parasolid,
            "3mf" => Format::ThreeMf,
            "slicer" => Format::Slicer,
            "step" | "stp" => Format::Step,
            "iges" | "igs" => Format::Iges,
            "stl" => Format::Stl,
            "glb" => Format::Glb,
            "obj" => Format::Obj,
            "usdz" => Format::Usdz,
            "dwg" => Format::DwgDrawing,
            "svg" => Format::Svg,
            "pdf" => Format::Pdf,
            "png" => Format::Image,
            _ => return None,
        })
    }
}

/// The dialog's state, kept while it is open.
#[derive(Clone, Debug)]
pub struct ExportDialog {
    pub chosen: Vec<Format>,
    /// Typed name; empty means the project's name (shown greyed in the field, as observed).
    pub file_name: String,
    pub project_name: String,
    pub hidden: bool,
    pub separately: bool,
    pub high: bool,
    pub colors: bool,
    pub stl_ascii: bool,
    pub stl_units: usize,
    /// STEP application protocol: 0 AP242 (the reference's default), 1 AP214, 2 AP203.
    pub step_ap: usize,
    /// The slicer the Slicer row opens (a name from `slicers::choices`).
    pub slicer: String,
    /// Quick Export: straight into this folder, no save dialog.
    pub quick: Option<std::path::PathBuf>,
    /// An "Open in …" row was clicked: export now, into that slicer.
    go: bool,
    scroll: f32,
    list_scroll: f32,
}

pub const STEP_APS: [&str; 3] = ["AP242", "AP214", "AP203"];

pub const STL_UNITS: [(&str, &str); 4] = [("Millimeter", "mm"), ("Centimeter", "cm"), ("Meter", "m"), ("Inch", "in")];

impl ExportDialog {
    pub fn new(name: &str, preset: Option<Format>) -> Self {
        ExportDialog {
            chosen: preset.into_iter().collect(),
            file_name: String::new(),
            project_name: name.to_string(),
            hidden: false,
            separately: false,
            high: true,
            colors: true,
            stl_ascii: false,
            stl_units: 0,
            step_ap: 0,
            slicer: super::slicers::pick(""),
            quick: None,
            go: false,
            scroll: 0.0,
            list_scroll: 0.0,
        }
    }

    /// The name files are saved under.
    pub fn name(&self) -> String {
        let n = self.file_name.trim();
        let n = if n.is_empty() { self.project_name.trim() } else { n };
        n.replace(['/', '\\', ':'], "-")
    }

    /// The engine parameters every chosen format shares (the path and items are added per file).
    pub fn params(&self, f: Format) -> serde_json::Value {
        let mut p = serde_json::json!({ "hidden": self.hidden });
        if matches!(f, Format::ThreeMf | Format::Slicer | Format::Stl | Format::Glb | Format::Obj | Format::Usdz) {
            p["resolution"] = serde_json::json!(if self.high { "high" } else { "low" });
            p["colors"] = serde_json::json!(self.colors);
        }
        if f == Format::Step {
            p["protocol"] = serde_json::json!(STEP_APS[self.step_ap.min(STEP_APS.len() - 1)]);
        }
        if f == Format::Stl {
            p["ascii"] = serde_json::json!(self.stl_ascii);
            p["units"] = serde_json::json!(STL_UNITS[self.stl_units.min(STL_UNITS.len() - 1)].1);
        }
        p
    }
}

const LIST_W: f32 = 240.0;
const ROW_H: f32 = 42.0;
const CARD_H: f32 = 40.0;
fn selected() -> Color32 {
    theme::c(0x4666f7)
}

fn card(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, radius(7), theme::c(0x212121));
    p.rect_stroke(r, radius(7), Stroke::new(1.0, theme::c(0x2a2a2a)), StrokeKind::Inside);
}

fn divider(p: &egui::Painter, r: Rect, y: f32) {
    p.line_segment([pos2(r.left() + 10.0, y), pos2(r.right() - 10.0, y)], Stroke::new(1.0, theme::c(0x2c2c2c)));
}

fn label(p: &egui::Painter, x: f32, y: f32, text: &str, sub: Option<&str>) {
    match sub {
        Some(s) => {
            p.text(pos2(x, y - 7.0), Align2::LEFT_CENTER, text, theme::body(13.5), Tokens::text_strong());
            p.text(pos2(x, y + 8.0), Align2::LEFT_CENTER, s, theme::body(11.5), Tokens::text_dim());
        }
        None => {
            p.text(pos2(x, y), Align2::LEFT_CENTER, text, theme::body(13.5), Tokens::text_strong());
        }
    }
}

/// An iOS-style switch, right-aligned at `right`.
fn switch(ui: &Ui, p: &egui::Painter, right: f32, y: f32, id: (&str, usize), on: &mut bool) {
    let r = Rect::from_min_size(pos2(right - 38.0, y - 10.0), vec2(38.0, 20.0));
    if ui.interact(r, ui.scope_id().with(id), Sense::click()).clicked() {
        *on = !*on;
    }
    p.rect_filled(r, radius(10), if *on { theme::c(0x2443c4) } else { theme::c(0x3a3a3c) });
    let knob = if *on { r.right() - 10.0 } else { r.left() + 10.0 };
    p.circle_filled(pos2(knob, y), 8.5, theme::c(0xd8d8d8));
}

/// A segmented control, right-aligned at `right`; returns the picked segment.
fn segmented(ui: &Ui, p: &egui::Painter, right: f32, y: f32, id: (&str, usize), items: &[&str], current: usize) -> Option<usize> {
    let w = 74.0;
    let r = Rect::from_min_size(pos2(right - w * items.len() as f32, y - 10.0), vec2(w * items.len() as f32, 20.0));
    p.rect_filled(r, radius(6), theme::c(0x2c2c2c));
    p.rect_stroke(r, radius(6), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let mut picked = None;
    for (k, item) in items.iter().enumerate() {
        let s = Rect::from_min_size(pos2(r.left() + w * k as f32, r.top()), vec2(w, 20.0));
        if k == current {
            p.rect_filled(s.shrink(1.0), radius(5), theme::c(0x5c5c5e));
        }
        p.text(s.center(), Align2::CENTER_CENTER, *item, theme::body(13.0), Tokens::text_strong());
        if ui.interact(s, ui.scope_id().with((id.0, id.1, k)), Sense::click()).clicked() {
            picked = Some(k);
        }
    }
    picked
}

fn badge(p: &egui::Painter, c: Pos2, text: &str) {
    let t = Rect::from_center_size(c, vec2(26.0, 26.0));
    p.rect_filled(t, radius(5), theme::c(0x87878b));
    p.text(t.center(), Align2::CENTER_CENTER, text, theme::strong(if text.len() > 3 { 7.0 } else { 8.0 }), Color32::WHITE);
}

/// The settings rows of one format, from `y` down; returns the y below them.
#[allow(clippy::too_many_arguments)]
fn settings(d: &mut ExportDialog, ui: &mut Ui, p: &egui::Painter, f: Format, k: usize, x0: f32, x1: f32, mut y: f32, paint: bool) -> f32 {
    let row = |y: &mut f32, h: f32| -> Rect {
        let r = Rect::from_min_size(pos2(x0, *y), vec2(x1 - x0, h));
        *y += h + 8.0;
        r
    };
    if f == Format::Slicer {
        // One action row per slicer, as the reference lists its targets ("Open in PrusaSlicer"): a click exports
        // and opens it there. The last one used sits first.
        let mut names = super::slicers::choices();
        if let Some(i) = names.iter().position(|n| *n == d.slicer) {
            let last = names.remove(i);
            names.insert(0, last);
        }
        let r = row(&mut y, CARD_H * names.len() as f32 + (names.len().saturating_sub(1)) as f32 * 2.0);
        if paint {
            card(p, r);
            for (i, name) in names.iter().enumerate() {
                let line = Rect::from_min_size(pos2(r.left(), r.top() + i as f32 * (CARD_H + 2.0)), vec2(r.width(), CARD_H));
                let resp = ui.interact(line, ui.scope_id().with(("exp-open-in", k, i)), Sense::click());
                if resp.hovered() {
                    p.rect_filled(line.shrink(2.0), radius(6), theme::c(0x2e2e30));
                }
                if i > 0 {
                    p.line_segment(
                        [pos2(line.left() + 10.0, line.top() - 1.0), pos2(line.right() - 10.0, line.top() - 1.0)],
                        Stroke::new(1.0, theme::c(0x2c2c2c)),
                    );
                }
                label(p, line.left() + 10.0, line.center().y, &format!("Open in {name}"), None);
                // An arrow out of a box: it leaves for another app.
                let c = pos2(line.right() - 18.0, line.center().y);
                let ink = Stroke::new(1.4, Tokens::text_strong());
                p.rect_stroke(Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(10.0, 10.0)), radius(2), ink, StrokeKind::Inside);
                p.line_segment([c + vec2(-1.0, 1.0), c + vec2(5.0, -5.0)], ink);
                p.line_segment([c + vec2(1.0, -5.0), c + vec2(5.0, -5.0)], ink);
                p.line_segment([c + vec2(5.0, -5.0), c + vec2(5.0, -1.0)], ink);
                if resp.clicked() {
                    d.slicer = (*name).to_string();
                    d.go = true;
                }
            }
        }
    }
    let mesh = matches!(f, Format::ThreeMf | Format::Slicer | Format::Stl | Format::Glb | Format::Obj | Format::Usdz);
    if mesh {
        let r = row(&mut y, CARD_H);
        if paint {
            card(p, r);
            label(p, r.left() + 10.0, r.center().y, "Resolution", None);
            if let Some(i) = segmented(ui, p, r.right() - 8.0, r.center().y, ("exp-res", k), &["Low", "High"], usize::from(d.high)) {
                d.high = i == 1;
            }
        }
        let (text, sub) = match f {
            Format::Glb => ("Include Visualization Materials", None),
            Format::Obj => ("Include Body Colors", Some("Writes a material file next to the mesh")),
            Format::Stl | Format::Usdz => ("", None),
            _ => ("Include Body Colors", Some("Bodies with an appearance keep its color")),
        };
        if !text.is_empty() {
            let r = row(&mut y, CARD_H);
            if paint {
                card(p, r);
                label(p, r.left() + 10.0, r.center().y, text, sub);
                switch(ui, p, r.right() - 10.0, r.center().y, ("exp-colors", k), &mut d.colors);
            }
        }
    }
    if f == Format::Step {
        let r = row(&mut y, CARD_H);
        if paint {
            card(p, r);
            label(p, r.left() + 10.0, r.center().y, "Output Format", None);
            let names: Vec<String> = STEP_APS.iter().map(|a| (*a).to_string()).collect();
            if let Some(i) = super::popover::pill_select(
                ui,
                &format!("exp-step-ap{k}"),
                pos2(r.right() - 10.0, r.center().y),
                70.0,
                &names,
                Some(d.step_ap),
            ) {
                d.step_ap = i;
            }
        }
    }
    if f == Format::Stl {
        let r = row(&mut y, CARD_H * 2.0 + 18.0);
        if paint {
            card(p, r);
            let y1 = r.top() + CARD_H / 2.0;
            label(p, r.left() + 10.0, y1, "Output Format", None);
            let names = vec!["Binary".to_string(), "ASCII".to_string()];
            if let Some(i) = super::popover::pill_select(
                ui,
                &format!("exp-stl-fmt{k}"),
                pos2(r.right() - 10.0, y1),
                70.0,
                &names,
                Some(usize::from(d.stl_ascii)),
            ) {
                d.stl_ascii = i == 1;
            }
            p.line_segment(
                [pos2(r.left() + 10.0, r.top() + CARD_H), pos2(r.right() - 10.0, r.top() + CARD_H)],
                Stroke::new(1.0, theme::c(0x2c2c2c)),
            );
            let y2 = r.top() + CARD_H + (CARD_H + 18.0) / 2.0;
            p.text(pos2(r.left() + 10.0, y2 - 14.0), Align2::LEFT_CENTER, "Units", theme::body(13.5), Tokens::text_strong());
            p.text(
                pos2(r.left() + 10.0, y2 + 2.0),
                Align2::LEFT_CENTER,
                "STL files carry plain numbers without a unit;",
                theme::body(11.5),
                Tokens::text_dim(),
            );
            p.text(
                pos2(r.left() + 10.0, y2 + 15.0),
                Align2::LEFT_CENTER,
                "set the same unit in your slicer.",
                theme::body(11.5),
                Tokens::text_dim(),
            );
            let units: Vec<String> = STL_UNITS.iter().map(|u| u.0.to_string()).collect();
            if let Some(i) =
                super::popover::pill_select(ui, &format!("exp-stl-units{k}"), pos2(r.right() - 10.0, y2), 90.0, &units, Some(d.stl_units))
            {
                d.stl_units = i;
            }
        }
    }
    y
}

pub(super) fn dialog(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(mut d) = ws.export.take() else { return };
    let r = Rect::from_center_size(full.center(), vec2(800.0, 648.0));
    let p = ui.painter().clone();
    p.rect_filled(full, 0.0, Color32::from_black_alpha(90));
    p.rect_filled(r.translate(vec2(0.0, 6.0)).expand(2.0), radius(12), Color32::from_black_alpha(90));
    p.rect_filled(r, radius(10), theme::c(0x1e1e1e));
    let list = Rect::from_min_size(r.min, vec2(LIST_W, r.height()));
    p.rect_filled(list, egui::CornerRadius { nw: 10, sw: 10, ne: 0, se: 0 }, theme::c(0x252525));
    p.line_segment([pos2(list.right(), r.top()), pos2(list.right(), r.bottom())], Stroke::new(1.0, Color32::BLACK));
    p.rect_stroke(r, radius(10), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);

    // The format list.
    let pill = Rect::from_center_size(pos2(list.center().x, r.top() + 21.0), vec2(108.0, 20.0));
    p.rect_filled(pill, radius(5), theme::c(0x3a3a3c));
    p.text(pill.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, "All Formats", theme::body(13.0), Tokens::text_strong());
    let chev = Rect::from_center_size(pill.right_center() - vec2(12.0, 0.0), vec2(14.0, 16.0));
    p.rect_filled(chev, radius(4), selected());
    p.text(chev.center(), Align2::CENTER_CENTER, "⌃", theme::body(8.0), Color32::WHITE);
    p.line_segment([pos2(list.left(), r.top() + 42.0), pos2(list.right(), r.top() + 42.0)], Stroke::new(1.0, theme::c(0x1a1a1a)));
    let cmd = ui.input(|i| i.modifiers.command);
    // The list scrolls with the wheel when it's taller than the dialog.
    let list_area = Rect::from_min_max(pos2(list.left(), r.top() + 43.0), list.max);
    let list_h = 62.0 + 2.0 * 32.0 + 14.0 + FORMATS.len() as f32 * ROW_H + 10.0;
    let max_scroll = (list_h - r.height()).max(0.0);
    if ui.input(|i| i.pointer.hover_pos()).is_some_and(|q| list_area.contains(q)) {
        d.list_scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    d.list_scroll = d.list_scroll.clamp(0.0, max_scroll);
    let full_p = p.clone();
    let p = p.with_clip_rect(list_area);
    let mut y = r.top() + 62.0 - d.list_scroll;
    for (i, (f, name, tag, ..)) in FORMATS.iter().enumerate() {
        if i == RECOMMENDED {
            y += 14.0;
        }
        if i == 0 || i == RECOMMENDED {
            p.text(
                pos2(list.left() + 17.0, y + 7.0),
                Align2::LEFT_CENTER,
                if i == 0 { "Recommended Formats" } else { "Everything Else" },
                theme::strong(11.0),
                theme::c(0x5a5a5a),
            );
            y += 32.0;
        }
        let row = Rect::from_min_size(pos2(list.left() + 10.0, y), vec2(LIST_W - 20.0, ROW_H));
        let resp = ui.interact(row.intersect(list_area), ui.scope_id().with(("exp-row", i)), Sense::click());
        let on = d.chosen.contains(f);
        if on {
            // Neighbouring picked rows join into one block (s12-108).
            let joined = |j: usize| j != RECOMMENDED && FORMATS.get(j).is_some_and(|g| d.chosen.contains(&g.0));
            let (up, down) = (i > 0 && i != RECOMMENDED && joined(i - 1), joined(i + 1));
            let (t, b) = (if up { 0 } else { 6 }, if down { 0 } else { 6 });
            p.rect_filled(row, egui::CornerRadius { nw: t, ne: t, sw: b, se: b }, selected());
        } else if resp.hovered() {
            p.rect_filled(row, radius(6), theme::c(0x2e2e30));
        }
        let dot = pos2(row.left() + 15.0, row.center().y);
        if on {
            p.circle_filled(dot, 6.5, Color32::WHITE);
            p.add(Shape::line(vec![dot + vec2(-3.0, 0.0), dot + vec2(-1.0, 2.5), dot + vec2(3.2, -2.5)], Stroke::new(1.6, selected())));
        } else {
            p.circle_stroke(dot, 6.0, Stroke::new(1.0, theme::c(0x6a6a6a)));
        }
        badge(&p, pos2(row.left() + 47.0, row.center().y), tag);
        p.text(pos2(row.left() + 67.0, row.center().y), Align2::LEFT_CENTER, *name, theme::body(14.0), Color32::WHITE);
        if resp.clicked() {
            // A click picks just this one; ⌘-click or the round button adds or removes it.
            let toggle = cmd || resp.interact_pointer_pos().is_some_and(|q| q.x < row.left() + 28.0);
            if toggle {
                if on {
                    d.chosen.retain(|c| c != f);
                } else {
                    d.chosen.push(*f);
                }
            } else {
                d.chosen = vec![*f];
            }
            d.scroll = 0.0;
        }
        y += ROW_H;
    }
    let p = full_p;

    // The chosen formats' settings, scrolled with the wheel when they don't fit.
    let right = Rect::from_min_max(pos2(list.right() + 1.0, r.top()), r.max);
    let (x0, x1) = (right.left() + 18.0, right.right() - 18.0);
    let footer_top = r.bottom() - 230.0;
    let pane = Rect::from_min_max(right.min, pos2(right.right(), footer_top));
    if d.chosen.is_empty() {
        p.text(
            pane.center(),
            Align2::CENTER_CENTER,
            "Choose one or more file formats to export…",
            theme::body(14.0),
            Tokens::text_strong(),
        );
    } else {
        let order: Vec<Format> = FORMATS.iter().map(|f| f.0).filter(|f| d.chosen.contains(f)).collect();
        let many = order.len() > 1;
        // Measure, then paint clipped at the scroll offset.
        let mut h = if many { 50.0 } else { 0.0 };
        for (k, f) in order.iter().enumerate() {
            h += if many { 34.0 } else { 62.0 };
            h = settings(&mut d, ui, &p, *f, k, x0, x1, h, false) + 10.0;
        }
        let max = (h - pane.height()).max(0.0);
        if ui.rect_contains_pointer(pane) {
            d.scroll = (d.scroll - ui.input(|i| i.smooth_scroll_delta.y)).clamp(0.0, max);
        }
        d.scroll = d.scroll.min(max);
        let clip = p.with_clip_rect(pane);
        let mut y = pane.top() - d.scroll;
        if many {
            clip.text(
                pos2(right.center().x, y + 26.0),
                Align2::CENTER_CENTER,
                format!("{} Formats Selected", order.len()),
                theme::strong(15.0),
                Tokens::text_strong(),
            );
            y += 50.0;
        }
        for (k, f) in order.iter().enumerate() {
            let (_, _, title, desc) = f.info();
            if many {
                clip.text(pos2(x0 + 10.0, y + 14.0), Align2::LEFT_CENTER, title, theme::strong(13.5), Tokens::text_strong());
                y += 34.0;
            } else {
                clip.text(pos2(x0 + 10.0, y + 25.0), Align2::LEFT_CENTER, title, theme::strong(15.0), Tokens::text_strong());
                clip.text(pos2(x0 + 10.0, y + 45.0), Align2::LEFT_CENTER, desc, theme::body(13.0), theme::c(0xbababa));
                y += 4.0;
                y += 62.0;
            }
            // Rows scrolled out of the pane don't take clicks.
            let mut sub = ui.new_child(egui::UiBuilder::new().max_rect(pane));
            sub.set_clip_rect(pane);
            y = settings(&mut d, &mut sub, &clip, *f, k, x0, x1, y, true) + 10.0;
        }
    }

    // File name and the shared switches.
    divider(&p, right, footer_top);
    let name_row = Rect::from_min_size(pos2(x0, footer_top + 19.0), vec2(x1 - x0, CARD_H));
    card(&p, name_row);
    label(&p, name_row.left() + 10.0, name_row.center().y, "File Name", None);
    let field = Rect::from_min_max(
        pos2(name_row.right() - 212.0, name_row.center().y - 9.0),
        pos2(name_row.right() - 10.0, name_row.center().y + 9.0),
    );
    p.rect_filled(field, radius(4), theme::c(0x2a2a2c));
    p.rect_stroke(field, radius(4), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let _ = ui.put(
        field.shrink2(vec2(6.0, 1.0)),
        egui::TextEdit::singleline(&mut d.file_name)
            .hint_text(d.project_name.clone())
            .frame(egui::Frame::NONE)
            .font(theme::body(13.0))
            .text_color(Tokens::text_strong()),
    );
    let toggles = Rect::from_min_size(pos2(x0, name_row.bottom() + 6.0), vec2(x1 - x0, CARD_H * 2.0 + 2.0));
    card(&p, toggles);
    let ya = toggles.top() + CARD_H / 2.0;
    label(&p, toggles.left() + 10.0, ya, "Include Hidden Items", None);
    switch(ui, &p, toggles.right() - 10.0, ya, ("exp-hidden", 0), &mut d.hidden);
    p.line_segment(
        [pos2(toggles.left() + 10.0, toggles.top() + CARD_H + 1.0), pos2(toggles.right() - 10.0, toggles.top() + CARD_H + 1.0)],
        Stroke::new(1.0, theme::c(0x2c2c2c)),
    );
    let yb = toggles.top() + CARD_H * 1.5 + 2.0;
    label(&p, toggles.left() + 10.0, yb, "Save Each 1st-Level Item Separately", Some("Files are named after the items"));
    switch(ui, &p, toggles.right() - 10.0, yb, ("exp-separate", 0), &mut d.separately);
    let bar = r.bottom() - 60.0;
    divider(&p, right, bar);
    let by = r.bottom() - 32.0;
    let n = d.chosen.len();
    let export_label = if n > 1 { format!("Export ({n})") } else { "Export".to_string() };
    let ew = if n > 1 { 82.0 } else { 62.0 };
    let export = Rect::from_min_size(pos2(x1 - ew, by - 10.0), vec2(ew, 21.0));
    let cancel = Rect::from_min_size(pos2(export.left() - 8.0 - 62.0, by - 10.0), vec2(62.0, 21.0));
    let cr = ui.interact(cancel, ui.scope_id().with("exp-cancel"), Sense::click());
    p.rect_filled(cancel, radius(5), if cr.hovered() { theme::c(0x6a6a6a) } else { theme::c(0x565656) });
    p.text(cancel.center(), Align2::CENTER_CENTER, "Cancel", theme::body(13.0), Color32::WHITE);
    let er = ui.interact(export, ui.scope_id().with("exp-go"), Sense::click());
    let can = n > 0 && !d.name().is_empty();
    p.rect_filled(
        export,
        radius(5),
        if !can {
            theme::c(0x3a3a3c)
        } else if er.hovered() {
            theme::c(0x3a62e8)
        } else {
            theme::c(0x2f55d4)
        },
    );
    p.text(export.center(), Align2::CENTER_CENTER, export_label, theme::body(13.0), if can { Color32::WHITE } else { Tokens::text_dim() });

    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if cr.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return;
    }
    let go = std::mem::take(&mut d.go);
    if can && (er.clicked() || enter || go) {
        ws.wants_export = Some(d);
        return;
    }
    ws.export = Some(d);
}
