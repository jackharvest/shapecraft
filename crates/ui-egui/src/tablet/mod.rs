//! Tablet access: ShapeCraft in a browser on the same network (iPad Safari first, then Chromium).
//!
//! A second copy of the app runs offscreen on a thread of its own, with a GPU device that has no window. A small page
//! (`client.html`) shows its frames and sends pen, touch, mouse and key input back over a WebSocket:
//! - frames: only the 64 px tiles that changed are sent. They go out as JPEG while things move, then as PNG once the
//!   screen has been still for a moment, so text ends up exact at the tablet's own pixel density;
//! - input: the pencil is a precise mouse that hovers; fingers turn, pan and pinch the view, tap to pick, two- and
//!   three-finger taps undo and redo, and a long press is a right-click (see `touch`).
//!
//! Preferences ▸ Tablet turns it on from the desktop app; `shapecraft --serve` runs it with no window at all (a
//! server, a VM, an LXC container). The page needs the pairing key from the link (or the QR code), kept afterwards
//! in a cookie and in the home-screen web app's start address.

mod frames;
mod http;
mod session;
mod touch;

use std::cell::Cell;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use shapecraft_kernel::Kernel;

use crate::Options;

/// The port asked for when nothing else is set (Preferences ▸ Tablet, `--serve`).
pub const DEFAULT_PORT: u16 = 8780;

thread_local! {
    static ON_TABLET: Cell<bool> = const { Cell::new(false) };
}

/// This thread draws the tablet's copy of the app (no system menu bar, no window buttons, files through the page).
pub fn on_tablet() -> bool {
    ON_TABLET.with(Cell::get)
}

/// What Preferences ▸ Tablet shows about the running host.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub port: u16,
    /// Addresses to type or scan, with the pairing key: this computer's name first, then its network address.
    pub urls: Vec<String>,
    /// The browsers connected now (their user agents, shortened), one per device.
    pub clients: Vec<String>,
    /// Why the host stopped or couldn't start.
    pub error: Option<String>,
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);

/// The host's state as last reported (None: not running).
pub fn status() -> Option<Status> {
    STATUS.lock().ok().and_then(|s| s.clone())
}

fn set_status(f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = STATUS.lock()
        && let Some(s) = s.as_mut()
    {
        f(s);
    }
}

/// How many devices get a copy of the app each at once. One more ends the copy idle the longest.
const MAX_SESSIONS: usize = 4;

/// One device's copy of the app: its thread and the way to reach it.
struct Slot {
    to: Sender<session::ToSession>,
    thread: Option<JoinHandle<()>>,
    /// Shown in Preferences while a page is connected.
    agent: Option<String>,
    /// When a page last connected or left (the idle one longest goes first when there are too many).
    since: std::time::Instant,
}

/// What the connections and the drawing threads share.
pub(crate) struct Shared {
    stop: AtomicBool,
    token: String,
    /// Each device (the page's own id, kept in its storage) gets its own copy of the app.
    sessions: Mutex<Vec<(String, Slot)>>,
    kernel: Arc<dyn Kernel>,
    opts: Options,
    /// Files offered to the page for download, by the id in their address.
    downloads: Mutex<Vec<(String, std::path::PathBuf)>>,
    /// Where uploads and downloads are kept while the host runs.
    scratch: std::path::PathBuf,
}

impl Shared {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Sends to a device's copy of the app, starting it first if `start` (a page connecting). The message comes back
    /// when there's none to send to (every copy has a page on it).
    fn to_session(self: &Arc<Self>, device: &str, msg: session::ToSession, start: bool) -> Result<(), session::ToSession> {
        let Ok(mut list) = self.sessions.lock() else { return Err(msg) };
        // Copies that ended (a panic, no GPU) make room.
        list.retain(|(_, s)| s.thread.as_ref().is_some_and(|t| !t.is_finished()));
        let msg = match list.iter().find(|(d, _)| d == device) {
            Some((_, slot)) => match slot.to.send(msg) {
                Ok(()) => return Ok(()),
                Err(e) => e.0,
            },
            None => msg,
        };
        if !start {
            return Err(msg);
        }
        list.retain(|(d, _)| d != device);
        if list.len() >= MAX_SESSIONS {
            // The copy with no page for the longest goes; it saves as the app does when it quits.
            let Some(k) = list.iter().enumerate().filter(|(_, (_, s))| s.agent.is_none()).min_by_key(|(_, (_, s))| s.since).map(|(k, _)| k)
            else {
                return Err(msg);
            };
            let (_, old) = list.remove(k);
            let _ = old.to.send(session::ToSession::End);
        }
        let (to, inbox) = std::sync::mpsc::channel();
        let _ = to.send(msg);
        let sh = self.clone();
        let id = device.to_string();
        let thread = std::thread::Builder::new().name("tablet-app".into()).spawn(move || {
            ON_TABLET.with(|t| t.set(true));
            // A failure here ends this device's copy, not the app that serves it.
            let (kernel, opts) = (sh.kernel.clone(), sh.opts.clone());
            let run = std::panic::AssertUnwindSafe(|| session::run(&sh, &id, inbox, kernel, opts));
            if std::panic::catch_unwind(run).is_err() {
                set_status(|s| s.error = Some("A tablet's copy of ShapeCraft stopped. Reload the page to start it again.".into()));
            }
            sh.connected(&id, None);
        });
        let Ok(thread) = thread else { return Ok(()) };
        list.push((device.to_string(), Slot { to, thread: Some(thread), agent: None, since: std::time::Instant::now() }));
        Ok(())
    }

