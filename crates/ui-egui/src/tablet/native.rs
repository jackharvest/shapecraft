//! The tablet's touch model for a touch screen the app draws on itself (the Android app): winit's touches go through
//! the same `Router` as the page's, so a finger taps, long-presses, turns, pans and pinches as it does on the tablet,
//! and the screen is scaled so the interface fits, as the page scales it.

use std::time::Instant;

use egui::{Event, RawInput, TouchPhase};

use super::session::{FINGER_REACH, MIN_SIZE, PEN_REACH, TOUCH_ZOOM};
use super::touch::{Device, Kind, Nav, Router};
use crate::workspace::Workspace;

/// What a pointer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerTool {
    Finger,
    /// A stylus (its eraser end too): a precise mouse that can hover.
    Pen,
    Mouse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
    /// Moving without touching (a pen above the glass, a mouse with no button held).
    Hover,
    /// The pen left hover range, or the mouse the window.
    Leave,
    /// A scroll wheel, in lines (x, y).
    Scroll(egui::Vec2),
}

/// One pointer event from the platform, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    pub tool: PointerTool,
    pub id: i64,
    pub phase: PointerPhase,
    pub pos: egui::Pos2,
    /// For a pen or mouse: 0 primary, 1 middle, 2 secondary (the pen's barrel button is secondary).
    pub button: u8,
}

#[derive(Default)]
pub struct NativeTouch {
    router: Router,
}

