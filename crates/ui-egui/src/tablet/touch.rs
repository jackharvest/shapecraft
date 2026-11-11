//! Pen, touch and mouse from the page, turned into what the app understands.
//!
//! - Pencil and mouse are the pointer, as on the desktop: press, drag, release, and hover (Apple Pencil hovers over
//!   the screen on recent iPads, which lights up what a tap would pick).
//! - One finger: a tap is a click, a long press a right-click (the context menu), and a drag is a mouse drag. In the
//!   3D view a drag on nothing turns the view even with a sketch tool on (the pencil draws); handles still drag.
//!   Elsewhere a drag that grabs nothing scrolls what's under it.
//! - Two fingers in the 3D view pan and pinch-zoom it; elsewhere they scroll. A quick two-finger tap undoes, three
//!   fingers redo, as in iPad drawing apps.
//! - Palm rejection: fingers are ignored while the pencil touches the glass and briefly after.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};

/// How far a finger goes before it's a drag, not a tap (points).
const SLOP: f32 = 9.0;
const LONG_PRESS: Duration = Duration::from_millis(520);
/// Fingers that come down this soon after the pencil lifts are taken for the palm.
const PALM_AFTER_PEN: Duration = Duration::from_millis(350);
/// How far three fingers go sideways to undo or redo (points).
const SWIPE: f32 = 50.0;
/// A pencil tap wobbles on the glass: this little movement this soon is still a tap at the spot it touched.
const PEN_SLOP: f32 = 7.0;
const PEN_TAP: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Down,
    Move,
    Up,
    Cancel,
    /// The pencil or mouse moved without touching (hover).
    Hover,
    /// The pencil or mouse left the page.
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Device {
    Mouse,
    Pen,
    Touch,
}

/// A view change for the workspace to make, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Nav {
    Pan(Vec2),
    /// Grow by this factor (> 1 is closer) around this point.
    Zoom(f32, Pos2),
}

struct Finger {
    id: i64,
    start: Pos2,
    at: Pos2,
}

enum Mode {
    Idle,
    /// One finger down: not yet a tap, a drag or a long press.
    Pending {
        since: Instant,
    },
    /// One finger pressed as the mouse. `scroll` is decided two frames in: whether anything took the drag.
    Drag {
        scroll: Option<bool>,
        pressed_at_frame: u64,
        last: Pos2,
    },
    Multi {
        since: Instant,
        centroid: Pos2,
        spread: f32,
        moved: f32,
        most: usize,
        over_view: Option<bool>,
        /// How far the fingers' middle has gone in all (a three-finger swipe is undo / redo).
        travel: Vec2,
    },
    /// A finger held still: over the 3D view nothing happens (the reference); over a panel it is a right-click,
    /// decided once the frame has said what is under it.
    Hold {
        at: Pos2,
        decided: bool,
    },
}

pub(super) struct Router {
    now: Vec<Event>,
    /// Events for the frames after this one, a batch each (a press and its release a frame apart).
    later: VecDeque<Vec<Event>>,
    nav: Vec<Nav>,
    fingers: Vec<Finger>,
    ignored: Vec<i64>,
    mode: Mode,
    pen_down: bool,
    pen_up: Option<Instant>,
    /// Where and when the pencil touched, until it moves off as a drag.
    pen_press: Option<(Pos2, Instant)>,
    /// Where and when the pencil touched, while it stays put (a long press starts an Area Selection, as on iPad).
    pen_hold: Option<(Pos2, Instant)>,
    /// The pencil was held still long enough: the Area Selection starts here once it moves.
    pub pen_held: Option<Pos2>,
    /// What moved the pointer last: a pencil or a finger reaches farther for edges than a mouse.
    pub device: Device,
    /// The keyboard's modifiers, from the last key or pointer event.
    pub mods: Modifiers,
    frame: u64,
    /// A finger drives the pointer: the workspace turns the view on drags instead of drawing or box-selecting.
    pub finger: bool,
    finger_until: u64,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            now: Vec::new(),
            later: VecDeque::new(),
            nav: Vec::new(),
            fingers: Vec::new(),
            ignored: Vec::new(),
            mode: Mode::Idle,
            pen_down: false,
            pen_up: None,
            pen_press: None,
            pen_hold: None,
            pen_held: None,
            device: Device::Mouse,
            mods: Modifiers::NONE,
            frame: 0,
            finger: false,
            finger_until: 0,
        }
    }
}

