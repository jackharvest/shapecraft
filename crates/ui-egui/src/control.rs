//! The control channel: JSON lines on a local TCP port, so scripts, tests and the MCP bridge can drive
//! the running app.
//!
//! Request: `{"id": 1, "method": "engine.execute", "params": {"command": "rectangle", "params": {...}}}`
//! Reply:   `{"id": 1, "result": ...}` or `{"id": 1, "error": "..."}`

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use serde_json::{Value, json};

pub struct Request {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Result<Value, String>>,
}

pub struct Server {
    pub port: u16,
    pub requests: Receiver<Request>,
}

const MAX_LINE: usize = 4 * 1024 * 1024;

impl Server {
    /// Listens on 127.0.0.1 only. `wake` is called after each request so the UI picks it up promptly.
    pub fn start(port: u16, wake: impl Fn() + Send + Sync + Clone + 'static) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let (tx, rx) = mpsc::channel();
        thread::Builder::new().name("control-accept".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                let wake = wake.clone();
                let _ = thread::Builder::new().name("control-conn".into()).spawn(move || serve(stream, tx, wake));
            }
        })?;
        Ok(Server { port, requests: rx })
    }
}

fn serve(stream: TcpStream, tx: Sender<Request>, wake: impl Fn()) {
    let Ok(mut out) = stream.try_clone() else { return };
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.len() > MAX_LINE {
            let _ = writeln!(out, "{}", json!({ "id": null, "error": "request too large" }));
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(req) => {
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                let params = req.get("params").cloned().unwrap_or(Value::Null);
                let (rtx, rrx) = mpsc::channel();
                if tx.send(Request { method, params, reply: rtx }).is_err() {
                    return;
                }
                wake();
                match rrx.recv() {
                    Ok(Ok(result)) => json!({ "id": id, "result": result }),
                    Ok(Err(e)) => json!({ "id": id, "error": e }),
                    Err(_) => json!({ "id": id, "error": "the app closed" }),
                }
            }
            Err(e) => json!({ "id": null, "error": format!("bad JSON: {e}") }),
        };
        if writeln!(out, "{reply}").is_err() {
            return;
        }
    }
}
