//! Pens and fingers on Linux (Wayland), beside the mouse (#97): the window system's own records, from our winit
//! (third_party/winit/PATCH.md), go through the touch model the Android app and the tablet use.

use shapecraft_ui::{ShapeCraftApp, global_menu};
use shapecraft_ui::tablet::{NativeTouch, Pointer, PointerPhase, PointerTool};
use winit::platform::wayland::pointers::{self, Phase, Record, Tool};

/// Tells the window system where the menus are, for a desktop with a global menu (KDE).
pub fn announce_menus() {
    if let Some(name) = global_menu::connect() {
        winit::platform::wayland::set_app_menu(&name, global_menu::PATH);
    }
}

pub struct Desktop {
    inner: ShapeCraftApp,
    touch: NativeTouch,
    pen: Pen,
}

impl Desktop {
    pub fn new(inner: ShapeCraftApp) -> Self {
        pointers::record_touches(true);
        if winit::platform::wayland::app_menu_announced() {
            global_menu::announced();
        }
        Self { inner, touch: NativeTouch::default(), pen: Pen::default() }
    }
}

impl eframe::App for Desktop {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let ppp = ctx.pixels_per_point();
        let list: Vec<Pointer> = pointers::take().into_iter().filter_map(|r| self.pen.pointer(r, ppp)).collect();
        self.touch.input_beside_mouse(raw, list, self.inner.workspace());
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.inner.ui(ui, frame);
        let over_view = self.inner.workspace().is_some_and(|ws| ws.viewport_hot);
        self.touch.after_frame(ui.ctx(), over_view);
    }

    fn on_exit(&mut self) {
        self.inner.on_exit();
    }
}

/// What the pen holds down: its tip, or a button on its barrel (a right- or middle-click without touching).
#[derive(Default)]
struct Pen {
    tip: bool,
    barrel: Option<u8>,
}

impl Pen {
    /// One of winit's records as the touch model's pointer, in points.
    fn pointer(&mut self, r: Record, ppp: f32) -> Option<Pointer> {
        let pos = egui::pos2(r.x / ppp, r.y / ppp);
        if !matches!(r.tool, Tool::Stylus | Tool::Eraser) {
            let phase = match r.phase {
                Phase::Down => PointerPhase::Down,
                Phase::Move => PointerPhase::Move,
                Phase::Up => PointerPhase::Up,
                Phase::Cancel => PointerPhase::Cancel,
                _ => return None,
            };
            return Some(Pointer { tool: PointerTool::Finger, id: r.id as i64, phase, pos, button: 0 });
        }
        let button = if r.button & pointers::STYLUS_SECONDARY != 0 { 1 } else { 2 };
        // One of the tip and the barrel at a time: the first one down is the click until it lets go.
        let (phase, button) = match r.phase {
            Phase::Down if self.barrel.is_none() => {
                self.tip = true;
                (PointerPhase::Down, 0)
            }
            Phase::Up if self.tip => {
                self.tip = false;
                (PointerPhase::Up, 0)
            }
            Phase::Cancel if self.tip => {
                self.tip = false;
                (PointerPhase::Cancel, 0)
            }
            Phase::ButtonPress if !self.tip && self.barrel.is_none() => {
                self.barrel = Some(button);
                (PointerPhase::Down, button)
            }
            Phase::ButtonRelease if self.barrel == Some(button) => {
                self.barrel = None;
                (PointerPhase::Up, button)
            }
            Phase::Move => (PointerPhase::Move, 0),
            Phase::Hover if self.barrel.is_some() => (PointerPhase::Move, 0),
            Phase::Hover => (PointerPhase::Hover, 0),
            Phase::HoverExit => (PointerPhase::Leave, 0),
            _ => return None,
        };
        Some(Pointer { tool: PointerTool::Pen, id: r.id as i64, phase, pos, button })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(phases: &[(Phase, u32)]) -> Vec<(PointerPhase, u8)> {
        let mut pen = Pen::default();
        let record = |(phase, button)| Record {
            tool: Tool::Stylus,
            id: 9,
            phase,
            x: 20.0,
            y: 40.0,
            pressure: 0.5,
            buttons: 0,
            button,
            scroll: (0.0, 0.0),
        };
        phases.iter().filter_map(|p| pen.pointer(record(*p), 2.0)).map(|p| (p.phase, p.button)).collect()
    }

    #[test]
    fn the_barrel_button_is_a_right_click_and_the_tip_is_a_click() {
        use PointerPhase as P;
        let side = pointers::STYLUS_PRIMARY;
        assert_eq!(
            run(&[(Phase::Hover, 0), (Phase::ButtonPress, side), (Phase::ButtonRelease, side), (Phase::Down, 0), (Phase::Up, 0)]),
            [(P::Hover, 0), (P::Down, 2), (P::Up, 2), (P::Down, 0), (P::Up, 0)]
        );
        // Touching with the button held is still the right-click, and it lets go once.
        assert_eq!(
            run(&[(Phase::ButtonPress, side), (Phase::Down, 0), (Phase::Up, 0), (Phase::ButtonRelease, side)]),
            [(P::Down, 2), (P::Up, 2)]
        );
        // Squeezed in the middle of a stroke: the stroke goes on.
        assert_eq!(
            run(&[(Phase::Down, 0), (Phase::ButtonPress, side), (Phase::Move, 0), (Phase::ButtonRelease, side), (Phase::Up, 0)]),
            [(P::Down, 0), (P::Move, 0), (P::Up, 0)]
        );
    }
}
