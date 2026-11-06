//! The host's few addresses: the page, its web-app manifest and icon, the WebSocket, uploads and downloads.
//! One request per connection; anything but the icon needs the pairing key (in the address or the cookie).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use super::Shared;
use super::session::ToSession;

const PAGE: &str = include_str!("client.html");
const ICON: &[u8] = include_bytes!("../../../../assets/icon/png/shapecraft-256.png");
const COOKIE: &str = "shapecraft_key";
/// The most one upload may be (a big STEP assembly fits easily).
const MAX_UPLOAD: u64 = 2 << 30;

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn param(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?.split(';').filter_map(|c| c.trim().split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v)
    }
}

pub(super) fn serve(stream: TcpStream, shared: &Arc<Shared>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.set_nodelay(true);
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let Some(req) = read_head(&mut reader) else { return };
    let mut out = stream;
    let key = req.param("k").or_else(|| req.cookie(COOKIE)).unwrap_or("");
    let paired = !shared.token.is_empty() && same(key, &shared.token);
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/icon.png" | "/apple-touch-icon.png" | "/favicon.ico") => reply(&mut out, "200 OK", "image/png", &[], ICON),
        ("GET", _) if !paired => {
            reply(&mut out, "403 Forbidden", "text/html; charset=utf-8", &[], NOT_PAIRED.as_bytes());
        }
        ("GET", "/" | "/index.html") => {
            // A year-long cookie, so the key can leave the address bar and a reload still works.
            let cookie = format!("Set-Cookie: {COOKIE}={}; Path=/; Max-Age=31536000; SameSite=Strict; HttpOnly", shared.token);
            reply(&mut out, "200 OK", "text/html; charset=utf-8", &[&cookie, "Cache-Control: no-store"], PAGE.as_bytes());
        }
        ("GET", "/manifest.webmanifest") => {
            let manifest = serde_json::json!({
                "name": "ShapeCraft",
                "short_name": "ShapeCraft",
                // The home-screen app keeps its own cookies, so its start address carries the key.
                "start_url": format!("/?k={}", shared.token),
                "display": "fullscreen",
                "orientation": "any",
                "background_color": "#1c1c1e",
                "theme_color": "#1c1c1e",
                "icons": [{ "src": "/icon.png", "sizes": "256x256", "type": "image/png" }],
            });
            reply(&mut out, "200 OK", "application/manifest+json", &[], manifest.to_string().as_bytes());
        }
        ("GET", "/ws") => upgrade(reader, out, &req, shared),
        ("POST", "/upload") => upload(&mut reader, &mut out, &req, shared),
        ("GET", p) if p.starts_with("/dl/") => download(&mut out, &p[4..], shared),
        _ => reply(&mut out, "404 Not Found", "text/plain", &[], b"Not found"),
    }
}

const NOT_PAIRED: &str = "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'>\
<title>ShapeCraft</title><body style='font:16px -apple-system,system-ui,sans-serif;background:#1c1c1e;color:#ddd;\
display:flex;align-items:center;justify-content:center;height:90vh;text-align:center;padding:0 24px'><div>\
<h2 style='font-weight:600'>Pair this device first</h2><p style='color:#999'>On the computer running ShapeCraft, open \
Preferences ▸ Tablet and scan the code, or type the address shown there (it ends in <code>?k=…</code>).</p></div>";

/// Compares the whole key whatever the first difference, so timing doesn't give it away.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn read_head(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    reader.by_ref().take(8192).read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = Vec::new();
    for _ in 0..100 {
        let mut h = String::new();
        reader.by_ref().take(8192).read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let (path, q) = target.split_once('?').unwrap_or((&target, ""));
    let query = q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), unescape(v))).collect();
    Some(Request { method, path: path.to_string(), query, headers })
}

