//! Camera navigation (orbit, pan, zoom) and keeping the hover up to date.

use egui::Ui;
use shapecraft_doc::Pick;

use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

impl Workspace {
    pub(super) fn navigate(&mut self, ui: &Ui, resp: &egui::Response) {
        let size = self.size();
        let (events, zoom, shift) = ui.input(|i| (i.events.clone(), i.zoom_delta(), i.modifiers.shift));
        if resp.hovered() {
            for ev in &events {
                if let egui::Event::MouseWheel { delta, source, unit, modifiers, .. } = ev {
                    if modifiers.command || modifiers.ctrl {
                        continue; // pinch / ctrl-scroll arrive as zoom_delta
                    }
                    let wheel = *source == egui::MouseWheelSource::Wheel || *unit != egui::MouseWheelUnit::Point;
                    if wheel {
                        let lines = if *unit == egui::MouseWheelUnit::Point { delta.y as f64 / 50.0 } else { delta.y as f64 };
                        let lines = if self.settings.reverse_scroll { lines } else { -lines };
                        if let Some(c) = self.cursor {
                            self.anim = None;
                            self.camera.zoom_at(0.88f64.powf(lines), self.local(c), size);
                        }
                    } else if modifiers.shift {
                        self.anim = None;
                        let (sx, sy) = self.settings.pan_signs();
                        self.camera.pan(-delta.x as f64 * sx, -delta.y as f64 * sy, size.y);
                    } else {
                        // Two-finger drag on a trackpad orbits.
                        self.anim = None;
                        let (sx, sy) = self.settings.orbit_signs();
                        self.camera.orbit(-delta.x as f64 * sx, delta.y as f64 * sy);
                    }
                }
            }
            if (zoom - 1.0).abs() > 1e-4
                && let Some(c) = self.cursor
            {
                self.anim = None;
                self.camera.zoom_at(1.0 / zoom as f64, self.local(c), size);
            }
        }
        // Mouse, as the chosen navigation preset maps it (the owner uses Blender's: middle-drag orbits, Shift pans,
        // Ctrl zooms). Left-drag on empty space orbits unless the preset uses it.
        let delta = resp.drag_delta();
        if delta != egui::Vec2::ZERO {
            use crate::settings::{Button, NavAction};
            let (ctrl, alt) = ui.input(|i| (i.modifiers.ctrl, i.modifiers.alt));
            let button = if resp.dragged_by(egui::PointerButton::Middle) {
                Some(Button::Middle)
            } else if resp.dragged_by(egui::PointerButton::Secondary) {
                Some(Button::Right)
            } else if resp.dragged_by(egui::PointerButton::Primary) {
                Some(Button::Left)
            } else {
                None
            };
            // Left-drag on nothing draws an Area Selection box where one applies, else it orbits.
            let primary_free = button == Some(Button::Left)
                && !self.dragging_handle()
                && (self.touch_drag || !self.tool.is_sketch_tool() && !self.area_open() && self.area.is_none());
            // The pen never moves the camera, as on iPad: fingers orbit, pan and zoom; the pen picks, draws and
            // (after a long press) drags an Area Selection.
            let action = if self.pen_input {
                None
            } else {
                button.and_then(|b| self.settings.preset().action(b, shift, ctrl, alt)).or(primary_free.then_some(NavAction::Orbit))
            };
            match action {
                Some(NavAction::Zoom) => {
                    if let Some(c) = self.cursor {
                        self.anim = None;
                        self.camera.zoom_at(1.01f64.powf(delta.y as f64), self.local(c), size);
                    }
                }
                Some(NavAction::Pan) => {
                    self.anim = None;
                    let (sx, sy) = self.settings.pan_signs();
                    self.camera.pan(delta.x as f64 * sx, delta.y as f64 * sy, size.y);
                }
                Some(NavAction::Orbit) => {
                    self.anim = None;
                    let (sx, sy) = self.settings.orbit_signs();
                    self.camera.orbit(delta.x as f64 * sx, delta.y as f64 * sy);
                }
                None => {}
            }
        }
    }

    /// A tool is running, so typed numbers go to it (the tablet offers its number keyboard).
    pub fn takes_numbers(&self) -> bool {
        !matches!(self.tool, Tool::Idle)
    }

    /// Two fingers moved the view (tablet): the model follows them.
    pub fn touch_pan(&mut self, delta: egui::Vec2) {
        if delta == egui::Vec2::ZERO || !delta.is_finite() {
            return;
        }
        self.anim = None;
        let (sx, sy) = self.settings.pan_signs();
        self.camera.pan(delta.x as f64 * sx, delta.y as f64 * sy, self.size().y);
    }