fn centroid(f: &[Finger]) -> Pos2 {
    let n = f.len().max(1) as f32;
    let sum = f.iter().fold(Vec2::ZERO, |s, f| s + f.at.to_vec2());
    (sum / n).to_pos2()
}

fn spread(f: &[Finger], c: Pos2) -> f32 {
    let n = f.len().max(1) as f32;
    (f.iter().map(|f| f.at.distance(c)).sum::<f32>() / n).max(1.0)
}

impl Router {
    /// Something is still to happen without new input (a release a frame later, a long press to time out).
    pub fn busy(&self) -> bool {
        !self.now.is_empty() || !self.later.is_empty() || !self.nav.is_empty() || matches!(self.mode, Mode::Pending { .. })
    }

    /// The page went away: forget fingers and buttons.
    pub fn reset(&mut self) {
        let mods = self.mods;
        *self = Self { mods, ..Self::default() };
        self.now.push(Event::PointerGone);
    }

    fn button(&self, pos: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton { pos, button, pressed, modifiers: self.mods }
    }

    fn after(&mut self, frames_ahead: usize, ev: Event) {
        while self.later.len() < frames_ahead {
            self.later.push_back(Vec::new());
        }
        if let Some(batch) = self.later.get_mut(frames_ahead - 1) {
            batch.push(ev);
        }
    }

    pub fn key(&mut self, ev: Event) {
        self.now.push(ev);
    }

    pub fn pointer(&mut self, kind: Kind, device: Device, id: i64, pos: Pos2, button: u8, now: Instant) {
        if kind != Kind::Leave && !(device == Device::Touch && self.palm(now)) {
            self.device = device;
        }
        match device {
            Device::Touch => self.touch(kind, id, pos, now),
            Device::Pen | Device::Mouse => self.mouse(kind, device, pos, button, now),
        }
    }

    fn mouse(&mut self, kind: Kind, device: Device, pos: Pos2, button: u8, now: Instant) {
        let button = match button {
            1 => PointerButton::Middle,
            2 => PointerButton::Secondary,
            3 => PointerButton::Extra1,
            4 => PointerButton::Extra2,
            _ => PointerButton::Primary,
        };
        match kind {
            Kind::Down => {
                if device == Device::Pen {
                    self.cancel_fingers();
                    self.pen_down = true;
                    self.pen_press = Some((pos, now));
                    self.pen_hold = Some((pos, now));
                    self.pen_held = None;
                }
                self.finger = false;
                self.now.push(Event::PointerMoved(pos));
                self.now.push(self.button(pos, button, true));
            }
            Kind::Move | Kind::Hover => {
                if let Some((at, since)) = self.pen_hold
                    && kind == Kind::Move
                    && self.pen_held.is_none()
                {
                    if pos.distance(at) > PEN_SLOP {
                        self.pen_hold = None;
                    } else if now.duration_since(since) >= LONG_PRESS {
                        self.pen_held = Some(at);
                    }
                }
                if let Some((at, since)) = self.pen_press {
                    if kind == Kind::Move && pos.distance(at) < PEN_SLOP && now.duration_since(since) < PEN_TAP {
                        return;
                    }
                    self.pen_press = None;
                }
                self.now.push(Event::PointerMoved(pos));
            }
            Kind::Up | Kind::Cancel => {
                // A tap that wobbled lifts where it touched, so it stays a click on what it touched.
                let pos = self.pen_press.take().filter(|(at, _)| at.distance(pos) < PEN_SLOP).map_or(pos, |(at, _)| at);
                self.now.push(Event::PointerMoved(pos));
                self.now.push(self.button(pos, button, false));
                if device == Device::Pen {
                    self.pen_down = false;
                    self.pen_up = Some(now);
                    self.pen_hold = None;
                    self.pen_held = None;
                }
                if kind == Kind::Cancel {
                    self.after(1, Event::PointerGone);
                }
            }
            Kind::Leave => self.now.push(Event::PointerGone),
        }
    }

    /// The pencil came down mid-gesture: what the fingers were doing stops.
    fn cancel_fingers(&mut self) {
        if let Mode::Drag { scroll, last, .. } = self.mode
            && scroll != Some(true)
        {
            self.now.push(self.button(last, PointerButton::Primary, false));
        }
        self.ignored.extend(self.fingers.drain(..).map(|f| f.id));
        self.mode = Mode::Idle;
    }

    fn palm(&self, now: Instant) -> bool {
        self.pen_down || self.pen_up.is_some_and(|t| now.duration_since(t) < PALM_AFTER_PEN)
    }

