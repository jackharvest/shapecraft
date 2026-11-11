//! The tablet's copy of the app: its frames, the page's input, and its files.

use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::App as _;
use egui::{Event, Key, Modifiers, Pos2, Rect, ViewportId, pos2, vec2};
use serde_json::{Value, json};
use shapecraft_kernel::Kernel;
use tungstenite::{Message, WebSocket};

use super::frames::{Painter, Stream, message};
use super::touch::{Device, Kind, Nav, Router, command_mods};
use super::{Shared, set_status};
use crate::{Options, ShapeCraftApp};

pub(crate) enum ToSession {
    Client {
        ws: Box<WebSocket<TcpStream>>,
        agent: String,
    },
    /// The page sent the file a picker asked for (`request` is the picker's id).
    Uploaded {
        request: u64,
        path: PathBuf,
    },
    /// Too many devices: this copy saves and ends to make room.
    End,
}

/// The smallest the desktop layout is drawn at (points); smaller screens see it scaled down to fit.
pub(super) const MIN_SIZE: [f32; 2] = [1000.0, 640.0];
/// Android draws everything this much bigger when there's room, never leaving less than `MIN_SIZE`: one point
/// becomes 1.21 dp, the size of an iPad point (132 per inch against Android's 160), so a tablet shows the interface
/// at the size it has on an iPad. The tablet page needs nothing: a CSS pixel on an iPad is already one point.
pub(super) const TOUCH_ZOOM: f32 = 1.21;
/// How far edges and curves reach for the pencil and a finger, against a mouse's (6 points for edges).
pub(super) const PEN_REACH: f32 = 2.0;
pub(super) const FINGER_REACH: f32 = 2.6;
/// A frame the page hasn't shown yet holds up the next one past this many.
const IN_FLIGHT: u32 = 2;
/// How long the screen is still before JPEG parts are sent again as PNG.
const SETTLE: Duration = Duration::from_millis(220);

struct Client {
    ws: Box<WebSocket<TcpStream>>,
    /// The page's size in CSS pixels and its device pixel ratio.
    size: Option<(f32, f32, f32)>,
    in_flight: u32,
    seq: u32,
    /// What the page was last told (cursor, keyboard…), to send only changes.
    state: Value,
}

impl Client {
    fn send(&mut self, m: Message) -> bool {
        match self.ws.write(m) {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return false,
        }
        flush(&mut self.ws)
    }

    /// Points per CSS pixel: 1, or more on a screen too small for the layout.
    fn scale(&self) -> f32 {
        self.size.map_or(1.0, |(w, h, _)| (MIN_SIZE[0] / w.max(1.0)).max(MIN_SIZE[1] / h.max(1.0)).max(1.0))
    }
}

fn flush(ws: &mut WebSocket<TcpStream>) -> bool {
    match ws.flush() {
        Ok(()) => true,
        Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => true,
        Err(_) => false,
    }
}

/// Asks to draw again at a time; the drawing loop reads it.
#[derive(Default)]
struct Wake {
    at: Mutex<Option<Instant>>,
}

impl Wake {
    fn at(&self, t: Instant) {
        if let Ok(mut a) = self.at.lock() {
            *a = Some(a.map_or(t, |old| old.min(t)));
        }
    }

    fn due(&self, now: Instant) -> bool {
        self.at.lock().is_ok_and(|a| a.is_some_and(|t| t <= now))
    }

    fn clear(&self) {
        if let Ok(mut a) = self.at.lock() {
            *a = None;
        }
    }
}

fn gpu() -> Result<egui_wgpu::RenderState, String> {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends.remove(wgpu::Backends::BROWSER_WEBGPU);
    let setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    let instance = pollster_block_on(setup.new_instance());
    let config = egui_wgpu::WgpuConfiguration { wgpu_setup: setup, ..Default::default() };
    pollster_block_on(egui_wgpu::RenderState::create(&config, &instance, None, egui_wgpu::RendererOptions::default()))
        .map_err(|e| format!("no graphics device for the tablet: {e}"))
}

/// Runs a future to its end on this thread (wgpu's setup only waits on the driver).
fn pollster_block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::task::{Context, Poll, Wake as TaskWake, Waker};
    struct Thread(std::thread::Thread);
    impl TaskWake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

