//! ShapeCraft patch: pens (`zwp_tablet_v2`), recorded as `platform::wayland::pointers` (see PATCH.md).

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use sctk::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use sctk::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_manager_v2::ZwpTabletManagerV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_group_v2::{
    self, ZwpTabletPadGroupV2,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2;
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_pad_v2::{self, ZwpTabletPadV2};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_seat_v2::{
    self, ZwpTabletSeatV2,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_tool_v2::{
    self, ZwpTabletToolV2,
};
use sctk::reexports::protocols::wp::tablet::zv2::client::zwp_tablet_v2::{self, ZwpTabletV2};

use crate::dpi::LogicalPosition;
use crate::platform::wayland::pointers::{self, Phase, Record, Tool};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, WindowId};

/// From <linux/input-event-codes.h>.
const BTN_STYLUS: u32 = 0x14b;
const BTN_STYLUS2: u32 = 0x14c;

#[derive(Debug)]
pub struct TabletState {
    manager: ZwpTabletManagerV2,
    shapes: Option<WpCursorShapeManagerV1>,
}

impl TabletState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        let shapes = globals.bind(queue_handle, 1..=1, GlobalData).ok();
        Ok(Self { manager, shapes })
    }

    pub fn tablet_seat(
        &self,
        seat: &WlSeat,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ZwpTabletSeatV2 {
        self.manager.get_tablet_seat(seat, queue_handle, ())
    }
}

/// One thing that happened to a tool since its last frame.
#[derive(Debug, Clone, Copy)]
enum Change {
    In,
    Out,
    Down,
    Up,
    Button(u32, bool),
}

#[derive(Debug, Default)]
struct ToolState {
    eraser: bool,
    surface: Option<WlSurface>,
    position: (f64, f64),
    moved: bool,
    pressure: f32,
    down: bool,
    buttons: u32,
    changes: Vec<Change>,
    shape: Option<WpCursorShapeDeviceV1>,
}

#[derive(Debug, Default)]
pub struct ToolData(Mutex<ToolState>);

impl ToolState {
    /// What the frame's changes come to, as records in the window's physical pixels.
    fn frame(&mut self, id: u64, scale: f64) -> Vec<Record> {
        let at = LogicalPosition::<f64>::from(self.position).to_physical::<f32>(scale);
        let mut out = Vec::new();
        let mut push = |s: &Self, phase, button| {
            out.push(Record {
                tool: if s.eraser { Tool::Eraser } else { Tool::Stylus },
                id,
                phase,
                x: at.x,
                y: at.y,
                pressure: s.pressure,
                buttons: s.buttons,
                button,
                scroll: (0.0, 0.0),
            });
        };
        if std::mem::take(&mut self.moved) {
            push(self, if self.down { Phase::Move } else { Phase::Hover }, 0);
        }
        for change in std::mem::take(&mut self.changes) {
            match change {
                Change::In => push(self, Phase::Hover, 0),
                Change::Down => {
                    self.down = true;
                    push(self, Phase::Down, 0);
                },
                Change::Up => {
                    self.down = false;
                    push(self, Phase::Up, 0);
                },
                Change::Button(bit, pressed) => {
                    self.buttons = if pressed { self.buttons | bit } else { self.buttons & !bit };
                    push(self, if pressed { Phase::ButtonPress } else { Phase::ButtonRelease }, bit);
                },
                Change::Out => {
                    if std::mem::take(&mut self.down) {
                        push(self, Phase::Cancel, 0);
                    }
                    // Released out of range: the compositor needn't say so.
                    for bit in [pointers::STYLUS_PRIMARY, pointers::STYLUS_SECONDARY] {
                        if self.buttons & bit != 0 {
                            self.buttons &= !bit;
                            push(self, Phase::ButtonRelease, bit);
                        }
                    }
                    push(self, Phase::HoverExit, 0);
                    self.surface = None;
                },
            }
        }
        out
    }
}

impl WinitState {
    /// Records pointers on a window and has it drawn, since the app takes them as it draws.
    pub fn record_pointers(&mut self, window_id: WindowId, records: Vec<Record>) {
        if records.is_empty() {
            return;
        }
        records.into_iter().for_each(pointers::push);
        if let Some(requests) = self.window_requests.get_mut().get(&window_id) {
            requests.redraw_requested.store(true, Ordering::Relaxed);
        }
        self.dispatched_events = true;
    }
}

impl Dispatch<ZwpTabletManagerV2, GlobalData, WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletManagerV2,
        _: <ZwpTabletManagerV2 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletSeatV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ToolData::default()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        _: &ZwpTabletSeatV2,
        _: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        tablet: &ZwpTabletV2,
        event: zwp_tablet_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        if let zwp_tablet_v2::Event::Removed = event {
            tablet.destroy();
        }
    }
}