    /// A device's page came (its agent) or went (None): Preferences lists the connected ones.
    fn connected(&self, device: &str, agent: Option<String>) {
        let Ok(mut list) = self.sessions.lock() else { return };
        if let Some((_, s)) = list.iter_mut().find(|(d, _)| d == device) {
            s.agent = agent;
            s.since = std::time::Instant::now();
        }
        let clients = list.iter().filter_map(|(_, s)| s.agent.clone()).collect();
        set_status(|s| s.clients = clients);
    }
}

/// A running host. Dropping it stops it.
pub struct Host {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    pub port: u16,
}

impl Host {
    /// Listens on `bind` and starts the tablet's copy of the app. `opts` are the desktop app's (same projects folder
    /// and preferences); the copy gets no control channel and no native menu.
    pub fn start(bind: SocketAddr, token: String, kernel: Arc<dyn Kernel>, opts: Options) -> std::io::Result<Host> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let scratch = std::env::temp_dir().join(format!("shapecraft-tablet-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&scratch);
        let opts = Options { control_port: None, native_menu: false, open: None, new_project: false, hosted: true, ..opts };
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            token: token.clone(),
            sessions: Mutex::new(Vec::new()),
            kernel,
            opts,
            downloads: Mutex::new(Vec::new()),
            scratch,
        });
        if let Ok(mut s) = STATUS.lock() {
            *s = Some(Status { port, urls: urls(bind, port, &token), clients: Vec::new(), error: None });
        }
        let mut threads = Vec::new();
        let sh = shared.clone();
        threads.push(std::thread::Builder::new().name("tablet-listen".into()).spawn(move || accept(&listener, &sh))?);
        Ok(Host { shared, threads, port })
    }

    /// Runs until the process ends (`--serve`).
    pub fn wait(mut self) {
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        // Each device's copy saves and ends (the listener is gone, so no new ones start).
        let copies: Vec<JoinHandle<()>> =
            self.shared.sessions.lock().map(|mut l| l.iter_mut().filter_map(|(_, s)| s.thread.take()).collect()).unwrap_or_default();
        for t in copies {
            let _ = t.join();
        }
        let _ = std::fs::remove_dir_all(&self.shared.scratch);
        if let Ok(mut s) = STATUS.lock() {
            *s = None;
        }
    }
}

fn accept(listener: &TcpListener, shared: &Arc<Shared>) {
    while !shared.stopping() {
        match listener.accept() {
            Ok((stream, _)) => {
                let sh = shared.clone();
                let _ = std::thread::Builder::new().name("tablet-conn".into()).spawn(move || http::serve(stream, &sh));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

/// A file name from elsewhere, made safe to write: no folders, nothing a file system minds.
pub fn safe_file_name(name: &str) -> String {
    http::safe_name(name)
}

/// A fresh pairing key: 12 characters a person can read off a screen if they have to (no 0/o, 1/l).
pub fn new_token() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    let mut seed = [0u8; 12];
    let from_os = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut seed));
    if from_os.is_err() {
        // Windows: the standard library's per-process random hash keys, mixed with the clock.
        use std::hash::{BuildHasher, Hasher};
        for (i, b) in seed.iter_mut().enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_usize(i);
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
            *b = h.finish() as u8;
        }
    }
    seed.iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect()
}

/// The addresses a tablet can reach this host at, each with the key.
fn urls(bind: SocketAddr, port: u16, token: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    if bind.ip().is_loopback() {
        hosts.push("localhost".to_string());
    } else {
        if let Some(name) = host_name() {
            hosts.push(name);
        }
        if let Some(ip) = lan_ip() {
            hosts.push(ip);
        }
    }
    hosts.into_iter().map(|h| format!("http://{h}:{port}/?k={token}")).collect()
}

/// This computer's network name (`name.local` on a Mac, as Bonjour answers it).
fn host_name() -> Option<String> {
    let out = std::process::Command::new("hostname").output().ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some(if cfg!(target_os = "macos") && !name.contains('.') { format!("{name}.local") } else { name })
}

