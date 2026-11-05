//! The right column's popovers (display, snapping, units, view) and Constraint Settings.

use egui::{Pos2, Rect, Ui, pos2, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::workspace::{RightPop, Workspace};

/// The right column's popovers, with what each row does.
pub(super) fn right_popover(ws: &mut Workspace, ui: &mut Ui, pop: RightPop, at: Pos2, opened: f64) {
    use crate::workspace::popover::Row;
    use shapecraft_render::DisplayMode as D;
    let bounds = ui.ctx().content_rect();
    let top_right = pos2(at.x - 10.0, at.y + 21.0);
    let (clicked, rect) = match pop {
        RightPop::Display => {
            let d = ws.display;
            let rows = [
                Row::Header("Shaders"),
                Row::Choice { label: "Wireframe", checked: d == D::Wireframe && !ws.visualizing, enabled: true },
                Row::Choice { label: "X-Ray", checked: d == D::XRay && !ws.visualizing, enabled: true },
                Row::Choice { label: "Shaded", checked: d == D::Shaded && !ws.visualizing, enabled: true },
                Row::Choice { label: "Visualized", checked: ws.visualizing || d == D::Visualized, enabled: true },
                Row::Separator,
                Row::Header("Surface Analysis"),
                Row::Choice { label: "Zebra", checked: matches!(d, D::Zebra { .. }) && !ws.visualizing, enabled: true },
                Row::Choice { label: "Curvature Map", checked: matches!(d, D::Curvature { .. }) && !ws.visualizing, enabled: true },
                Row::Separator,
            ];
            // The last section follows the shader, as observed (L14): Zebra's direction and scale, the curvature
            // scale, or the edge and decal switches.
            let scale = std::cell::Cell::new(match d {
                D::Zebra { scale, .. } | D::Curvature { scale } => scale as f64 / 100.0,
                D::XRay => ws.xray_opacity as f64,
                _ => 1.0,
            });
            let mut rows = rows.to_vec();
            match d {
                D::Zebra { vertical, .. } if !ws.visualizing => rows.extend([
                    Row::Header("Zebra Analysis"),
                    Row::Select { label: "Direction", options: &["Horizontal", "Vertical"], current: usize::from(vertical) },
                    Row::Scale { label: "Scale", value: &scale, min: 0.2, max: 2.0 },
                ]),
                D::Curvature { .. } if !ws.visualizing => {
                    rows.extend([Row::Header("Curvature Map Analysis"), Row::Scale { label: "Scale", value: &scale, min: 0.2, max: 2.0 }])
                }
                _ => {
                    rows.extend([
                        Row::Header("Options"),
                        Row::Switch { label: "Show Edges", on: ws.view_opts.edges },
                        Row::Switch { label: "Show Hidden Edges", on: ws.view_opts.hidden_edges },
                        Row::Switch { label: "Show Decals", on: ws.view_opts.decals },
                    ]);
                    // X-Ray: how see-through the bodies are.
                    if d == D::XRay && !ws.visualizing {
                        rows.extend([
                            Row::Separator,
                            Row::Header("X-Ray"),
                            Row::Scale { label: "Opacity", value: &scale, min: 0.05, max: 0.9 },
                        ]);
                    }
                }
            }
            let (c, r) = crate::workspace::popover::show(ui, "pop-display", top_right, bounds, &rows);
            let pct = (scale.get() * 100.0).round().clamp(10.0, 500.0) as u16;
            let analysis = matches!(d, D::Zebra { .. } | D::Curvature { .. }) && !ws.visualizing;
            match (c, analysis) {
                (Some((1, _)), _) => (ws.display, ws.visualizing) = (D::Wireframe, false),
                (Some((2, _)), _) => (ws.display, ws.visualizing) = (D::XRay, false),
                (Some((3, _)), _) => (ws.display, ws.visualizing) = (D::Shaded, false),
                (Some((4, _)), _) if !ws.visualizing => {
                    if d != D::Visualized {
                        ws.toggle_visualized();
                    }
                }
                (Some((7, _)), _) => (ws.display, ws.visualizing) = (D::Zebra { vertical: false, scale: 100 }, false),
                (Some((8, _)), _) => (ws.display, ws.visualizing) = (D::Curvature { scale: 100 }, false),
                (Some((11, k)), true) if matches!(d, D::Zebra { .. }) => {
                    if let D::Zebra { scale, .. } = d {
                        ws.display = D::Zebra { vertical: k == 1, scale };
                    }
                }
                (Some((_, _)), true) => {
                    ws.display = match d {
                        D::Zebra { vertical, .. } => D::Zebra { vertical, scale: pct },
                        _ => D::Curvature { scale: pct },
                    };
                }
                (Some((16, _)), false) if d == D::XRay => ws.xray_opacity = scale.get().clamp(0.05, 0.9) as f32,
                (Some((11, _)), false) => ws.view_opts.edges = !ws.view_opts.edges,
                (Some((12, _)), false) => ws.view_opts.hidden_edges = !ws.view_opts.hidden_edges,
                (Some((13, _)), false) => ws.view_opts.decals = !ws.view_opts.decals,
                _ => {}
            }
            (c, r)
        }
        RightPop::Snap => {
            let o = ws.snap_opts;
            let rows = [
                Row::Header("Snap to"),
                Row::Switch { label: "Grid", on: o.grid },
                Row::Switch { label: "Sketch Guide Lines", on: o.guide_lines },
                Row::Switch { label: "Sketch Guide Points", on: o.guide_points },
                Row::Switch { label: "3D Guide Points", on: o.points_3d },
                Row::Switch { label: "Distant Edges", on: o.distant_edges },
                Row::Separator,
                Row::Header("Show"),
                Row::Switch { label: "Guide Points", on: o.show_guide_points },
                Row::Switch { label: "Snapping Hints", on: o.hints },
            ];
            let (c, r) = crate::workspace::popover::show(ui, "pop-snap", top_right, bounds, &rows);
            let s = &mut ws.snap_opts;
            match c.map(|c| c.0) {
                Some(1) => s.grid = !s.grid,
                Some(2) => s.guide_lines = !s.guide_lines,
                Some(3) => s.guide_points = !s.guide_points,
                Some(4) => s.points_3d = !s.points_3d,
                Some(5) => s.distant_edges = !s.distant_edges,
                Some(8) => s.show_guide_points = !s.show_guide_points,
                Some(9) => s.hints = !s.hints,
                _ => {}
            }
            (c, r)
        }
        RightPop::Units => {
            use shapecraft_doc::Units as U;
            let u = ws.session.doc.units;
            let step = shapecraft_render::grid_step(ws.camera.world_per_pixel(ws.viewport_rect.height() as f64));
            let lock = format!("Lock Grid Size ({})", shapecraft_doc::format_length(ws.grid_locked.unwrap_or(step), u));
            let rows = [
                Row::Header("Units"),
                Row::Choice { label: "Millimeter", checked: u == U::Millimeter, enabled: true },
                Row::Choice { label: "Centimeter", checked: u == U::Centimeter, enabled: true },
                Row::Choice { label: "Meter", checked: u == U::Meter, enabled: true },
                Row::Choice { label: "Inch", checked: u == U::Inch, enabled: true },
                Row::Choice { label: "Foot", checked: u == U::Foot, enabled: true },
                Row::Separator,
                Row::Header("Grid"),
                Row::Switch { label: &lock, on: ws.grid_locked.is_some() },
                Row::Tip("Tip: the grid size follows the zoom level"),
                Row::Separator,
                Row::Header("Degree Format"),
                Row::Choice { label: "Decimal (e.g. 27.25°)", checked: !ws.degrees_fractional, enabled: true },
                Row::Choice { label: "Fractional (e.g. 27°15')", checked: ws.degrees_fractional, enabled: true },
                Row::Separator,
                Row::Header("Foot & Inch Format"),
                Row::Choice { label: "Decimal (e.g. 1.375')", checked: !ws.feet_architectural, enabled: true },
                Row::Choice { label: "Architectural (e.g. 1' - 4 1/2\")", checked: ws.feet_architectural, enabled: true },
            ];
            let (c, r) = crate::workspace::popover::show(ui, "pop-units", top_right, bounds, &rows);
            let unit = |i: usize| ["Millimeter", "Centimeter", "Meter", "Inch", "Foot"].get(i.wrapping_sub(1)).copied();
            match c.map(|c| c.0) {
                Some(i @ 1..=5) => {
                    if let Some(name) = unit(i) {
                        let _ = ws.run("set_units", json!({ "units": name }));
                    }
                }
                Some(8) => ws.grid_locked = if ws.grid_locked.is_some() { None } else { Some(step) },
                Some(12) => ws.degrees_fractional = false,
                Some(13) => ws.degrees_fractional = true,
                Some(16) => ws.feet_architectural = false,
                Some(17) => ws.feet_architectural = true,
                _ => {}
            }
            (c, r)
        }
        RightPop::View if ws.view_tab == 1 => {
            let fov = std::cell::Cell::new(ws.settings.field_of_view);
            let rows = [
                Row::Segments(&["Views", "Appearance"], 1),
                Row::Switch { label: "Animate Camera", on: ws.settings.animate_camera },
                Row::Slider { left: "Orthographic", right: "Perspective", value: &fov, min: 0.0, max: 90.0 },
                Row::Tip("Tip: 0° gives an orthographic view"),
                Row::Separator,
                Row::Switch { label: "Show Grid", on: ws.view_opts.grid },
                Row::Switch { label: "Show World Axes", on: ws.view_opts.axes },
            ];
            let (c, r) = crate::workspace::popover::show(ui, "pop-view", top_right, bounds, &rows);
            match c {
                Some((0, 0)) => ws.view_tab = 0,
                Some((1, _)) => ws.settings.animate_camera = !ws.settings.animate_camera,
                Some((2, _)) => {
                    ws.settings.field_of_view = fov.get();
                    let s = ws.settings.clone();
                    s.apply_view(&mut ws.camera);
                }
                Some((5, _)) => ws.view_opts.grid = !ws.view_opts.grid,
                Some((6, _)) => ws.view_opts.axes = !ws.view_opts.axes,
                _ => {}
            }
            (c, r)
        }
        RightPop::View => {
            use shapecraft_render::StandardView as V;
            let cur = ws.current_view();
            let view = |label, v: V| Row::Choice { label, checked: cur == Some(v), enabled: true };
            const SLOTS: [&str; 8] = ["View 1", "View 2", "View 3", "View 4", "View 5", "View 6", "View 7", "View 8"];
            let saved = ws.session.doc.saved_views;
            let slot = |k: usize| Row::Saved { label: SLOTS[k], saved: saved[k].is_some() };
            let rows = [
                Row::Segments(&["Views", "Appearance"], 0),
                Row::Button("Default View"),
                Row::Tip("Tip: double-click the view cube to reset quickly"),
                Row::Separator,
                view("Front", V::Front),
                view("Back", V::Back),
                view("Top", V::Top),
                view("Bottom", V::Bottom),
                view("Right", V::Right),
                view("Left", V::Left),
                Row::Choice { label: "Nearest Ortho View", checked: false, enabled: true },
                Row::Separator,
                Row::Header("Saved Views"),
                slot(0),
                slot(1),
                slot(2),
                slot(3),
                slot(4),
                slot(5),
                slot(6),
                slot(7),
                Row::Tip("Saves the camera view with the project"),
                Row::Separator,
                Row::Header("Grid Position"),
                Row::Choice { label: "XY Plane", checked: ws.view_opts.grid && ws.grid_plane == 0, enabled: true },
                Row::Choice { label: "YZ Plane", checked: ws.view_opts.grid && ws.grid_plane == 1, enabled: true },
                Row::Choice { label: "ZX Plane", checked: ws.view_opts.grid && ws.grid_plane == 2, enabled: true },
            ];
            let (c, r) = crate::workspace::popover::show(ui, "pop-view", top_right, bounds, &rows);
            match c {
                Some((0, 1)) => ws.view_tab = 1,
                Some((1, _)) => ws.standard_view(V::Default),
                Some((4, _)) => ws.standard_view(V::Front),
                Some((5, _)) => ws.standard_view(V::Back),
                Some((6, _)) => ws.standard_view(V::Top),
                Some((7, _)) => ws.standard_view(V::Bottom),
                Some((8, _)) => ws.standard_view(V::Right),
                Some((9, _)) => ws.standard_view(V::Left),
                Some((10, _)) => ws.nearest_ortho_view(),
                Some((i @ 13..=20, 1)) => ws.save_view(i - 12),
                Some((i @ 13..=20, _)) => ws.go_to_saved_view(i - 12),
                Some((i @ 24..=26, _)) => {
                    ws.grid_plane = i - 24;
                    ws.view_opts.grid = true;
                }
                _ => {}
            }
            (c, r)
        }
    };
    // Shader choices close the popover; the surface analyses (they bring their own options, as observed), switches,
    // tabs and sliders keep it open.
    let choice_closes = match pop {
        RightPop::Display => clicked.is_some_and(|c| (1..=4).contains(&c.0)),
        RightPop::View => ws.view_tab == 0 && clicked.is_some_and(|c| c.0 != 0),
        _ => false,
    };
    let outside = ws.now - opened > 0.15
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|q| {
                    !rect.contains(q) && !Rect::from_min_size(at, vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH)).contains(q)
                })
        });
    if choice_closes || outside {
        ws.right_pop = None;
    }
}

