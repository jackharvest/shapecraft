//! MCP (JSON-RPC 2.0 over stdio) for ShapeCraft.
//!
//! Headless: an in-process project; `render` returns a PNG of it. Bridged (`connect`): every call goes to
//! the running app's control channel, so you can watch the agent model and `screenshot` shows the whole UI.

#![forbid(unsafe_code)]

mod b64;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use shapecraft_doc::project;
use shapecraft_engine::Session;
use shapecraft_kernel::Kernel;
use shapecraft_render::{Camera, Renderer, SceneColors, SceneInput, StandardView};

const PROTOCOL: &str = "2025-06-18";
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const GUIDE: &str = "ShapeCraft is a direct-modeling CAD app. Units are millimeters, Z is up. \
Typical flow: execute sketch {plane:\"xy\"} (or {body, face} to draw on a flat face; inspect lists faces with \
normals) → rectangle {corner:[u,v], opposite:[u,v]} or {center, size}, circle {center, radius}, line {a, b} in \
the sketch's own 2D coordinates (xy: u=X v=Y; xz: u=X v=Z; yz: u=Y v=Z; on a face: centered on the face) → \
extrude {sketch, region, distance} (each rectangle/circle is a region numbered by its index; a closed chain of \
lines is region 4294967295; negative distance cuts; a sketch on a body's face merges into or cuts that body) → \
extrude {body, face, distance} pushes or pulls a flat face → union/subtract/intersect {target, tools} → \
fillet {body, edges?, radius}. Call inspect to check volumes and bounds, and render to look at it.";

enum Backend {
    Local { session: Box<Session>, renderer: Option<Box<Renderer>> },
    Bridge { addr: String, conn: Option<(BufReader<TcpStream>, TcpStream)>, next_id: u64 },
}