impl Dispatch<ZwpTabletToolV2, ToolData, WinitState> for TabletState {
    fn event(
        state: &mut WinitState,
        tool: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        data: &ToolData,
        _: &Connection,
        queue_handle: &QueueHandle<WinitState>,
    ) {
        use zwp_tablet_tool_v2::{ButtonState, Event, Type};
        let Ok(mut t) = data.0.lock() else { return };
        match event {
            Event::Type { tool_type: WEnum::Value(kind) } => t.eraser = kind == Type::Eraser,
            Event::ProximityIn { serial, surface, .. } => {
                // A client that sets no cursor may get none at all.
                if t.shape.is_none() {
                    let shapes = state.tablet_state.as_ref().and_then(|s| s.shapes.as_ref());
                    t.shape = shapes.map(|s| s.get_tablet_tool_v2(tool, queue_handle, GlobalData));
                }
                if let Some(shape) = &t.shape {
                    shape.set_shape(serial, Shape::Default);
                }
                t.surface = Some(surface);
                t.changes.push(Change::In);
            },
            Event::ProximityOut => t.changes.push(Change::Out),
            Event::Down { .. } => t.changes.push(Change::Down),
            Event::Up => t.changes.push(Change::Up),
            Event::Motion { x, y } => {
                t.position = (x, y);
                t.moved = true;
            },
            Event::Pressure { pressure } => t.pressure = pressure as f32 / 65535.0,
            Event::Button { button, state, .. } => {
                let bit = match button {
                    BTN_STYLUS => pointers::STYLUS_PRIMARY,
                    BTN_STYLUS2 => pointers::STYLUS_SECONDARY,
                    _ => return,
                };
                let pressed = state == WEnum::Value(ButtonState::Pressed);
                t.changes.push(Change::Button(bit, pressed));
            },
            Event::Frame { .. } => {
                let window = t.surface.as_ref().map(wayland::make_wid);
                let scale = window.and_then(|id| {
                    let window = state.windows.get_mut().get(&id)?;
                    Some(window.lock().ok()?.scale_factor())
                });
                let id = u64::from(tool.id().protocol_id());
                let records = t.frame(id, scale.unwrap_or(1.0));
                drop(t);
                if let (Some(window), true) = (window, scale.is_some()) {
                    state.record_pointers(window, records);
                }
            },
            Event::Removed => {
                let window = t.surface.as_ref().map(wayland::make_wid);
                if let Some(window) = window {
                    t.changes.push(Change::Out);
                    let records = t.frame(u64::from(tool.id().protocol_id()), 1.0);
                    state.record_pointers(window, records);
                }
                if let Some(shape) = t.shape.take() {
                    shape.destroy();
                }
                tool.destroy();
            },
            _ => (),
        }
    }
}

impl Dispatch<ZwpTabletPadV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        pad: &ZwpTabletPadV2,
        event: zwp_tablet_pad_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        if let zwp_tablet_pad_v2::Event::Removed = event {
            pad.destroy();
        }
    }
}

impl Dispatch<ZwpTabletPadGroupV2, (), WinitState> for TabletState {
    event_created_child!(WinitState, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);

    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadGroupV2,
        _: zwp_tablet_pad_group_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletPadRingV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadRingV2,
        _: <ZwpTabletPadRingV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpTabletPadStripV2, (), WinitState> for TabletState {
    fn event(
        _: &mut WinitState,
        _: &ZwpTabletPadStripV2,
        _: <ZwpTabletPadStripV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
    }
}

delegate_dispatch!(WinitState: [ZwpTabletManagerV2: GlobalData] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletSeatV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletToolV2: ToolData] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadGroupV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadRingV2: ()] => TabletState);
delegate_dispatch!(WinitState: [ZwpTabletPadStripV2: ()] => TabletState);

#[cfg(test)]
mod tests {
    use super::*;

    fn phases(t: &mut ToolState) -> Vec<Phase> {
        t.frame(1, 2.0).into_iter().map(|r| r.phase).collect()
    }

    #[test]
    fn a_stroke_hovers_presses_moves_and_lifts() {
        let mut t = ToolState { position: (10.0, 20.0), moved: true, ..Default::default() };
        t.changes.push(Change::In);
        let first = t.frame(1, 2.0);
        assert_eq!((first[0].x, first[0].y), (20.0, 40.0), "in physical pixels");
        t.changes.push(Change::Down);
        assert_eq!(phases(&mut t), [Phase::Down]);
        t.moved = true;
        assert_eq!(phases(&mut t), [Phase::Move]);
        // Moved and lifted in one frame: the last point of the stroke comes first.
        t.moved = true;
        t.changes.push(Change::Up);
        assert_eq!(phases(&mut t), [Phase::Move, Phase::Up]);
        t.moved = true;
        assert_eq!(phases(&mut t), [Phase::Hover]);
    }

    #[test]
    fn leaving_with_the_button_held_lets_go_of_it() {
        let mut t = ToolState::default();
        t.changes.extend([Change::In, Change::Button(pointers::STYLUS_PRIMARY, true)]);
        assert_eq!(phases(&mut t), [Phase::Hover, Phase::ButtonPress]);
        assert_eq!(t.buttons, pointers::STYLUS_PRIMARY);
        t.changes.push(Change::Out);
        assert_eq!(phases(&mut t), [Phase::ButtonRelease, Phase::HoverExit]);
        assert_eq!(t.buttons, 0);
    }
}