    fn touch(&mut self, kind: Kind, id: i64, pos: Pos2, now: Instant) {
        if self.ignored.contains(&id) {
            if matches!(kind, Kind::Up | Kind::Cancel) {
                self.ignored.retain(|i| *i != id);
            }
            return;
        }
        match kind {
            Kind::Down => {
                if self.palm(now) {
                    self.ignored.push(id);
                    return;
                }
                self.fingers.retain(|f| f.id != id);
                self.fingers.push(Finger { id, start: pos, at: pos });
                match self.mode {
                    Mode::Idle => self.mode = Mode::Pending { since: now },
                    Mode::Pending { .. } => self.start_multi(now),
                    Mode::Drag { scroll, last, .. } => {
                        if scroll != Some(true) {
                            self.now.push(self.button(last, PointerButton::Primary, false));
                        }
                        self.start_multi(now);
                    }
                    Mode::Multi { ref mut most, ref mut centroid, ref mut spread, .. } => {
                        *most = (*most).max(self.fingers.len());
                        // A new finger moves the middle; start measuring from here so the view doesn't jump.
                        *centroid = self::centroid(&self.fingers);
                        *spread = self::spread(&self.fingers, *centroid);
                    }
                    // A second finger after a hold: the hold is over, the fingers start a gesture.
                    Mode::Hold { .. } => self.start_multi(now),
                }
            }
            Kind::Move => {
                let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) else { return };
                let before = f.at;
                f.at = pos;
                let start = f.start;
                self.moved_finger(before, pos, start);
            }
            Kind::Up | Kind::Cancel => {
                let Some(i) = self.fingers.iter().position(|f| f.id == id) else { return };
                let f = self.fingers.remove(i);
                self.lifted(f, kind == Kind::Cancel, now);
            }
            Kind::Hover | Kind::Leave => {}
        }
    }

    fn moved_finger(&mut self, before: Pos2, pos: Pos2, start: Pos2) {
        match &mut self.mode {
            Mode::Pending { .. } if pos.distance(start) > SLOP => {
                // Press where the finger came down, then follow it: handles and widgets see an ordinary drag.
                self.finger = true;
                self.now.push(Event::PointerMoved(start));
                self.now.push(self.button(start, PointerButton::Primary, true));
                self.after(1, Event::PointerMoved(pos));
                self.mode = Mode::Drag { scroll: None, pressed_at_frame: self.frame, last: pos };
            }
            Mode::Drag { scroll, last, .. } => {
                *last = pos;
                if *scroll == Some(true) {
                    let delta = pos - before;
                    self.now.push(Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        modifiers: self.mods,
                    });
                } else {
                    self.now.push(Event::PointerMoved(pos));
                }
            }
            Mode::Multi { centroid, spread, moved, over_view, travel, most, .. } => {
                let c = self::centroid(&self.fingers);
                let s = self::spread(&self.fingers, c);
                let dc = c - *centroid;
                *moved += dc.length() + (s - *spread).abs();
                *travel += dc;
                // Three fingers are a swipe for undo / redo, never a pan or zoom.
                let swipe = *most >= 3;
                match over_view {
                    _ if swipe => {}
                    Some(true) => {
                        self.nav.push(Nav::Pan(dc));
                        self.nav.push(Nav::Zoom(s / *spread, c));
                    }
                    Some(false) => {
                        self.now.push(Event::PointerMoved(c));
                        self.now.push(Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: dc,
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: self.mods,
                        });
                    }
                    None => {}
                }
                *centroid = c;
                *spread = s;
            }
            _ => {}
        }
    }

    fn lifted(&mut self, f: Finger, cancelled: bool, now: Instant) {
        match self.mode {
            Mode::Pending { .. } => {
                if !cancelled {
                    // A tap: press now, release next frame, then no hover (a finger isn't over anything once lifted).
                    self.finger = true;
                    self.now.push(Event::PointerMoved(f.at));
                    self.now.push(self.button(f.at, PointerButton::Primary, true));
                    self.after(1, self.button(f.at, PointerButton::Primary, false));
                    self.after(2, Event::PointerGone);
                    self.finger_until = self.frame + 2;
                }
                self.mode = Mode::Idle;
            }
            Mode::Drag { scroll, .. } => {
                if scroll != Some(true) {
                    self.now.push(Event::PointerMoved(f.at));
                    self.now.push(self.button(f.at, PointerButton::Primary, false));
                }
                self.after(1, Event::PointerGone);
                self.finger_until = self.frame + 1;
                self.mode = Mode::Idle;
            }
            Mode::Multi { since, moved, most, over_view, travel, .. } => {
                if !self.fingers.is_empty() {
                    // Lifting one of several: the rest carry on (a tap's fingers never lift at quite the same time),
                    // measured from where they are so the view doesn't jump.
                    let c = centroid(&self.fingers);
                    self.mode = Mode::Multi { since, centroid: c, spread: spread(&self.fingers, c), moved, most, over_view, travel };
                    return;
                }
                // A three-finger swipe sideways: left undoes, right redoes (the reference on iPad). Taps do nothing.
                if !cancelled && most >= 3 && travel.x.abs() >= SWIPE && travel.x.abs() > 1.5 * travel.y.abs() {
                    self.undo_key(travel.x > 0.0);
                }
                let _ = (since, now);
                if over_view == Some(false) {
                    self.after(1, Event::PointerGone);
                }
                self.mode = Mode::Idle;
            }
            Mode::Hold { .. } => {
                if self.fingers.is_empty() {
                    self.after(1, Event::PointerGone);
                    self.mode = Mode::Idle;
                }
            }
            Mode::Idle => {}
        }
    }

    fn start_multi(&mut self, now: Instant) {
        let c = centroid(&self.fingers);
        let since = match self.mode {
            Mode::Pending { since } => since,
            _ => now,
        };
        self.mode = Mode::Multi {
            since,
            centroid: c,
            spread: spread(&self.fingers, c),
            moved: 0.0,
            most: self.fingers.len(),
            over_view: None,
            travel: Vec2::ZERO,
        };
        // Look at what's under the fingers' middle; `after_frame` reads the answer.
        self.now.push(Event::PointerMoved(c));
    }

    /// Undo (⌘Z), or redo (⇧⌘Z).
    fn undo_key(&mut self, redo: bool) {
        let mut m = command_mods();
        m.shift = redo;
        for pressed in [true, false] {
            self.now.push(Event::Key { key: Key::Z, physical_key: Some(Key::Z), pressed, repeat: false, modifiers: m });
        }
    }

    /// Time passing with no input: a held finger becomes a long press.
    pub fn tick(&mut self, now: Instant) {
        if let Some((at, since)) = self.pen_hold
            && self.pen_held.is_none()
            && now.duration_since(since) >= LONG_PRESS
        {
            self.pen_held = Some(at);
        }
        if let Mode::Pending { since } = self.mode
            && now.duration_since(since) >= LONG_PRESS
            && let Some(f) = self.fingers.first()
        {
            let at = f.at;
            self.finger = true;
            self.now.push(Event::PointerMoved(at));
            self.mode = Mode::Hold { at, decided: false };
        }
    }

    /// This frame's events and view changes; the next batch moves up.
    pub fn take(&mut self) -> (Vec<Event>, Vec<Nav>) {
        let events = std::mem::take(&mut self.now);
        if let Some(next) = self.later.pop_front() {
            self.now = next;
        }
        (events, std::mem::take(&mut self.nav))
    }

    /// After a frame: what's under a new two-finger gesture, and whether a one-finger drag took hold of anything.
    /// `over_view`: the pointer is over the 3D view itself (no panel on top). `dragging`: a widget has the drag.
    pub fn after_frame(&mut self, over_view: bool, dragging: bool) {
        self.frame += 1;
        if self.finger && self.finger_until != 0 && self.frame > self.finger_until && matches!(self.mode, Mode::Idle) {
            self.finger = false;
            self.finger_until = 0;
        }
        match &mut self.mode {
            Mode::Hold { at, decided: false } => {
                let at = *at;
                if !over_view {
                    self.now.push(self.button(at, PointerButton::Secondary, true));
                    self.after(1, self.button(at, PointerButton::Secondary, false));
                }
                self.mode = Mode::Hold { at, decided: true };
            }
            Mode::Multi { over_view: o @ None, .. } => {
                *o = Some(over_view);
                if over_view {
                    self.now.push(Event::PointerGone);
                }
            }
            Mode::Drag { scroll: s @ None, pressed_at_frame, last } if self.frame >= *pressed_at_frame + 2 => {
                let scroll = !dragging && !over_view;
                *s = Some(scroll);
                if scroll {
                    // Let go far from where it pressed: not a click, and the rest of the drag scrolls.
                    let at = *last;
                    self.now.push(self.button(at, PointerButton::Primary, false));
                }
            }
            _ => {}
        }
    }
}