/// The address other devices on the network see: the one a datagram to a private address would leave from.
/// Nothing is sent.
fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("10.254.254.254:1").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_readable_and_differ() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 12);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric() && !"01lo".contains(c)));
        assert_ne!(a, b);
    }

    /// One HTTP GET, the status line back.
    fn get(port: u16, path: &str) -> String {
        use std::io::{Read, Write};
        let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) else { return String::new() };
        let _ = write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n");
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        String::from_utf8_lossy(&out).lines().next().unwrap_or("").to_string()
    }

    #[test]
    fn a_paired_page_gets_frames_and_an_unpaired_one_nothing() {
        let projects = std::env::temp_dir().join(format!("shapecraft-tablet-test-{}", std::process::id()));
        let opts = Options {
            projects_root: projects.clone(),
            control_port: None,
            open: None,
            new_project: false,
            native_menu: false,
            settings_file: None,
            hosted: false,
        };
        let kernel = Arc::new(shapecraft_kernel_occt::OcctKernel::new());
        let host = Host::start("127.0.0.1:0".parse().unwrap(), "testkey".into(), kernel, opts).unwrap();
        let port = host.port;
        assert!(get(port, "/").contains("403"), "no key, no page");
        assert!(get(port, "/?k=wrong").contains("403"));
        assert!(get(port, "/ws").contains("403"));
        assert!(get(port, "/?k=testkey").contains("200"));
        assert!(get(port, "/manifest.webmanifest?k=testkey").contains("200"));
        assert!(get(port, "/icon.png").contains("200"), "the home-screen icon needs no key");

        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
        let (mut ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}/ws?k=testkey"), stream).unwrap();
        ws.send(tungstenite::Message::text(r#"{"t":"hello","w":800,"h":600,"dpr":1}"#)).unwrap();
        // Without a graphics device (some CI machines) the copy says so instead of drawing.
        let frame = loop {
            match ws.read() {
                Ok(tungstenite::Message::Binary(b)) => break Some(b),
                Ok(_) => continue,
                Err(_) => break None,
            }
        };
        match frame {
            Some(b) => {
                assert_eq!(b[0], 1, "a frame message");
                let count = u16::from_le_bytes([b[5], b[6]]);
                assert!(count > 0, "the first frame covers the screen");
            }
            None => assert!(status().and_then(|s| s.error).is_some(), "no frame and no reason given"),
        }
        drop(ws);
        drop(host);
        assert!(status().is_none(), "stopped hosts leave no status behind");
        let _ = std::fs::remove_dir_all(projects);
    }

    /// Two devices each get their own copy; the same device again takes over its own and the old page is told so
    /// (it waits instead of reconnecting, so two tabs don't fight).
    #[test]
    fn each_device_gets_its_own_copy_and_a_second_tab_takes_over() {
        let projects = std::env::temp_dir().join(format!("shapecraft-tablet-test2-{}", std::process::id()));
        let opts = Options {
            projects_root: projects.clone(),
            control_port: None,
            open: None,
            new_project: false,
            native_menu: false,
            settings_file: None,
            hosted: false,
        };
        let kernel = Arc::new(shapecraft_kernel_occt::OcctKernel::new());
        let host = Host::start("127.0.0.1:0".parse().unwrap(), "testkey".into(), kernel, opts).unwrap();
        let port = host.port;
        let open = |d: &str| {
            let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(20))).unwrap();
            let (mut ws, _) = tungstenite::client(format!("ws://127.0.0.1:{port}/ws?d={d}&k=testkey"), stream).unwrap();
            ws.send(tungstenite::Message::text(r#"{"t":"hello","w":800,"h":600,"dpr":1}"#)).unwrap();
            ws
        };
        let first_frame = |ws: &mut tungstenite::WebSocket<std::net::TcpStream>| loop {
            match ws.read() {
                Ok(tungstenite::Message::Binary(_)) => break true,
                Ok(_) => continue,
                Err(_) => break false,
            }
        };
        let mut a = open("ipad");
        if !first_frame(&mut a) {
            return; // no graphics device here (the other test checks the reason is given)
        }
        let mut b = open("phone");
        assert!(first_frame(&mut b), "a second device gets frames of its own");
        assert_eq!(host.shared.sessions.lock().unwrap().len(), 2);
        // (The status itself is one for the process, shared with the other test.)
        assert_eq!(host.shared.sessions.lock().unwrap().iter().filter(|(_, s)| s.agent.is_some()).count(), 2, "both listed");
        let _a2 = open("ipad");
        let told = loop {
            match a.read() {
                Ok(tungstenite::Message::Text(t)) if t.contains("taken") => break true,
                Ok(_) => continue,
                Err(_) => break false,
            }
        };
        assert!(told, "the old tab hears it was taken over");
        assert_eq!(host.shared.sessions.lock().unwrap().len(), 2, "same device, same copy");
        drop((a, b));
        drop(host);
        let _ = std::fs::remove_dir_all(projects);
    }

    #[test]
    fn loopback_hosts_say_localhost() {
        let u = urls("127.0.0.1:8780".parse().unwrap(), 8780, "abc");
        assert_eq!(u, vec!["http://localhost:8780/?k=abc".to_string()]);
    }
}
