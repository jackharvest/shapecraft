//! ShapeCraft on the command line.
//!
//! - `demo OUT_DIR` builds a sample part and writes STEP + STL (kernel smoke test).
//! - `run SCRIPT.jsonl [--save DIR] [--png OUT.png]` runs commands (one JSON object per line:
//!   `{"command": "rectangle", "params": {…}}`) on a new project.
//! - `render PROJECT_DIR OUT.png [WIDTH HEIGHT]` renders a saved project.
//! - `bench [FRAMES]` times headless frames of a heavy scene (144 filleted cylinders) while the camera turns.
//! - `commands` lists every command with its parameters.
//! - `mcp [--connect HOST:PORT]` serves MCP on stdio, headless or bridged to the running app.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use shapecraft_doc::project;
use shapecraft_engine::Session;
use shapecraft_kernel::{BoolOp, DVec3, Kernel};
use shapecraft_kernel_occt::OcctKernel;
use shapecraft_render::{Camera, Renderer, SceneColors, SceneInput, StandardView};

fn main() -> Result<()> {
    let result = cli();
    // A measurement left running past its budget would crash the normal way out (OCCT's statics go first).
    shapecraft_kernel_occt::finish_process(i32::from(result.is_err()));
    result
}

fn cli() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    match arg(0) {
        Some("demo") => demo(PathBuf::from(arg(1).unwrap_or("."))),
        Some("run") => {
            let script = arg(1).context("run needs a script")?;
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from);
            // --size WxH; --clear: see-through around the model, no axes (artwork for the home screen).
            let size = flag("--size")
                .and_then(|p| p.to_str().and_then(|t| t.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
                .unwrap_or((1600, 1000));
            let clear = args.iter().any(|a| a == "--clear");
            run(Path::new(script), flag("--save"), flag("--png"), size, clear)
        }
        Some("render") => {
            let dir = arg(1).context("render needs a project folder")?;
            let out = arg(2).context("render needs an output .png")?;
            let w = arg(3).and_then(|s| s.parse().ok()).unwrap_or(1600);
            let h = arg(4).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let kernel = Arc::new(OcctKernel::new());
            let doc = project::load(kernel.as_ref(), Path::new(dir))?;
            let session = Session::with_document(kernel, doc, None);
            write_png(&session, Path::new(out), (w, h))
        }
        Some("mcp") => {
            let kernel = Arc::new(OcctKernel::new());
            let connect = args.iter().position(|a| a == "--connect").and_then(|i| args.get(i + 1)).cloned();
            let mut server = match connect {
                Some(addr) => shapecraft_mcp::Server::bridge(kernel, addr),
                None => shapecraft_mcp::Server::headless(kernel),
            };
            let stdin = std::io::stdin();
            server.run(stdin.lock(), std::io::stdout().lock())?;
            Ok(())
        }
        Some("bench") => bench(arg(1).and_then(|n| n.parse().ok()).unwrap_or(600)),
        Some("commands") => {
            for c in shapecraft_engine::registry() {
                println!("{:<16} {:<22} {:<6} {}", c.id, c.label, c.shortcut.unwrap_or(""), c.params);
            }
            Ok(())
        }
        _ => bail!(
            "usage: shapecraft-cli demo OUT_DIR | run SCRIPT [--save DIR] [--png OUT [--size WxH] [--clear]] | render DIR OUT.png [W H] | bench [FRAMES] | commands | mcp [--connect HOST:PORT]"
        ),
    }
}

fn run(script: &Path, save: Option<PathBuf>, png: Option<PathBuf>, size: (u32, u32), clear: bool) -> Result<()> {
    let text = std::fs::read_to_string(script).with_context(|| format!("reading {}", script.display()))?;
    let mut s = Session::new(Arc::new(OcctKernel::new()), "Untitled Project");
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).with_context(|| format!("line {}", n + 1))?;
        let id = v["command"].as_str().with_context(|| format!("line {}: missing \"command\"", n + 1))?;
        let out = s.execute(id, &v["params"]).with_context(|| format!("line {}: {id}", n + 1))?;
        if !out.value.is_null() {
            println!("{id}: {}", out.value);
        }
    }
    if let Some(dir) = save {
        s.execute("save", &serde_json::json!({ "path": dir }))?;
    }
    if let Some(png) = png {
        if clear {
            write_clear_png(&mut s, &png, size)?;
        } else {
            write_png(&s, &png, size)?;
        }
    }
    Ok(())
}

fn bench(frames: usize) -> Result<()> {
    use serde_json::json;
    let mut s = Session::new(Arc::new(OcctKernel::new()), "Bench");
    s.execute("sketch", &json!({ "plane": "xy" }))?;
    s.execute("circle", &json!({ "center": [0, 0], "radius": 8 }))?;
    s.execute("extrude", &json!({ "sketch": 1, "region": 0, "distance": 12 }))?;
    s.execute("select_all", &json!({}))?;
    s.execute("fillet", &json!({ "radius": 1.5 }))?;
    s.execute("select_all", &json!({}))?;
    s.execute("pattern", &json!({ "direction": [1, 0, 0], "spacing": 20, "count": 12 }))?;
    s.execute("select_all", &json!({}))?;
    s.execute("pattern", &json!({ "direction": [0, 1, 0], "spacing": 20, "count": 12 }))?;
    s.execute("deselect_all", &json!({}))?;
    let mut r = Renderer::headless()?;
    let (min, max) = s.doc.bounds().context("no bodies")?;
    let mut camera = Camera::default();
    camera.fit(min, max);
    let mut times = Vec::with_capacity(frames);
    for i in 0..frames {
        camera.yaw += 0.01;
        let scene = plain_scene(&s);
        let began = std::time::Instant::now();
        r.render(&scene, &camera, (1600, 1000))?;
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        // Lets the GPU catch up now and then, as a display's pacing would.
        if i % 30 == 0 {
            r.read_pixels()?;
        }
    }
    times.sort_by(f64::total_cmp);
    println!("{} bodies, {frames} frames: p50 {:.2} ms, p95 {:.2} ms", s.doc.bodies.len(), times[frames / 2], times[frames * 95 / 100]);
    Ok(())
}

