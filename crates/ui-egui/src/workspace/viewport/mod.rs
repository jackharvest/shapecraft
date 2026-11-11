//! The 3D viewport: our renderer's texture, navigation, picking, and what tools draw on top.
//!
//! `Workspace::viewport` here draws the view each frame and hands input to the submodules:
//! - `gpu`: the renderer and its textures, screenshots and captures.
//! - `scene`: what the renderer draws besides the model (overlays, tinted bodies).
//! - `navigate`: orbit, pan and zoom, and what the pointer hovers.
//! - `clicks`: clicks, double-clicks and sketch drawing drags.
//! - `gizmo`: the Move, Scale and Rotate Around Axis gizmos.
//! - `handles`: the other tools' drag handles (arrows, draft, section, pattern, edge plane).
//! - `in_place`: Text and Sketch Fillet, which work in the view, and the bottom Cancel / Done buttons.
//! - `annotations`: labels, handles and value boxes drawn in screen space over the view.
//! - `dimensions`: sketch dimensions: which there are, where they sit, and applying typed ones.
//! - `paint`: small painter helpers (dimension lines, pills, value boxes, arrows).
//! - `snap`: where the pointer lands on a sketch plane or the model, with snapping.
//! - `picking`: what is under the pointer: sketch points, plane tiles, model picks.

mod annotations;
mod clicks;
mod dimensions;
mod gizmo;
mod gpu;
mod handles;
mod in_place;
mod navigate;
mod paint;
mod picking;
mod scene;
mod snap;

use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, pos2, vec2};
use glam::{DVec2, DVec3};
use shapecraft_doc::{Pick, SketchPlane};
use shapecraft_render::{PlaneTile, Rgba, SceneColors, SceneInput};

use self::snap::to_d;
use super::Workspace;
use super::tools::Tool;
use crate::theme::{self, Tokens, radius};

pub(in crate::workspace) use dimensions::{DimEdit, DimKind, format_degrees};
pub use gpu::ViewportGpu;
pub(crate) use gpu::unmix;
pub(in crate::workspace) use paint::PILL_INK;

impl Workspace {
    fn local(&self, p: Pos2) -> DVec2 {
        to_d(p) - to_d(self.view_rect.min)
    }

    pub(in crate::workspace) fn size(&self) -> DVec2 {
        DVec2::new(self.view_rect.width() as f64, self.view_rect.height() as f64)
    }

    /// World point → screen position (points).
    pub fn to_screen(&self, p: DVec3) -> Option<Pos2> {
        self.camera.project(p, self.size()).map(|(s, _)| self.view_rect.min + vec2(s.x as f32, s.y as f32))
    }