pub struct Server {
    backend: Backend,
    kernel: Arc<dyn Kernel>,
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

fn tools(bridged: bool) -> Vec<Value> {
    let mut v = vec![
        tool(
            "execute",
            &format!("Run any ShapeCraft command with JSON params (list_commands shows them). Never opens dialogs. {GUIDE}"),
            json!({ "type": "object", "properties": {
                "command": { "type": "string", "description": "command id, e.g. \"rectangle\"" },
                "params": { "type": "object", "description": "the command's parameters" } },
                "required": ["command"] }),
        ),
        tool(
            "list_commands",
            "The command catalog: ids, labels, shortcuts and parameter docs. `filter` matches part of an id or label.",
            json!({ "type": "object", "properties": { "filter": { "type": "string" } } }),
        ),
        tool(
            "inspect",
            "Describe the project: bodies (volume, area, bounds, planar faces with normals), sketches (entities, regions with holes and nesting), selection, history. `body` or `sketch` narrows it to that one; `faces: false` leaves out the face lists.",
            json!({ "type": "object", "properties": {
                "body": { "type": "integer" }, "sketch": { "type": "integer" }, "faces": { "type": "boolean" } } }),
        ),
        tool(
            "render",
            "A PNG of the model from a standard view (default, front, back, top, bottom, left, right), framed to fit.",
            json!({ "type": "object", "properties": {
                "view": { "type": "string" }, "width": { "type": "integer" }, "height": { "type": "integer" } } }),
        ),
        tool(
            "new_project",
            "Start a new, empty project (the current one is kept on disk if it was saved).",
            json!({ "type": "object", "properties": { "name": { "type": "string" } } }),
        ),
        tool(
            "open_project",
            "Open a project folder (one with project.json).",
            json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
        ),
        tool(
            "save_project",
            "Save the project folder (project.json + a STEP file).",
            json!({ "type": "object", "properties": { "path": { "type": "string" } } }),
        ),
        tool(
            "export",
            "Export bodies to a .step or .stl file (by extension).",
            json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
        ),
    ];
    if bridged {
        v.push(tool("screenshot", "A PNG of the whole app window as the user sees it.", json!({ "type": "object", "properties": {} })));
    }
    v
}

fn text(v: impl ToString) -> Value {
    json!({ "content": [{ "type": "text", "text": v.to_string() }] })
}

fn tool_error(msg: impl ToString) -> Value {
    json!({ "content": [{ "type": "text", "text": msg.to_string() }], "isError": true })
}

fn image(png: &[u8]) -> Value {
    json!({ "content": [{ "type": "image", "data": b64::encode(png), "mimeType": "image/png" }] })
}

fn encode_png(w: u32, h: u32, px: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(px).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn view_named(name: &str) -> StandardView {
    match name {
        "front" => StandardView::Front,
        "back" => StandardView::Back,
        "top" => StandardView::Top,
        "bottom" => StandardView::Bottom,
        "right" => StandardView::Right,
        "left" => StandardView::Left,
        _ => StandardView::Default,
    }
}

impl Server {
    pub fn headless(kernel: Arc<dyn Kernel>) -> Self {
        let session = Box::new(Session::new(kernel.clone(), "Untitled Project"));
        Self { backend: Backend::Local { session, renderer: None }, kernel }
    }

    /// Bridges to a running app's control channel (`shapecraft --control PORT`).
    pub fn bridge(kernel: Arc<dyn Kernel>, addr: impl Into<String>) -> Self {
        Self { backend: Backend::Bridge { addr: addr.into(), conn: None, next_id: 1 }, kernel }
    }

    /// Serves until the input closes.
    pub fn run(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = self.handle_line(&line) {
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// One JSON-RPC message in, maybe one out (notifications get no reply).
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })),
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let id = id?; // notifications (no id) need no reply
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL);
                let version = if SUPPORTED.contains(&asked) { asked } else { PROTOCOL };
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {}, "resources": {} },
                    "serverInfo": { "name": "shapecraft", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": GUIDE,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools(matches!(self.backend, Backend::Bridge { .. })) })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !args.is_object() {
                    Err((-32602, "arguments must be an object".to_string()))
                } else if !tools(true).iter().any(|t| t["name"] == name) {
                    Err((-32602, format!("unknown tool '{name}'")))
                } else {
                    Ok(self.call(name, &args))
                }
            }
            "resources/list" => Ok(json!({ "resources": [
                { "uri": "shapecraft://project", "name": "project", "description": "inspect output", "mimeType": "application/json" },
                { "uri": "shapecraft://commands", "name": "commands", "description": "command catalog", "mimeType": "application/json" },
            ] })),
            "resources/read" => {
                let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                let body = match uri {
                    "shapecraft://project" => self.exec("inspect", json!({})),
                    "shapecraft://commands" => Ok(self.commands("")),
                    _ => Err(format!("unknown resource {uri}")),
                };
                match body {
                    Ok(v) => Ok(json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": v.to_string() }] })),
                    Err(e) => Err((-32602, e)),
                }
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        })
    }

    fn call(&mut self, name: &str, args: &Value) -> Value {
        let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
        let out = match name {
            "execute" => match s("command") {
                Some(c) => self.exec(&c, args.get("params").cloned().unwrap_or(json!({}))).map(text),
                None => Err("'command' is required".into()),
            },
            "list_commands" => Ok(text(self.commands(&s("filter").unwrap_or_default()))),
            "inspect" => {
                let mut p = serde_json::Map::new();
                for k in ["body", "sketch", "faces"] {
                    if let Some(v) = args.get(k) {
                        p.insert(k.into(), v.clone());
                    }
                }
                self.exec("inspect", Value::Object(p))
            }
            .map(|v| text(serde_json::to_string_pretty(&v).unwrap_or_default())),
            "render" => {
                let w = args.get("width").and_then(Value::as_u64).unwrap_or(1200).clamp(64, 4096) as u32;
                let h = args.get("height").and_then(Value::as_u64).unwrap_or(800).clamp(64, 4096) as u32;
                self.render(&s("view").unwrap_or_default(), (w, h)).map(|png| image(&png))
            }
            "screenshot" => self.screenshot().map(|png| image(&png)),
            "new_project" => self.new_project(s("name")).map(|_| text("new project")),
            "open_project" => match s("path") {
                Some(p) => self.open_project(&p).map(|_| text(format!("opened {p}"))),
                None => Err("'path' is required".into()),
            },
            "save_project" => {
                let params = s("path").map(|p| json!({ "path": p })).unwrap_or(json!({}));
                self.exec("save", params).map(text)
            }
            "export" => match s("path") {
                Some(p) if p.to_ascii_lowercase().ends_with(".stl") => self.exec("export_stl", json!({ "path": p })).map(text),
                Some(p) => self.exec("export_step", json!({ "path": p })).map(text),
                None => Err("'path' is required".into()),
            },
            _ => Err(format!("unknown tool '{name}'")),
        };
        out.unwrap_or_else(tool_error)
    }

    fn commands(&mut self, filter: &str) -> Value {
        let f = filter.to_lowercase();
        Value::Array(
            shapecraft_engine::registry()
                .iter()
                .filter(|c| f.is_empty() || c.id.contains(&f) || c.label.to_lowercase().contains(&f))
                .map(|c| json!({ "id": c.id, "label": c.label, "shortcut": c.shortcut, "help": c.help, "params": c.params }))
                .collect(),
        )
    }

    fn exec(&mut self, command: &str, params: Value) -> Result<Value, String> {
        match &mut self.backend {
            Backend::Local { session, .. } => session.execute(command, &params).map(|o| o.value).map_err(|e| e.to_string()),
            Backend::Bridge { .. } => self.control("engine.execute", json!({ "command": command, "params": params })),
        }
    }

    fn control(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let Backend::Bridge { addr, conn, next_id } = &mut self.backend else { return Err("not bridged".into()) };
        if conn.is_none() {
            let stream = TcpStream::connect(addr.as_str())
                .map_err(|e| format!("can't reach ShapeCraft at {addr} ({e}); start it with `shapecraft --control PORT`"))?;
            let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
            *conn = Some((reader, stream));
        }
        let id = *next_id;
        *next_id += 1;
        let req = json!({ "id": id, "method": method, "params": params });
        let result = (|| {
            let (reader, stream) = conn.as_mut().ok_or("no connection")?;
            writeln!(stream, "{req}").map_err(|e| e.to_string())?;
            let mut line = String::new();
            reader.read_line(&mut line).map_err(|e| e.to_string())?;
            if line.is_empty() {
                return Err("ShapeCraft closed the connection".to_string());
            }
            let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            match reply.get("error") {
                Some(e) if !e.is_null() => Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())),
                _ => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
            }
        })();
        if result.as_ref().is_err_and(|e| e.contains("closed") || e.contains("Broken pipe")) {
            *conn = None;
        }
        result
    }

    fn render(&mut self, view: &str, size: (u32, u32)) -> Result<Vec<u8>, String> {
        match &mut self.backend {
            Backend::Local { session, renderer } => {
                if renderer.is_none() {
                    *renderer = Some(Box::new(Renderer::headless().map_err(|e| e.to_string())?));
                }
                let r = renderer.as_mut().ok_or("no renderer")?;
                let mut camera = Camera::default();
                if let Some((min, max)) = session.doc.bounds() {
                    camera.fit(min, max);
                }
                let camera = Camera::standard(view_named(view), camera.target, camera.distance);
                // Agents want to see the part, not the selection highlight.
                let scene = SceneInput {
                    doc: &session.doc,
                    selection: &[],
                    hover: None,
                    active_sketch: session.active_sketch,
                    plane_tiles: &[],
                    overlays: &[],
                    colors: SceneColors::default(),
                    show_axes: true,
                    show_edges: true,
                    show_hidden_edges: false,
                    show_decals: true,
                    grid: session.active_sketch().map(|s| s.plane),
                    display: Default::default(),
                    section: None,
                    presentation: false,
                    backdrop: shapecraft_render::Rgba::hex(0x101010),
                    shadow: 0.0,
                    ghosts: &[],
                    tints: &[],
                    drawn: &[],
                    defined: &[],
                };
                r.render(&scene, &camera, size).map_err(|e| e.to_string())?;
                let (w, h, px) = r.read_pixels().map_err(|e| e.to_string())?;
                encode_png(w, h, &px)
            }
            Backend::Bridge { .. } => {
                self.control("ui.view", json!({ "view": if view.is_empty() { "default" } else { view } }))?;
                std::thread::sleep(std::time::Duration::from_millis(450));
                self.screenshot()
            }
        }
    }

    fn screenshot(&mut self) -> Result<Vec<u8>, String> {
        let path = std::env::temp_dir().join(format!("shapecraft-mcp-{}.png", std::process::id()));
        self.control("ui.screenshot", json!({ "path": path }))?;
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&path);
        Ok(bytes)
    }

    fn new_project(&mut self, name: Option<String>) -> Result<(), String> {
        match &mut self.backend {
            Backend::Local { session, .. } => {
                **session = Session::new(self.kernel.clone(), name.as_deref().unwrap_or("Untitled Project"));
                Ok(())
            }
            Backend::Bridge { .. } => self.control("project.new", json!({})).map(|_| ()),
        }
    }

    fn open_project(&mut self, path: &str) -> Result<(), String> {
        match &mut self.backend {
            Backend::Local { session, .. } => {
                let doc = project::load(self.kernel.as_ref(), &PathBuf::from(path)).map_err(|e| e.to_string())?;
                **session = Session::with_document(self.kernel.clone(), doc, Some(PathBuf::from(path)));
                Ok(())
            }
            Backend::Bridge { .. } => self.control("project.open", json!({ "path": path })).map(|_| ()),
        }
    }
}

#[cfg(test)]
mod tests;
