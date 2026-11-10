//! The tablet's touch model for a touch screen the app draws on itself (the Android app): winit's touches go through
//! the same `Router` as the page's, so a finger taps, long-presses, turns, pans and pinches as it does on the tablet,
//! and the screen is scaled so the interface fits, as the page scales it.

use std::time::Instant;

use egui::{Event, RawInput, TouchPhase};

use super::session::{FINGER_REACH, MIN_SIZE};
use super::touch::{Device, Kind, Nav, Router};
use crate::workspace::Workspace;

#[derive(Default)]
pub struct NativeTouch {
    router: Router,
}

impl NativeTouch {
    /// Before a frame: winit's touches become the router's clicks and drags (its own pointer events, made from the
    /// first finger, are dropped), and two-finger moves turn into view changes on the workspace.
    pub fn input(&mut self, raw: &mut RawInput, ws: Option<&mut Workspace>) {
        let now = Instant::now();
        let mut rest = Vec::with_capacity(raw.events.len());
        for ev in raw.events.drain(..) {
            match ev {
                Event::Touch { id, phase, pos, .. } => {
                    let kind = match phase {
                        TouchPhase::Start => Kind::Down,
                        TouchPhase::Move => Kind::Move,
                        TouchPhase::End => Kind::Up,
                        TouchPhase::Cancel => Kind::Cancel,
                    };
                    self.router.pointer(kind, Device::Touch, id.0 as i64, pos, 0, now);
                }
                Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone => {}
                other => rest.push(other),
            }
        }
        self.router.tick(now);
        let (events, nav) = self.router.take();
        rest.extend(events);
        raw.events = rest;
        if let Some(ws) = ws {
            ws.touch_drag = self.router.finger;
            ws.pick_slop = FINGER_REACH;
            for n in nav {
                match n {
                    Nav::Pan(d) => ws.touch_pan(d),
                    Nav::Zoom(f, at) => ws.touch_zoom(f, at),
                }
            }
        }
    }

    /// After a frame: tells the router what the finger was over, and asks for the frames a pending gesture needs.
    pub fn after_frame(&mut self, ctx: &egui::Context, over_view: bool) {
        self.router.after_frame(over_view, ctx.dragged_id().is_some());
        if self.router.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    /// The zoom that gives the interface at least `MIN_SIZE` points on this screen (1 on a big one), as the tablet
    /// page scales it: from the window's pixels and the system's own pixels per point.
    pub fn fit_zoom(ctx: &egui::Context) -> Option<f32> {
        let native = ctx.native_pixels_per_point().filter(|p| p.is_finite() && *p > 0.0)?;
        let natural = ctx.content_rect().size() * ctx.pixels_per_point() / native;
        if !(natural.x > 1.0 && natural.y > 1.0) {
            return None;
        }
        Some(fit(natural.x, natural.y))
    }
}

/// The zoom for a screen of this many natural points.
fn fit(w: f32, h: f32) -> f32 {
    1.0 / (MIN_SIZE[0] / w).max(MIN_SIZE[1] / h).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_screens_keep_their_size_and_phones_shrink_to_fit() {
        assert_eq!(fit(1280.0, 800.0), 1.0);
        // A phone held sideways: 640 points tall.
        assert!((411.0 / fit(914.0, 411.0) - 640.0).abs() < 1e-3);
        // Upright: 1000 points wide.
        assert!((411.0 / fit(411.0, 914.0) - 1000.0).abs() < 1e-3);
    }

    #[test]
    fn a_tap_is_a_click_and_winit_pointer_events_are_dropped() {
        let mut t = NativeTouch::default();
        let at = egui::pos2(100.0, 200.0);
        let touch = |phase| Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(7), phase, pos: at, force: None };
        let mut raw = RawInput { events: vec![touch(TouchPhase::Start), Event::PointerMoved(at)], ..Default::default() };
        t.input(&mut raw, None);
        let mut seen = raw.events.clone();
        let mut raw = RawInput { events: vec![touch(TouchPhase::End)], ..Default::default() };
        t.input(&mut raw, None);
        seen.extend(raw.events.clone());
        for _ in 0..3 {
            let mut raw = RawInput::default();
            t.input(&mut raw, None);
            seen.extend(raw.events);
        }
        let presses: Vec<bool> = seen
            .iter()
            .filter_map(|e| match e {
                Event::PointerButton { button: egui::PointerButton::Primary, pressed, pos, .. } if *pos == at => Some(*pressed),
                _ => None,
            })
            .collect();
        assert_eq!(presses, [true, false], "{seen:?}");
        assert!(!seen.iter().any(|e| matches!(e, Event::Touch { .. })));
    }
}