/// Constraint Settings, as observed (L19): auto-constraining, what shows, and which element stays put.
pub(super) fn constraint_settings(ws: &mut Workspace, ui: &mut Ui, at: Pos2, opened: f64) {
    use crate::workspace::popover::Row;
    let st = &ws.settings;
    let rows = [
        Row::Switch { label: "Auto-constraining", on: st.auto_constrain },
        Row::Separator,
        Row::Header("Constraint & Locked Dimension Visibility"),
        Row::Switch { label: "Always Show Constraints", on: st.always_show_constraints },
        Row::Switch { label: "Always Show Dimensions", on: st.always_show_dimensions },
        Row::Paragraph("Otherwise the constraints of what's selected show.", 1),
        Row::Separator,
        Row::Header("Anchored Sketch Entity"),
        Row::Choice { label: "First Selected", checked: !st.anchor_last, enabled: true },
        Row::Choice { label: "Last Selected", checked: st.anchor_last, enabled: true },
        Row::Paragraph("Constraining several elements moves the others to meet this one (constraints already there win).", 2),
    ];
    let bounds = ui.ctx().content_rect();
    let (clicked, rect) = crate::workspace::popover::show(ui, "constraint-settings", pos2(at.x - 10.0, at.y + 21.0), bounds, &rows);
    let st = &mut ws.settings;
    match clicked.map(|c| c.0) {
        Some(0) => st.auto_constrain = !st.auto_constrain,
        Some(3) => st.always_show_constraints = !st.always_show_constraints,
        Some(4) => st.always_show_dimensions = !st.always_show_dimensions,
        Some(8) => st.anchor_last = false,
        Some(9) => st.anchor_last = true,
        _ => {}
    }
    let button = Rect::from_min_size(at, vec2(Tokens::TOOL_BUTTON, Tokens::TOOL_PITCH + 2.0));
    if ws.now - opened > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !rect.contains(q) && !button.contains(q)))
    {
        ws.constraint_pop = None;
    }
}