impl NativeTouch {
    /// Before a frame: the screen's pointers become the router's clicks and drags, and finger gestures turn into
    /// view changes on the workspace. `pointers` are the platform's own records with their tool (pen, finger, mouse);
    /// without them (None) winit's touches are taken as fingers. Either way the pointer events winit made up from the
    /// first finger are dropped.
    pub fn input(&mut self, raw: &mut RawInput, pointers: Option<Vec<Pointer>>, ws: Option<&mut Workspace>) {
        let now = Instant::now();
        let mut rest = Vec::with_capacity(raw.events.len());
        let from_platform = pointers.is_some();
        for ev in raw.events.drain(..) {
            match ev {
                Event::Touch { id, phase, pos, .. } if !from_platform => {
                    let kind = match phase {
                        TouchPhase::Start => Kind::Down,
                        TouchPhase::Move => Kind::Move,
                        TouchPhase::End => Kind::Up,
                        TouchPhase::Cancel => Kind::Cancel,
                    };
                    self.router.pointer(kind, Device::Touch, id.0 as i64, pos, 0, now);
                }
                Event::Touch { .. } | Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone => {}
                Event::MouseWheel { .. } if from_platform => {}
                other => {
                    // Shift- or Ctrl-clicks with a keyboard: the router's clicks carry the keys held.
                    if let Event::Key { modifiers, .. } = &other {
                        self.router.mods = *modifiers;
                    }
                    rest.push(other);
                }
            }
        }
        for p in pointers.unwrap_or_default() {
            let device = match p.tool {
                PointerTool::Finger => Device::Touch,
                PointerTool::Pen => Device::Pen,
                PointerTool::Mouse => Device::Mouse,
            };
            if let PointerPhase::Scroll(delta) = p.phase {
                rest.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta,
                    phase: TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: self.router.mods,
                });
                continue;
            }
            let kind = match p.phase {
                PointerPhase::Down => Kind::Down,
                PointerPhase::Move => Kind::Move,
                PointerPhase::Up => Kind::Up,
                PointerPhase::Cancel => Kind::Cancel,
                PointerPhase::Hover => Kind::Hover,
                PointerPhase::Leave => Kind::Leave,
                PointerPhase::Scroll(_) => continue,
            };
            self.router.pointer(kind, device, p.id, p.pos, p.button, now);
        }
        self.router.tick(now);
        let (events, nav) = self.router.take();
        rest.extend(events);
        raw.events = rest;
        if let Some(ws) = ws {
            ws.touch_drag = self.router.finger;
            ws.finger_nav = self.router.device == Device::Touch;
            ws.pen_input = self.router.device == Device::Pen;
            if ws.pen_input {
                super::mark_pen_seen();
            }
            ws.pen_held = self.router.pen_held;
            ws.pick_slop = match self.router.device {
                Device::Mouse => 1.0,
                Device::Pen => PEN_REACH,
                Device::Touch => FINGER_REACH,
            };
            for n in nav {
                match n {
                    Nav::Pan(d) => ws.touch_pan(d),
                    Nav::Zoom(f, at) => ws.touch_zoom(f, at),
                }
            }
        }
    }

    /// The pointer used last: the pen, a finger or a mouse.
    pub fn last_tool(&self) -> PointerTool {
        match self.router.device {
            Device::Touch => PointerTool::Finger,
            Device::Pen => PointerTool::Pen,
            Device::Mouse => PointerTool::Mouse,
        }
    }

    /// After a frame: tells the router what the finger was over, and asks for the frames a pending gesture needs.
    pub fn after_frame(&mut self, ctx: &egui::Context, over_view: bool) {
        self.router.after_frame(over_view, ctx.dragged_id().is_some());
        if self.router.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    /// The zoom that gives the interface at least `MIN_SIZE` points on this screen, and makes it up to `TOUCH_ZOOM`
    /// bigger where there's room, as the tablet page scales it: from the window's pixels and the system's own pixels
    /// per point.
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
    1.0 / (MIN_SIZE[0] / w).max(MIN_SIZE[1] / h).max(1.0 / TOUCH_ZOOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_screens_keep_their_size_and_phones_shrink_to_fit() {
        // A Galaxy Tab sideways (1280 × 800 dp): an iPad point is 1.21 dp, still 1058 × 661 points.
        assert!((fit(1280.0, 800.0) - TOUCH_ZOOM).abs() < 1e-6);
        // Just over the minimum: only as much bigger as still leaves 1000 points.
        assert!((1100.0 / fit(1100.0, 900.0) - 1000.0).abs() < 1e-3);
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
        t.input(&mut raw, None, None);
        let mut seen = raw.events.clone();
        let mut raw = RawInput { events: vec![touch(TouchPhase::End)], ..Default::default() };
        t.input(&mut raw, None, None);
        seen.extend(raw.events.clone());
        for _ in 0..3 {
            let mut raw = RawInput::default();
            t.input(&mut raw, None, None);
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

    #[test]
    fn a_pen_stroke_is_a_mouse_drag_and_fingers_from_the_platform_still_tap() {
        let mut t = NativeTouch::default();
        let p = |phase, x: f32, tool| Pointer { tool, id: 1, phase, pos: egui::pos2(x, 50.0), button: 0 };
        let mut seen = Vec::new();
        for batch in [
            vec![p(PointerPhase::Hover, 10.0, PointerTool::Pen)],
            vec![p(PointerPhase::Down, 10.0, PointerTool::Pen)],
            vec![p(PointerPhase::Move, 40.0, PointerTool::Pen)],
            vec![p(PointerPhase::Up, 40.0, PointerTool::Pen)],
        ] {
            // A finger winit also reported is ignored when the platform's own records come in.
            let ghost = Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(9),
                phase: TouchPhase::Start,
                pos: egui::pos2(500.0, 500.0),
                force: None,
            };
            let mut raw = RawInput { events: vec![ghost], ..Default::default() };
            t.input(&mut raw, Some(batch), None);
            seen.extend(raw.events);
        }
        assert_eq!(t.last_tool(), PointerTool::Pen);
        let buttons: Vec<(bool, f32)> = seen
            .iter()
            .filter_map(|e| match e {
                Event::PointerButton { pressed, pos, .. } => Some((*pressed, pos.x)),
                _ => None,
            })
            .collect();
        assert_eq!(buttons, [(true, 10.0), (false, 40.0)], "{seen:?}");
        assert!(seen.iter().any(|e| matches!(e, Event::PointerMoved(p) if p.x == 40.0)));
        assert!(!seen.iter().any(|e| matches!(e, Event::PointerButton { pos, .. } if pos.x == 500.0)));
    }
}