pub(super) fn run(shared: &Arc<Shared>, device: &str, inbox: Receiver<ToSession>, kernel: Arc<dyn Kernel>, opts: Options) {
    let rs = match gpu() {
        Ok(rs) => rs,
        Err(e) => {
            set_status(|s| s.error = Some(e));
            return;
        }
    };
    let ctx = egui::Context::default();
    let wake = Arc::new(Wake::default());
    {
        let wake = wake.clone();
        ctx.set_request_repaint_callback(move |info| wake.at(Instant::now() + info.delay));
    }
    let mut frame = eframe::Frame::_new_kittest();
    frame.wgpu_render_state = Some(rs.clone());
    let mut app = ShapeCraftApp::new(kernel, opts, &ctx);
    let mut painter = Painter::new(rs.clone());
    let mut stream = Stream::default();
    let mut router = Router::default();
    let mut client: Option<Client> = None;
    let started = Instant::now();
    let mut last_pass = Instant::now() - Duration::from_secs(10);
    let mut last_change = Instant::now();
    let mut uploads_dir_seq = 0u64;
    // `SHAPECRAFT_TABLET_TRACE=1`: each frame's timings on stderr, for tuning.
    let trace = std::env::var_os("SHAPECRAFT_TABLET_TRACE").is_some();
    let mut ending = false;
    loop {
        if shared.stopping() || ending {
            app.on_exit();
            if let Some(c) = client.as_mut() {
                let _ = c.ws.close(None);
                flush(&mut c.ws);
            }
            return;
        }
        let now = Instant::now();
        let mut input = false;
        loop {
            match inbox.try_recv() {
                Ok(ToSession::Client { ws, agent }) => {
                    // The same device again (another tab, the home-screen app): the new page takes over, and the old
                    // one is told so it waits instead of connecting back.
                    if let Some(mut old) = client.take() {
                        let _ = old.send(Message::text(json!({ "t": "taken" }).to_string()));
                        let _ = old.ws.close(None);
                        flush(&mut old.ws);
                    }
                    client = Some(Client { ws, size: None, in_flight: 0, seq: 0, state: json!({}) });
                    stream.reset();
                    router.reset();
                    shared.connected(device, Some(agent));
                }
                Ok(ToSession::End) => ending = true,
                Ok(ToSession::Uploaded { request, path }) => {
                    app.tablet_file_arrived(request, path);
                    input = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        // The page's messages.
        let mut lost = false;
        if let Some(c) = client.as_mut() {
            loop {
                match c.ws.read() {
                    Ok(Message::Text(t)) => {
                        let scale = c.scale();
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            input |= handle(&v, c, &mut router, scale, now);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        lost = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        lost = true;
                        break;
                    }
                }
            }
            lost |= !flush(&mut c.ws);
        }
        if lost {
            client = None;
            router.reset();
            shared.connected(device, None);
        }
        router.tick(now);

        let ready = client.as_ref().is_some_and(|c| c.size.is_some() && c.in_flight < IN_FLIGHT);
        let due = input || router.busy() || wake.due(now);
        // With no page, the app still runs now and then: autosave, the projects folder, a file that came in.
        let idle_pass = client.is_none() && now.duration_since(last_pass) >= Duration::from_secs(1);
        if (ready && due) || idle_pass {
            wake.clear();
            last_pass = now;
            let (events, nav) = router.take();
            if let Some(ws) = app.workspace() {
                ws.touch_drag = router.finger;
                ws.finger_nav = router.device == Device::Touch;
                ws.pen_input = router.device == Device::Pen;
                if ws.pen_input {
                    super::mark_pen_seen();
                }
                ws.pen_held = router.pen_held;
                // Edges, curves and snaps reach about a fingertip for the pencil (and more for a finger).
                ws.pick_slop = match router.device {
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
            let (w, h, dpr, scale) =
                client.as_ref().and_then(|c| c.size.map(|s| (s.0, s.1, s.2, c.scale()))).unwrap_or((1180.0, 820.0, 2.0, 1.0));
            let points = vec2(w * scale, h * scale);
            let ppp = dpr / scale;
            let mut raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, points)),
                time: Some(started.elapsed().as_secs_f64()),
                predicted_dt: 1.0 / 60.0,
                events,
                focused: true,
                max_texture_side: Some(rs.device.limits().max_texture_dimension_2d as usize),
                ..Default::default()
            };
            if let Some(v) = raw.viewports.get_mut(&ViewportId::ROOT) {
                v.native_pixels_per_point = Some(ppp);
                v.focused = Some(true);
                v.inner_rect = Some(Rect::from_min_size(Pos2::ZERO, points));
                v.monitor_size = Some(points);
            }
            let mut out = ctx.run_ui(raw, |ui| {
                app.logic(ui.ctx(), &mut frame);
                app.ui(ui, &mut frame);
            });
            let over_view = app.workspace().is_some_and(|ws| ws.viewport_hot);
            // The page's number keyboard, unless our own numpad is up.
            let numbers = app.workspace().is_some_and(|ws| ws.takes_numbers() && !ws.numpad_open());
            router.after_frame(over_view, ctx.dragged_id().is_some());
            if let Some(v) = out.viewport_output.get(&ViewportId::ROOT)
                && v.repaint_delay < Duration::from_secs(3600)
            {
                wake.at(now + v.repaint_delay);
            }
            let requests = app.take_tablet_requests();
            match client.as_mut() {
                Some(c) if !idle_pass => {
                    let px = [(w * dpr).round().max(1.0) as u32, (h * dpr).round().max(1.0) as u32];
                    let t_ui = now.elapsed();
                    let prims = ctx.tessellate(out.shapes, ppp);
                    match painter.paint(&prims, &out.textures_delta, px, ppp) {
                        Ok(rgba) => {
                            let t_paint = now.elapsed();
                            let pieces = stream.update(rgba, px);
                            if trace {
                                let bytes: usize = pieces.iter().map(|p| p.bytes.len()).sum();
                                eprintln!(
                                    "tablet frame: ui {:.1} ms, paint+read {:.1} ms, encode {:.1} ms, {} pieces, {} KB",
                                    t_ui.as_secs_f64() * 1e3,
                                    (t_paint - t_ui).as_secs_f64() * 1e3,
                                    (now.elapsed() - t_paint).as_secs_f64() * 1e3,
                                    pieces.len(),
                                    bytes / 1024
                                );
                            }
                            if !pieces.is_empty() {
                                c.seq = c.seq.wrapping_add(1);
                                c.in_flight += 1;
                                last_change = now;
                                lost |= !c.send(Message::Binary(message(c.seq, &pieces).into()));
                            }
                        }
                        Err(e) => set_status(|s| s.error = Some(e)),
                    }
                    lost |= !platform(c, &out.platform_output, &ctx, scale, numbers);
                    for r in requests {
                        lost |= !file_request(c, r, shared, &mut uploads_dir_seq);
                    }
                }
                _ => painter.textures_only(&out.textures_delta),
            }
            // Applied above (the GPU has them); egui checks that nothing was dropped unseen.
            out.textures_delta.clear();
        } else if let Some(c) = client.as_mut()
            && c.in_flight == 0
            && stream.has_lossy()
            && now.duration_since(last_change) >= SETTLE
        {
            let pieces = stream.refine();
            if !pieces.is_empty() {
                c.seq = c.seq.wrapping_add(1);
                c.in_flight += 1;
                lost |= !c.send(Message::Binary(message(c.seq, &pieces).into()));
            }
        } else {
            std::thread::sleep(Duration::from_millis(if client.is_some() { 2 } else { 20 }));
        }
        if lost {
            client = None;
            router.reset();
            shared.connected(device, None);
        }
    }
}

/// Cursor, keyboard and clipboard for the page, sent when they change.
fn platform(c: &mut Client, out: &egui::PlatformOutput, ctx: &egui::Context, scale: f32, numbers: bool) -> bool {
    let mut ok = true;
    for cmd in &out.commands {
        match cmd {
            egui::OutputCommand::CopyText(s) => ok &= c.send(Message::text(json!({ "t": "copy", "s": s }).to_string())),
            egui::OutputCommand::OpenUrl(u) => ok &= c.send(Message::text(json!({ "t": "open", "url": u.url }).to_string())),
            _ => {}
        }
    }
    let typing = ctx.egui_wants_keyboard_input() || out.ime.is_some();
    let caret = out.ime.as_ref().map(|i| [i.cursor_rect.min.x / scale, i.cursor_rect.max.y / scale]);
    let state = json!({ "t": "state", "cursor": cursor(out.cursor_icon), "kbd": typing, "num": numbers, "caret": caret });
    if state != c.state {
        ok &= c.send(Message::text(state.to_string()));
        c.state = state;
    }
    ok
}

fn cursor(icon: egui::CursorIcon) -> &'static str {
    use egui::CursorIcon as C;
    match icon {
        C::PointingHand => "pointer",
        C::Text => "text",
        C::Crosshair => "crosshair",
        C::Grab => "grab",
        C::Grabbing => "grabbing",
        C::Move | C::AllScroll => "move",
        C::ResizeHorizontal | C::ResizeColumn | C::ResizeEast | C::ResizeWest => "ew-resize",
        C::ResizeVertical | C::ResizeRow | C::ResizeNorth | C::ResizeSouth => "ns-resize",
        C::ResizeNeSw | C::ResizeNorthEast | C::ResizeSouthWest => "nesw-resize",
        C::ResizeNwSe | C::ResizeNorthWest | C::ResizeSouthEast => "nwse-resize",
        C::NotAllowed | C::NoDrop => "not-allowed",
        C::Wait | C::Progress => "wait",
        C::None => "none",
        _ => "default",
    }
}

/// Tells the page about a file to pick or to save.
fn file_request(c: &mut Client, r: crate::app::TabletRequest, shared: &Arc<Shared>, seq: &mut u64) -> bool {
    match r {
        crate::app::TabletRequest::Pick { id, label, accept } => {
            let accept: Vec<String> = accept.iter().map(|e| format!(".{e}")).collect();
            c.send(Message::text(json!({ "t": "pick", "id": id, "label": label, "accept": accept.join(",") }).to_string()))
        }
        crate::app::TabletRequest::Saved(dir) => {
            let mut ok = true;
            let mut files: Vec<PathBuf> =
                std::fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
            files.sort();
            for path in files {
                *seq += 1;
                let id = format!("{}{}", *seq, super::new_token());
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Ok(mut d) = shared.downloads.lock() {
                    d.push((id.clone(), path));
                }
                ok &= c.send(Message::text(json!({ "t": "download", "url": format!("/dl/{id}"), "name": name }).to_string()));
            }
            ok
        }
    }
}

/// One message from the page. True when it was input for the app.
fn handle(v: &Value, c: &mut Client, router: &mut Router, scale: f32, now: Instant) -> bool {
    let num = |k: &str| v.get(k).and_then(Value::as_f64).filter(|n| n.is_finite());
    let at = |x: f64, y: f64| pos2(x as f32 * scale, y as f32 * scale);
    match v.get("t").and_then(Value::as_str).unwrap_or("") {
        "hello" | "size" => {
            let (Some(w), Some(h)) = (num("w"), num("h")) else { return false };
            let dpr = num("dpr").unwrap_or(2.0).clamp(1.0, 4.0) as f32;
            let (w, h) = (w.clamp(200.0, 4096.0) as f32, h.clamp(200.0, 4096.0) as f32);
            // A frame's pixels have to fit a texture: big monitors at 3× come down a little.
            let dpr = dpr.min(8192.0 / w).min(8192.0 / h);
            c.size = Some((w, h, dpr));
            true
        }
        "ack" => {
            c.in_flight = c.in_flight.saturating_sub(1);
            false
        }
        "p" => {
            router.mods = mods(v.get("m"));
            let Some(list) = v.get("e").and_then(Value::as_array) else { return false };
            for e in list.iter().take(512) {
                let n = |i: usize| e.get(i).and_then(Value::as_f64).filter(|n| n.is_finite());
                let (Some(kind), Some(id), Some(dev), Some(x), Some(y)) = (n(0), n(1), n(2), n(3), n(4)) else { continue };
                let kind = match kind as u8 {
                    0 => Kind::Down,
                    1 => Kind::Move,
                    2 => Kind::Up,
                    3 => Kind::Cancel,
                    4 => Kind::Hover,
                    _ => Kind::Leave,
                };
                let dev = match dev as u8 {
                    1 => Device::Pen,
                    2 => Device::Touch,
                    _ => Device::Mouse,
                };
                router.pointer(kind, dev, id as i64, at(x, y), n(5).unwrap_or(0.0) as u8, now);
            }
            true
        }
        "wheel" => {
            let (Some(x), Some(y)) = (num("x"), num("y")) else { return false };
            let m = mods(v.get("m"));
            router.mods = m;
            let (dx, dy) = (num("dx").unwrap_or(0.0) as f32, num("dy").unwrap_or(0.0) as f32);
            router.key(Event::PointerMoved(at(x, y)));
            if v.get("pinch").and_then(Value::as_bool).unwrap_or(false) {
                router.key(Event::Zoom((-dy * 0.01).exp()));
            } else {
                let lines = num("mode").unwrap_or(0.0) as u8 == 1;
                router.key(Event::MouseWheel {
                    unit: if lines { egui::MouseWheelUnit::Line } else { egui::MouseWheelUnit::Point },
                    delta: vec2(-dx, -dy),
                    phase: egui::TouchPhase::Move,
                    source: if lines { egui::MouseWheelSource::Wheel } else { egui::MouseWheelSource::Trackpad },
                    modifiers: m,
                });
            }
            true
        }
        "zoom" => {
            if let (Some(f), Some(x), Some(y)) = (num("f"), num("x"), num("y")) {
                router.key(Event::PointerMoved(at(x, y)));
                router.key(Event::Zoom((f as f32).clamp(0.5, 2.0)));
            }
            true
        }
        "key" => {
            let m = mods(v.get("m"));
            router.mods = m;
            let pressed = v.get("down").and_then(Value::as_bool).unwrap_or(true);
            let code = v.get("code").and_then(Value::as_str).unwrap_or("");
            let name = v.get("key").and_then(Value::as_str).unwrap_or("");
            let Some(key) = egui_key(code, name) else { return false };
            if pressed && m.command {
                match key {
                    Key::C => router.key(Event::Copy),
                    Key::X => router.key(Event::Cut),
                    _ => {}
                }
            }
            let repeat = v.get("rep").and_then(Value::as_bool).unwrap_or(false);
            router.key(Event::Key { key, physical_key: Key::from_name(code), pressed, repeat, modifiers: m });
            true
        }
        "text" => {
            if let Some(s) = v.get("s").and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() < 4096) {
                router.key(Event::Text(s.to_string()));
            }
            true
        }
        "paste" => {
            if let Some(s) = v.get("s").and_then(Value::as_str).filter(|s| s.len() < 1 << 20) {
                router.key(Event::Paste(s.to_string()));
            }
            true
        }
        _ => false,
    }
}