/// `%20` and `+` in a query value.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| b.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (b[i], hex) {
            (_, Some(v)) => {
                out.push(v);
                i += 2;
            }
            (b'+', None) => out.push(b' '),
            (c, None) => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn reply(out: &mut TcpStream, status: &str, kind: &str, extra: &[&str], body: &[u8]) {
    let mut head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for h in extra {
        head.push_str(h);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    let _ = out.write_all(head.as_bytes());
    let _ = out.write_all(body);
    let _ = out.flush();
}

/// Hands the connection to the drawing thread as a WebSocket (which replaces any page connected before).
fn upgrade(reader: BufReader<TcpStream>, mut out: TcpStream, req: &Request, shared: &Arc<Shared>) {
    let Some(key) = req.header("sec-websocket-key").filter(|_| req.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket")))
    else {
        reply(&mut out, "400 Bad Request", "text/plain", &[], b"WebSocket only");
        return;
    };
    // Nothing should be waiting past the head, but don't lose it if something is.
    if !reader.buffer().is_empty() {
        return;
    }
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let head =
        format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
    if out.write_all(head.as_bytes()).is_err() {
        return;
    }
    let _ = out.set_read_timeout(None);
    if out.set_nonblocking(true).is_err() {
        return;
    }
    let ws = tungstenite::WebSocket::from_raw_socket(out, tungstenite::protocol::Role::Server, None);
    let agent = req.header("user-agent").map(short_agent).unwrap_or_default();
    // Every copy has a page on it: this one is told why, and waits instead of connecting back.
    if let Err(ToSession::Client { mut ws, .. }) = shared.to_session(&device(req), ToSession::Client { ws: Box::new(ws), agent }, true) {
        let _ = ws.get_mut().set_nonblocking(false);
        let _ = ws.send(tungstenite::Message::text(format!("{{\"t\":\"full\",\"n\":{}}}", super::MAX_SESSIONS)));
        let _ = ws.close(None);
        let _ = ws.flush();
    }
}

/// The page's device id (`d=`, kept in its storage), so each device gets its own copy of the app.
fn device(req: &Request) -> String {
    let d: String = req.param("d").unwrap_or_default().chars().filter(char::is_ascii_alphanumeric).take(40).collect();
    if d.is_empty() { "default".into() } else { d }
}

/// "iPad (Safari)" from a long user agent.
fn short_agent(ua: &str) -> String {
    let device = if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Macintosh") {
        // iPadOS Safari asks for desktop pages as a Mac; the page tells them apart by touch (see `hello`).
        "Mac or iPad"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Browser"
    };
    let browser = if ua.contains("CriOS") || ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Firefox") || ua.contains("FxiOS") {
        "Firefox"
    } else if ua.contains("Safari") {
        "Safari"
    } else {
        "browser"
    };
    format!("{device} ({browser})")
}

/// A file the page picked: written to the scratch folder under its own name, then handed to the app.
fn upload(reader: &mut BufReader<TcpStream>, out: &mut TcpStream, req: &Request, shared: &Arc<Shared>) {
    let len = req.header("content-length").and_then(|l| l.parse::<u64>().ok());
    let (Some(len), Some(id)) = (len, req.param("id").and_then(|i| i.parse::<u64>().ok())) else {
        reply(out, "400 Bad Request", "text/plain", &[], b"Missing length or id");
        return;
    };
    if len > MAX_UPLOAD {
        reply(out, "413 Payload Too Large", "text/plain", &[], b"Too large");
        return;
    }
    let name = req.param("name").map(safe_name).unwrap_or_else(|| "upload".to_string());
    let dir = shared.scratch.join("uploads").join(id.to_string());
    let path = dir.join(&name);
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::File::create(&path))
        .and_then(|mut f| std::io::copy(&mut reader.by_ref().take(len), &mut f));
    match written {
        Ok(n) if n == len => {
            reply(out, "200 OK", "application/json", &[], b"{\"ok\":true}");
            let _ = shared.to_session(&device(req), ToSession::Uploaded { request: id, path }, false);
        }
        _ => {
            let _ = std::fs::remove_file(&path);
            reply(out, "500 Internal Server Error", "text/plain", &[], b"Upload failed");
        }
    }
}

/// A file name with no folders in it and nothing a file system minds.
pub(super) fn safe_name(s: &str) -> String {
    let base = s.rsplit(['/', '\\']).next().unwrap_or(s);
    let clean: String = base.chars().map(|c| if c.is_control() || ":*?\"<>|".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() { "file".to_string() } else { clean.chars().take(120).collect() }
}

fn download(out: &mut TcpStream, id: &str, shared: &Arc<Shared>) {
    let path = shared.downloads.lock().ok().and_then(|d| d.iter().find(|(i, _)| i == id).map(|(_, p)| p.clone()));
    let Some(path) = path else {
        reply(out, "404 Not Found", "text/plain", &[], b"Gone");
        return;
    };
    let Ok(mut file) = std::fs::File::open(&path) else {
        reply(out, "404 Not Found", "text/plain", &[], b"Gone");
        return;
    };
    let len = file.metadata().map_or(0, |m| m.len());
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {len}\r\n\
         Content-Disposition: attachment; filename*=UTF-8''{encoded}\r\nConnection: close\r\n\r\n"
    );
    if out.write_all(head.as_bytes()).is_ok() {
        let _ = std::io::copy(&mut file, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_unescape() {
        assert_eq!(unescape("My%20Part+2.step"), "My Part 2.step");
        assert_eq!(unescape("100%"), "100%");
        assert_eq!(unescape("%e2%9c%93"), "✓");
    }

    #[test]
    fn names_lose_their_folders() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\x\\a:b.stl"), "a_b.stl");
        assert_eq!(safe_name(".."), "file");
    }

    #[test]
    fn keys_compare_whole() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
    }

    #[test]
    fn agents_read_short() {
        assert_eq!(short_agent("Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) AppleWebKit Version/18.0 Safari/604.1"), "iPad (Safari)");
        assert_eq!(short_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) CriOS/120 Safari"), "iPhone (Chrome)");
    }
}