    /// Draws the 3D view and handles input over it.
    pub(in crate::workspace) fn viewport(&mut self, ui: &mut Ui, render_state: Option<&egui_wgpu::RenderState>) {
        self.confirm_seen = self.confirm.take();
        self.seen_bottom.clear();
        if self.confirm_seen.is_none() {
            self.confirm_click = None;
        }
        if self.drawings.is_some() {
            super::drawings::sheet_view(self, ui);
            return;
        }
        let rect = self.view_rect;
        let resp = ui.interact(rect, ui.scope_id().with("viewport"), Sense::click_and_drag());
        let pointer = ui.input(|i| i.pointer.hover_pos());
        self.cursor = pointer.filter(|p| rect.contains(*p) && resp.hovered() || resp.dragged());
        self.viewport_hot = resp.hovered() || resp.dragged();
        if self.finger_nav {
            // A finger on a touch screen only moves the view, as on iPad: no picking, no hover, no
            // handles (the pen does those). A double tap zooms to the face under it.
            self.hover = None;
            if resp.double_clicked()
                && let Some(c) = self.cursor
            {
                self.zoom_to_face(c);
            }
            self.navigate(ui, &resp);
        } else {
            self.update_hover();
            self.handle_clicks(ui, &resp);
            self.handle_drags(ui, &resp);
            self.area_drags(ui, &resp);
            // After the handles: a drag that just grabbed one mustn't also turn the view on its first frame.
            self.navigate(ui, &resp);
        }

        let ppp = ui.ctx().pixels_per_point();
        self.ppp = ppp;
        let px = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
        let tiles: Vec<PlaneTile> = self.plane_tiles().into_iter().map(|(_, t)| t).collect();
        self.refresh_section_caps();
        let overlays = self.overlays();
        let grid = self
            .sketch_plane()
            // Choosing a plane: the hovered tile previews its grid. Otherwise an empty project shows none (s11-020),
            // while one that already has a sketch keeps the ground grid (s30-105).
            .or(match self.tool {
                Tool::PickPlane { hovered: Some(i) } => self.plane_tiles().get(i).map(|(p, _)| *p),
                Tool::PickPlane { hovered: None } if !self.session.doc.sketches.is_empty() || crate::tablet::touch_ui() => {
                    Some(SketchPlane::XY)
                }
                _ => None,
            })
            .or((self.view_opts.grid && !matches!(self.tool, Tool::PickPlane { .. })).then(|| {
                [SketchPlane::XY, SketchPlane::YZ, SketchPlane { origin: DVec3::ZERO, x: DVec3::Z, y: DVec3::X }][self.grid_plane.min(2)]
            }));
        let live = self.has_live_preview();
        let shown = self.shown_doc().into_owned();
        // Highlights refer to the real document's faces; drop them while showing a preview. Moving keeps whole
        // bodies (same ids), so they stay selected as they move, as observed.
        // A box being dragged shows what it would take.
        let boxed: Option<Vec<Pick>> = self.area.as_ref().map(|a| self.area_picks(a));
        let kept: Vec<Pick> = if let Some((body, face)) = self.growing_face() {
            // The face it grew from stays picked on the part as it was; the new end face isn't.
            vec![Pick::Face { body: Self::before_id(body), face }]
        } else if live && matches!(self.tool, Tool::Move(_) | Tool::Scale(_) | Tool::Pattern(_) | Tool::RotateAxis(_)) {
            self.session.selection.iter().filter(|p| matches!(p, Pick::Body { .. } | Pick::Plane { .. })).copied().collect()
        } else if self.side_drag.is_some() {
            // A side being pushed keeps its place in the sketch, so it stays picked.
            self.session.selection.clone()
        } else {
            Vec::new()
        };
        let ghosts = self.boolean_ghosts();
        let defined = self.defined_elements();
        let tints = self.split_tints();
        let scene = SceneInput {
            doc: &shown,
            selection: if let Some(b) = &boxed {
                b
            } else if live || self.growing_face().is_some() {
                &kept
            } else if self.offsetting_region() || matches!(self.tool, Tool::SketchFillet { .. }) {
                &[]
            } else {
                &self.session.selection
            },
            hover: if live || self.offsetting_region() || self.growing_face().is_some() {
                None
            } else {
                self.menu_hover.or(self.hover).or(self.items_hover)
            },
            active_sketch: self.session.active_sketch,
            plane_tiles: &tiles,
            overlays: &overlays,
            colors: SceneColors {
                xray: self.xray_opacity,
                ..if theme::is_light() { SceneColors::light() } else { SceneColors::default() }
            },
            show_axes: self.view_opts.axes,
            show_edges: self.view_opts.edges,
            show_hidden_edges: self.view_opts.hidden_edges,
            show_decals: self.view_opts.decals,
            grid,
            display: self.display,
            section: self.section.map(|s| (s.origin(), s.normal)),
            presentation: self.visualized_look(),
            backdrop: Rgba::hex(super::visual::backdrop(&self.session.doc.visual)),
            shadow: super::visual::shadow(&self.session.doc.visual),
            ghosts: &ghosts,
            tints: &tints,
            drawn: if self.session.selection == self.just_drawn { &self.just_drawn } else { &[] },
            defined: &defined,
        };
        let texture = self.gpu.draw(ui.ctx(), render_state, &scene, &self.camera, px);
        match texture {
            Some(id) => {
                ui.painter().image(id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(rect, 0.0, Tokens::viewport());
                if let Some(e) = &self.gpu.error {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, e, theme::body(13.0), Tokens::text_dim());
                }
            }
        }
        self.draw_annotations(ui);
        self.paint_area(ui);
        self.text_field(ui);
        // Moving sketch curves: a "Copy" pill under the gizmo (bodies have theirs already), s11-088.
        if let Tool::Move(t) = &self.tool
            && t.items.is_empty()
            && t.faces.is_none()
            && t.edge.is_none()
            && t.dragging.is_none()
            && let Some(o) = self.to_screen(t.shown_pivot())
        {
            let copy = t.copy;
            let r = Rect::from_center_size(o + vec2(0.0, 44.0), vec2(36.0, 17.0));
            let p = ui.painter();
            p.rect_filled(r, radius(8), if copy { Tokens::accent() } else { Color32::WHITE });
            p.text(r.center(), Align2::CENTER_CENTER, "Copy", theme::strong(10.0), if copy { Color32::WHITE } else { paint::PILL_INK });
            if ui.interact(r, ui.scope_id().with("sketch-move-copy"), Sense::click()).clicked()
                && let Tool::Move(t) = &mut self.tool
            {
                t.copy = !t.copy;
            }
        }
        self.sketch_fillet_handle(ui);
        // A tool whose pull applied live (Extrude, Chamfer/Fillet, Shell, Offset Face…): Cancel takes the step back,
        // Done keeps it (26.200, s11-101).
        let live = self.is_live() && !matches!(self.tool, Tool::Idle);
        // Booleans apply on Done (shown from the start, s11-112).
        // Union offers Done at once; Subtract and Intersect once they have what they need (s11-112/113); the
        // others once they'd change something.
        let boolean = match &self.tool {
            Tool::Boolean(t) => t.op == "union" || t.ready(),
            Tool::Pattern(t) => t.spacing.abs() > 1e-9,
            Tool::Translate { to, .. } => to.is_some(),
            Tool::Align { to, .. } => to.is_some(),
            Tool::Onto(_) => self.is_live(),
            Tool::CircularPattern(t) => t.about.is_some(),
            Tool::RotateAxis(t) => t.angle.abs() > 1e-9,
            Tool::BodyEdgeOffset(t) => t.distance.abs() > 1e-9,
            Tool::OffsetFace(t) => t.whole,
            _ => false,
        };
        if (live || self.picked_modify_tool()) && self.sketch_transform().is_none() && !self.offsetting_region() {
            match self.bottom_actions(ui, live || boolean) {
                Some("Cancel") => self.ui_action(crate::workspace::toolbar::UiAction::ToolCancel),
                // Nothing pulled yet: Done just ends it.
                Some(_) if !live && matches!(self.tool, Tool::OffsetFace(_)) => {
                    self.ui_action(crate::workspace::toolbar::UiAction::ToolCancel)
                }
                Some(_) if boolean && !live => self.commit_tool(),
                Some(_) => self.finish_live(),
                None => {}
            }
        }
        // Project in a sketch: Cancel at the bottom (s24 matrix).
        if matches!(self.tool, Tool::Project) && self.bottom_actions(ui, false).is_some() {
            self.tool = Tool::Idle;
        }
        // A tool waiting for its first pick: Cancel at the bottom (s24 matrix).
        // (Trim waiting shows none.)
        if matches!(self.tool, Tool::Await { kind, .. } if kind != crate::workspace::toolbar::ToolKind::Trim)
            && self.bottom_actions(ui, false).is_some()
        {
            self.tool = Tool::Idle;
        }
        // Done joins once it has changed something (s11-090).
        if let Some(changed) = self.sketch_transform() {
            match self.bottom_actions(ui, changed) {
                Some("Cancel") => self.escape(),
                // A move is already applied (live): Done ends it, as Return does.
                Some(_) if matches!(self.tool, Tool::Move(_)) && self.is_live() => self.finish_live(),
                Some(_) => self.commit_tool(),
                None => {}
            }
        }
        // Offset Edge on a region: Cancel, and Done once pulled (s11-075/077).
        if self.offsetting_region() {
            let pulled = matches!(&self.tool, Tool::OffsetEdge(t) if t.distance.abs() > 1e-9);
            match self.bottom_actions(ui, pulled) {
                Some("Cancel") => self.escape(),
                Some(_) => self.commit_tool(),
                None => {}
            }
        }
    }
}