/// The page's `[shift, ctrl, alt, meta]`. The tablet's ⌘ is the app's command key whatever the host.
fn mods(v: Option<&Value>) -> Modifiers {
    let b = |i: usize| v.and_then(|v| v.get(i)).and_then(Value::as_bool).unwrap_or(false);
    let mut m = if b(3) { command_mods() } else { Modifiers::NONE };
    m.shift = b(0);
    m.alt = b(2);
    if b(1) {
        m.ctrl = true;
        if !cfg!(target_os = "macos") {
            m.command = true;
        }
    }
    m
}

/// A key from the page: by position for letters and digits (so ⌥ and layouts don't change them), else by name.
fn egui_key(code: &str, name: &str) -> Option<Key> {
    let by_code =
        (code.starts_with("Key") || code.starts_with("Digit") || code.starts_with("Numpad")).then(|| Key::from_name(code)).flatten();
    let alias = match name {
        "Esc" => Some(Key::Escape),
        "Del" => Some(Key::Delete),
        _ => None,
    };
    by_code.or_else(|| Key::from_name(name)).or(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_by_position_then_name() {
        assert_eq!(egui_key("KeyZ", "Ω"), Some(Key::Z));
        assert_eq!(egui_key("Digit5", "%"), Some(Key::Num5));
        assert_eq!(egui_key("Enter", "Enter"), Some(Key::Enter));
        assert_eq!(egui_key("", "Escape"), Some(Key::Escape));
        assert_eq!(egui_key("Minus", "-"), Some(Key::Minus));
    }

    #[test]
    fn the_tablets_command_key_is_the_apps() {
        let m = mods(Some(&json!([true, false, false, true])));
        assert!(m.command && m.shift);
        assert!(!mods(Some(&json!([false, false, false, false]))).any());
    }
}