/// ⌘ on a Mac host, Ctrl elsewhere, as egui reads it.
pub(super) fn command_mods() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers { mac_cmd: true, command: true, ..Modifiers::NONE }
    } else {
        Modifiers { ctrl: true, command: true, ..Modifiers::NONE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn presses(ev: &[Event]) -> Vec<(PointerButton, bool)> {
        ev.iter()
            .filter_map(|e| match e {
                Event::PointerButton { button, pressed, .. } => Some((*button, *pressed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_tap_presses_then_releases_a_frame_later() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Touch, 1, pos2(100.0, 100.0), 0, t);
        assert!(r.take().0.is_empty(), "nothing until it's known to be a tap");
        r.pointer(Kind::Up, Device::Touch, 1, pos2(101.0, 100.0), 0, t + Duration::from_millis(80));
        assert_eq!(presses(&r.take().0), vec![(PointerButton::Primary, true)]);
        assert_eq!(presses(&r.take().0), vec![(PointerButton::Primary, false)]);
        assert!(r.take().0.contains(&Event::PointerGone));
    }

    #[test]
    fn a_held_pencil_marks_an_area_selection_and_a_quick_drag_does_not() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Pen, 1, pos2(100.0, 100.0), 0, t);
        r.tick(t + Duration::from_millis(200));
        assert_eq!(r.pen_held, None);
        r.pointer(Kind::Move, Device::Pen, 1, pos2(103.0, 101.0), 0, t + Duration::from_millis(400));
        r.tick(t + Duration::from_millis(600));
        assert_eq!(r.pen_held, Some(pos2(100.0, 100.0)), "held still past the long press");
        r.pointer(Kind::Move, Device::Pen, 1, pos2(200.0, 150.0), 0, t + Duration::from_millis(700));
        assert_eq!(r.pen_held, Some(pos2(100.0, 100.0)), "stays while the box is dragged");
        r.pointer(Kind::Up, Device::Pen, 1, pos2(200.0, 150.0), 0, t + Duration::from_millis(800));
        assert_eq!(r.pen_held, None);
        // A drag that moves off at once is no long press.
        r.pointer(Kind::Down, Device::Pen, 1, pos2(100.0, 100.0), 0, t + Duration::from_secs(2));
        r.pointer(Kind::Move, Device::Pen, 1, pos2(140.0, 100.0), 0, t + Duration::from_millis(2100));
        r.tick(t + Duration::from_secs(3));
        assert_eq!(r.pen_held, None);
    }

    #[test]
    fn a_wobbly_pencil_tap_is_a_click_where_it_touched() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Pen, 1, pos2(100.0, 100.0), 0, t);
        r.pointer(Kind::Move, Device::Pen, 1, pos2(104.0, 102.0), 0, t + Duration::from_millis(30));
        r.pointer(Kind::Up, Device::Pen, 1, pos2(105.0, 103.0), 0, t + Duration::from_millis(60));
        let (ev, _) = r.take();
        let moves: Vec<Pos2> = ev.iter().filter_map(|e| if let Event::PointerMoved(p) = e { Some(*p) } else { None }).collect();
        assert!(moves.iter().all(|p| *p == pos2(100.0, 100.0)), "it never left the spot: {moves:?}");
        assert_eq!(presses(&ev), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);
        assert_eq!(r.device, Device::Pen);
        // A real stroke goes through.
        r.pointer(Kind::Down, Device::Pen, 1, pos2(100.0, 100.0), 0, t);
        r.pointer(Kind::Move, Device::Pen, 1, pos2(130.0, 100.0), 0, t + Duration::from_millis(30));
        r.pointer(Kind::Up, Device::Pen, 1, pos2(160.0, 100.0), 0, t + Duration::from_millis(60));
        assert!(r.take().0.iter().any(|e| *e == Event::PointerMoved(pos2(160.0, 100.0))));
    }

    #[test]
    fn a_held_finger_does_nothing_in_the_view_and_right_clicks_on_a_panel() {
        for over_view in [true, false] {
            let mut r = Router::default();
            let t = Instant::now();
            r.pointer(Kind::Down, Device::Touch, 1, pos2(10.0, 10.0), 0, t);
            r.tick(t + Duration::from_millis(600));
            assert!(presses(&r.take().0).is_empty(), "nothing until the frame says what's under the finger");
            r.after_frame(over_view, false);
            let mut seen = presses(&r.take().0);
            seen.extend(presses(&r.take().0));
            let want = if over_view { vec![] } else { vec![(PointerButton::Secondary, true), (PointerButton::Secondary, false)] };
            assert_eq!(seen, want, "over the view: {over_view}");
            r.pointer(Kind::Up, Device::Touch, 1, pos2(10.0, 10.0), 0, t + Duration::from_millis(700));
            assert!(presses(&r.take().0).is_empty(), "lifting after a hold doesn't click");
        }
    }

    #[test]
    fn a_drag_presses_at_the_start_and_becomes_a_scroll_when_nothing_takes_it() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Touch, 1, pos2(10.0, 10.0), 0, t);
        r.pointer(Kind::Move, Device::Touch, 1, pos2(10.0, 30.0), 0, t);
        let (ev, _) = r.take();
        assert!(matches!(ev.as_slice(), [Event::PointerMoved(p), Event::PointerButton { pressed: true, .. }] if *p == pos2(10.0, 10.0)));
        assert!(r.finger);
        r.after_frame(false, false);
        r.after_frame(false, false);
        assert_eq!(presses(&r.take().0), vec![(PointerButton::Primary, false)]);
        r.pointer(Kind::Move, Device::Touch, 1, pos2(10.0, 50.0), 0, t);
        assert!(r.take().0.iter().any(|e| matches!(e, Event::MouseWheel { delta, .. } if delta.y == 20.0)));
    }

    #[test]
    fn two_fingers_over_the_view_pan_and_zoom() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Touch, 1, pos2(100.0, 100.0), 0, t);
        r.pointer(Kind::Down, Device::Touch, 2, pos2(200.0, 100.0), 0, t);
        r.take();
        r.after_frame(true, false);
        r.pointer(Kind::Move, Device::Touch, 1, pos2(90.0, 110.0), 0, t);
        r.pointer(Kind::Move, Device::Touch, 2, pos2(210.0, 110.0), 0, t);
        let (_, nav) = r.take();
        assert!(nav.iter().any(|n| matches!(n, Nav::Zoom(f, _) if *f > 1.0)), "{nav:?}");
        assert!(nav.iter().any(|n| matches!(n, Nav::Pan(d) if d.y > 0.0)), "{nav:?}");
    }

    #[test]
    fn three_finger_swipes_undo_and_redo_and_taps_do_nothing() {
        // (fingers, sideways travel, expected: None = nothing, Some(shift) = ⌘Z / ⇧⌘Z)
        for (n, dx, want) in [(3, -80.0, Some(false)), (3, 80.0, Some(true)), (3, 0.0, None), (2, 0.0, None), (3, 20.0, None)] {
            let mut r = Router::default();
            let t = Instant::now();
            for i in 0..n {
                r.pointer(Kind::Down, Device::Touch, i, pos2(300.0 + 50.0 * i as f32, 300.0), 0, t);
            }
            r.after_frame(true, false);
            r.take();
            for step in 1..=4 {
                for i in 0..n {
                    let at = pos2(300.0 + 50.0 * i as f32 + dx * step as f32 / 4.0, 300.0);
                    r.pointer(Kind::Move, Device::Touch, i, at, 0, t + Duration::from_millis(30 * step));
                }
            }
            let (_, nav) = r.take();
            assert!(n < 3 || nav.is_empty(), "three fingers never pan or zoom the view");
            for i in 0..n {
                r.pointer(Kind::Up, Device::Touch, i, pos2(300.0 + 50.0 * i as f32 + dx, 300.0), 0, t + Duration::from_millis(200));
            }
            let (ev, _) = r.take();
            let z = ev.iter().find_map(|e| match e {
                Event::Key { key: Key::Z, pressed: true, modifiers, .. } => Some(modifiers.shift),
                _ => None,
            });
            assert_eq!(z, want, "{n} fingers, {dx} sideways");
        }
    }

    #[test]
    fn the_palm_is_ignored_while_the_pencil_draws() {
        let mut r = Router::default();
        let t = Instant::now();
        r.pointer(Kind::Down, Device::Pen, 9, pos2(50.0, 50.0), 0, t);
        r.take();
        r.pointer(Kind::Down, Device::Touch, 1, pos2(300.0, 300.0), 0, t);
        r.pointer(Kind::Up, Device::Touch, 1, pos2(300.0, 300.0), 0, t);
        assert!(presses(&r.take().0).is_empty());
        r.pointer(Kind::Up, Device::Pen, 9, pos2(60.0, 60.0), 0, t);
        assert_eq!(presses(&r.take().0), vec![(PointerButton::Primary, false)]);
    }
}