/// The model as a saved project would show it, with nothing picked.
fn plain_scene(s: &Session) -> SceneInput<'_> {
    SceneInput {
        doc: &s.doc,
        selection: &s.selection,
        hover: None,
        active_sketch: s.active_sketch,
        plane_tiles: &[],
        overlays: &[],
        colors: SceneColors::default(),
        show_axes: true,
        show_edges: true,
        show_hidden_edges: false,
        show_decals: true,
        grid: s.active_sketch().map(|sk| sk.plane),
        display: Default::default(),
        section: None,
        presentation: false,
        backdrop: shapecraft_render::Rgba::hex(0x101010),
        shadow: 0.0,
        ghosts: &[],
        tints: &[],
        drawn: &[],
        defined: &[],
        boxing: false,
    }
}

pub fn write_png(s: &Session, out: &Path, size: (u32, u32)) -> Result<()> {
    let mut r = Renderer::headless()?;
    let mut camera = Camera::default();
    if let Some((min, max)) = s.doc.bounds() {
        camera.fit(min, max);
    }
    camera = Camera::standard(StandardView::Default, camera.target, camera.distance);
    let scene = plain_scene(s);
    r.render(&scene, &camera, size)?;
    let (w, h, px) = r.read_pixels()?;
    let file = File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&px)?;
    println!("wrote {}", out.display());
    Ok(())
}

/// The model alone, see-through around it: rendered over black and over white and unmixed.
fn write_clear_png(s: &mut Session, out: &Path, size: (u32, u32)) -> Result<()> {
    s.selection.clear();
    s.active_sketch = None;
    let mut r = Renderer::headless()?;
    let mut camera = Camera::default();
    if let Some((min, max)) = s.doc.bounds() {
        camera.fit(min, max);
    }
    camera = Camera::standard(StandardView::Default, camera.target, camera.distance * 0.9);
    let mut over = |bg: u32| -> Result<Vec<u8>> {
        let mut scene = plain_scene(s);
        scene.colors.background = shapecraft_render::Rgba::hex(bg);
        scene.show_axes = false;
        scene.grid = None;
        r.render(&scene, &camera, size)?;
        Ok(r.read_pixels()?.2)
    };
    let (black, white) = (over(0x000000)?, over(0xffffff)?);
    let mut px = Vec::with_capacity(black.len());
    for (b, w) in black.as_chunks::<4>().0.iter().zip(white.as_chunks::<4>().0) {
        let alpha = (1.0 - (0..3).map(|k| w[k] as f32 - b[k] as f32).sum::<f32>() / 765.0).clamp(0.0, 1.0);
        for &v in &b[..3] {
            px.push(if alpha > 1e-3 { (v as f32 / alpha).round().clamp(0.0, 255.0) as u8 } else { 0 });
        }
        px.push((alpha * 255.0).round() as u8);
    }
    let file = File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), size.0, size.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&px)?;
    println!("wrote {}", out.display());
    Ok(())
}

fn demo(out: PathBuf) -> Result<()> {
    let k = OcctKernel::new();
    let plate = k.extrude_polygon(
        &[DVec3::ZERO, DVec3::new(60.0, 0.0, 0.0), DVec3::new(60.0, 40.0, 0.0), DVec3::new(0.0, 40.0, 0.0)],
        DVec3::new(0.0, 0.0, 8.0),
    )?;
    let plate = k.fillet_all_edges(&plate, 1.5)?;
    let mut part = plate;
    for (x, y) in [(10.0, 10.0), (50.0, 10.0), (50.0, 30.0), (10.0, 30.0)] {
        let hole = k.extrude_circle(DVec3::new(x, y, -1.0), 2.5, DVec3::new(0.0, 0.0, 10.0))?;
        part = k.boolean(BoolOp::Subtract, &part, &hole)?;
    }
    let mp = k.mass_props(&part)?;
    let mesh = k.tessellate(&part, 0.05)?;
    std::fs::create_dir_all(&out)?;
    k.write_step(std::slice::from_ref(&part), &out.join("demo.step"))?;
    let stl = File::create(out.join("demo.stl")).context("creating demo.stl")?;
    mesh.write_stl(BufWriter::new(stl))?;
    println!(
        "kernel {}: volume {:.3} mm³, area {:.3} mm², {} triangles, {} edges → {}",
        k.name(),
        mp.volume,
        mp.area,
        mesh.triangle_count(),
        mesh.edges.len(),
        out.display()
    );
    Ok(())
}
