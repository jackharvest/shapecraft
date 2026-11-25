//! Touch handling.

use tracing::warn;

use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::protocol::wl_touch::WlTouch;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};

use sctk::seat::touch::{TouchData, TouchHandler};

use crate::dpi::LogicalPosition;
use crate::event::{Touch, TouchPhase, WindowEvent};

use crate::platform::wayland::pointers::{self, Phase, Record, Tool};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};

/// ShapeCraft patch: a finger as a record, where the app asked for those; `false` sends the window event.
fn record(state: &mut WinitState, window: WindowId, id: i32, at: LogicalPosition<f64>, scale: f64, phase: Phase) -> bool {
    if !pointers::touches_recorded() {
        return false;
    }
    let at = at.to_physical::<f32>(scale);
    state.record_pointers(
        window,
        vec![Record {
            tool: Tool::Finger,
            id: id as u64,
            phase,
            x: at.x,
            y: at.y,
            pressure: 1.0,
            buttons: 0,
            button: 0,
            scroll: (0.0, 0.0),
        }],
    );
    true
}

impl TouchHandler for WinitState {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        _: u32,
        _: u32,
        surface: WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        let window_id = wayland::make_wid(&surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::down without seat");
                return;
            },
        };

        // Update the state of the point.
        let location = LogicalPosition::<f64>::from(position);
        seat_state.touch_map.insert(id, TouchPoint { surface, location });
        if record(self, window_id, id, location, scale_factor, Phase::Down) {
            return;
        }

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Started,
                location: location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        _: u32,
        _: u32,
        id: i32,
    ) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::up without seat");
                return;
            },
        };

        // Remove the touch point.
        let touch_point = match seat_state.touch_map.remove(&id) {
            Some(touch_point) => touch_point,
            None => return,
        };

        let window_id = wayland::make_wid(&touch_point.surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        if record(self, window_id, id, touch_point.location, scale_factor, Phase::Up) {
            return;
        }

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Ended,
                location: touch_point.location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        touch: &WlTouch,
        _: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::motion without seat");
                return;
            },
        };

        // Remove the touch point.
        let touch_point = match seat_state.touch_map.get_mut(&id) {
            Some(touch_point) => touch_point,
            None => return,
        };

        let window_id = wayland::make_wid(&touch_point.surface);
        let scale_factor = match self.windows.get_mut().get(&window_id) {
            Some(window) => window.lock().unwrap().scale_factor(),
            None => return,
        };

        touch_point.location = LogicalPosition::<f64>::from(position);
        let location = touch_point.location;
        if record(self, window_id, id, location, scale_factor, Phase::Move) {
            return;
        }

        self.events_sink.push_window_event(
            WindowEvent::Touch(Touch {
                device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                    DeviceId,
                )),
                phase: TouchPhase::Moved,
                location: location.to_physical(scale_factor),
                force: None,
                id: id as u64,
            }),
            window_id,
        );
    }

    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, touch: &WlTouch) {
        let seat_state = match self.seats.get_mut(&touch.seat().id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received wl_touch::cancel without seat");
                return;
            },
        };

        let points: Vec<_> = seat_state.touch_map.drain().collect();
        for (id, touch_point) in points {
            let window_id = wayland::make_wid(&touch_point.surface);
            let scale_factor = match self.windows.get_mut().get(&window_id) {
                Some(window) => window.lock().unwrap().scale_factor(),
                None => return,
            };

            if record(self, window_id, id, touch_point.location, scale_factor, Phase::Cancel) {
                continue;
            }

            let location = touch_point.location.to_physical(scale_factor);

            self.events_sink.push_window_event(
                WindowEvent::Touch(Touch {
                    device_id: crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(
                        DeviceId,
                    )),
                    phase: TouchPhase::Cancelled,
                    location,
                    force: None,
                    id: id as u64,
                }),
                window_id,
            );
        }
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
        // Blank.
    }

    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlTouch, _: i32, _: f64) {
        // Blank.
    }
}

/// The state of the touch point.
#[derive(Debug)]
pub struct TouchPoint {
    /// The surface on which the point is present.
    pub surface: WlSurface,

    /// The location of the point on the surface.
    pub location: LogicalPosition<f64>,
}

pub trait TouchDataExt {
    fn seat(&self) -> &WlSeat;
}

impl TouchDataExt for WlTouch {
    fn seat(&self) -> &WlSeat {
        self.data::<TouchData>().expect("failed to get touch data.").seat()
    }
}

sctk::delegate_touch!(WinitState);