    /// Two fingers pinched (tablet): `factor` > 1 spreads them, closer in, around the point between them.
    pub fn touch_zoom(&mut self, factor: f32, at: egui::Pos2) {
        if !factor.is_finite() || (factor - 1.0).abs() < 1e-4 {
            return;
        }
        self.anim = None;
        let size = self.size();
        self.camera.zoom_at(1.0 / factor.clamp(0.5, 2.0) as f64, self.local(at), size);
    }

    /// The part of the canvas nothing floats over: where pills that have to be clicked may go.
    pub(super) fn reachable(&self) -> egui::Rect {
        let vp = self.viewport_rect;
        let left: f32 = if self.compact { 64.0 } else { 210.0 };
        egui::Rect::from_min_max(
            egui::pos2(vp.left() + left.min(vp.width() / 3.0), vp.top() + 70.0),
            egui::pos2(vp.right() - 16.0, vp.bottom() - 60.0),
        )
    }

    pub(in crate::workspace) fn dragging_handle(&self) -> bool {
        self.side_drag.is_some()
            || self.curve_drag.is_some()
            || self.text_drag.is_some()
            || self.fillet_drag
            || matches!(&self.tool, Tool::Text { at: Some(_), .. })
                && self.text_handle_points().zip(self.cursor).is_some_and(|(h, c)| h[1..].iter().any(|q| q.distance(c) < 14.0))
            || matches!(&self.tool, Tool::Move(t) if t.dragging.is_some() || t.relocating)
            || self.section.is_some_and(|s| s.dragging.is_some())
            || matches!(&self.tool, Tool::Pattern(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::Extrude(t) if t.dragging.is_some() || t.drafting.is_some())
            || matches!(&self.tool, Tool::Fillet(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::Shell(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::OffsetFace(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::Scale(t) if t.dragging.is_some() || t.relocating)
            || matches!(&self.tool, Tool::RotateAxis(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::EdgePlane(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::BodyEdgeOffset(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::OffsetEdge(t) if t.dragging.is_some())
            || matches!(&self.tool, Tool::OffsetPlane(t) if t.dragging.is_some())
    }

    pub(super) fn update_hover(&mut self) {
        let Some(c) = self.cursor.filter(|_| self.anim.is_none()) else {
            self.hover = None;
            return;
        };
        if self.section_pick.is_some() {
            let tile = self.tile_under(c);
            self.section_pick = Some(tile);
            self.hover = if tile.is_none() { self.pick_at(c).map(|(p, _)| p).filter(|p| matches!(p, Pick::Face { .. })) } else { None };
            return;
        }
        match &mut self.tool {
            Tool::PickPlane { .. } => {
                let tile = self.tile_under(c);
                if let Tool::PickPlane { hovered } = &mut self.tool {
                    *hovered = tile;
                }
                self.hover = if tile.is_none() {
                    self.pick_at(c).map(|(p, _)| p).filter(|p| matches!(p, Pick::Face { .. } | Pick::Plane { .. }))
                } else {
                    None
                };
            }
            Tool::Mirror(_) => {
                let tile = self.mirror_tile_under(c);
                if let Tool::Mirror(t) = &mut self.tool {
                    t.hovered = tile;
                }
                // In a sketch a straight line can be the mirror too (s11-091/092).
                let sketch = self.session.active_sketch;
                self.hover = if tile.is_none() {
                    let split = matches!(&self.tool, Tool::Mirror(t) if t.split);
                    self.pick_at(c).map(|(p, _)| p).filter(|p| {
                        matches!(p, Pick::Face { .. } | Pick::Plane { .. })
                            || matches!(p, Pick::SketchEntity { sketch: s, .. } if split || Some(*s) == sketch)
                    })
                } else {
                    None
                };
            }
            Tool::Trim { .. } | Tool::Extend { .. } => {
                let found = match self.pick_at(c) {
                    Some((Pick::SketchEntity { sketch, entity }, hit)) if Some(sketch) == self.session.active_sketch => {
                        self.sketch_plane().map(|pl| (entity, pl.to_local(hit)))
                    }
                    _ => None,
                };
                let extends = |e: u32| {
                    self.session
                        .active_sketch()
                        .and_then(|sk| sk.entities.get(e as usize))
                        .is_some_and(|e| matches!(e, shapecraft_doc::SketchEntity::Line { .. } | shapecraft_doc::SketchEntity::Arc { .. }))
                };
                let grows = found.filter(|(e, _)| extends(*e));
                match &mut self.tool {
                    Tool::Trim { target } => *target = found,
                    Tool::Extend { target, .. } => *target = grows,
                    _ => {}
                }
                self.hover = None;
            }
            t if t.is_sketch_tool() => self.hover = None,
            _ => self.hover = self.pick_at(c).map(|(p, _)| p),
        }
    }
}
