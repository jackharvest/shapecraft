//! ShapeCraft patch: pointers with their tool, shared by the Android, iOS and Wayland backends (see PATCH.md).

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Finger,
    Stylus,
    Mouse,
    Eraser,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
    /// Moving without touching (a stylus above the glass, a mouse).
    Hover,
    /// Hovering ended (the stylus left the range).
    HoverExit,
    /// A mouse button went down or up: `button` says which.
    ButtonPress,
    ButtonRelease,
    /// A scroll wheel: `scroll` is (horizontal, vertical) in wheel steps.
    Scroll,
}

/// One pointer at one moment, in physical pixels of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub tool: Tool,
    pub id: u64,
    pub phase: Phase,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    /// The buttons held (Android's MotionEvent button state bits).
    pub buttons: u32,
    /// For `ButtonPress` / `ButtonRelease`: the button that changed (the same bits).
    pub button: u32,
    pub scroll: (f32, f32),
}

/// Button bits, as Android numbers them.
pub const PRIMARY: u32 = 1;
pub const SECONDARY: u32 = 2;
pub const TERTIARY: u32 = 4;
pub const STYLUS_PRIMARY: u32 = 32;
pub const STYLUS_SECONDARY: u32 = 64;

/// The button a press or release changed, from the buttons held before it and after it (the lowest such bit; 0
/// when none changed). Android names it itself only from Android 13 (`AMotionEvent_getActionButton`, API 33);
/// linking that call kept the app from starting on anything older (#78), so the change is worked out here.
pub fn changed_button(before: u32, now: u32, press: bool) -> u32 {
    let changed = if press { now & !before } else { before & !now };
    changed & changed.wrapping_neg()
}

/// More than this unread means no one is reading: the oldest go.
const CAP: usize = 4096;

static RECORDS: Mutex<Vec<Record>> = Mutex::new(Vec::new());

pub(crate) fn push(r: Record) {
    if let Ok(mut v) = RECORDS.lock() {
        if v.len() >= CAP {
            v.remove(0);
        }
        v.push(r);
    }
}

static TOUCHES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// On Wayland: fingers are recorded too, and no longer arrive as `WindowEvent::Touch` (an app that turns those into mouse
/// events couldn't tell them from the real mouse's).
pub fn record_touches(on: bool) {
    TOUCHES.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[allow(dead_code)]
pub(crate) fn touches_recorded() -> bool {
    TOUCHES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Everything recorded since the last call, oldest first.
pub fn take() -> Vec<Record> {
    RECORDS.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_changed_button_comes_from_the_held_ones() {
        assert_eq!(changed_button(0, PRIMARY, true), PRIMARY);
        assert_eq!(changed_button(PRIMARY, PRIMARY | SECONDARY, true), SECONDARY);
        assert_eq!(changed_button(PRIMARY | SECONDARY, PRIMARY, false), SECONDARY);
        assert_eq!(changed_button(TERTIARY, 0, false), TERTIARY);
        assert_eq!(changed_button(PRIMARY, PRIMARY, true), 0, "nothing new");
    }
}
