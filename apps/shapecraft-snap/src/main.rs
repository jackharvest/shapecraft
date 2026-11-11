//! Drives the real UI offscreen and writes PNGs, so it can be compared with reference screenshots
//! without opening a window. The same steps score the task benchmark (`bench/`).
//!
//! `shapecraft-snap SCENARIO.jsonl OUT_DIR`, one step per line:
//! `{"size": [1840, 1324]}` (first line: start size, `"scale": 2` for Retina pixels; later: resize), `{"control": "engine.execute", "params": {...}}`,
//! `{"hover": [x, y]}`, `{"click": [x, y]}`, `{"right_click": [x, y]}`, `{"double_click": [x, y]}`, `{"right_drag": [[x0, y0], [x1, y1]]}`, `{"drag": [[..], [..]]}`,
//! `{"key": "R", "mods": "cmd+shift"}`, `{"text": "10"}`, `{"touch_screen": true}` and `{"small_screen": true}` (as on an Android tablet or phone), `{"wait": 0.5}`, `{"sleep": 0.5}` (real time, for background threads), `{"shot": "name"}`.
//! A point with three numbers is a model point (mm), placed on screen through the current camera.
//! `"@face:BODY:x,y,z"` and `"@edge:BODY:x,y,z"` anywhere in a step become the face or edge at that model point.
//! `{"expect": {...}}` checks the model (see [`expect`]); `{"task": {...}}` names a benchmark task.
//!
//! `shapecraft-snap --bench TASKS_DIR [OUT_DIR] [--only NAME]` runs every task and writes `results.json` and
//! `RESULTS.md` into OUT_DIR (default: TASKS_DIR/..), scored by the categories in TASKS_DIR/../categories.json.

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_kernel_occt::OcctKernel;
use shapecraft_ui::{Options, ShapeCraftApp};

type App<'a> = Harness<'a, ShapeCraftApp>;

/// A screen point, or a model point (three numbers) projected through the camera.
fn pos(h: &mut App<'_>, v: &Value) -> Option<Pos2> {
    let n = |i: usize| v.get(i).and_then(Value::as_f64);
    match (n(0), n(1), n(2)) {
        (Some(x), Some(y), Some(z)) => h.state_mut().workspace()?.to_screen(DVec3::new(x, y, z)),
        (Some(x), Some(y), None) => Some(pos2(x as f32, y as f32)),
        _ => None,
    }
}

fn key(name: &str) -> Option<Key> {
    Key::from_name(name)
}

fn mods(s: &str) -> Modifiers {
    let mut m = Modifiers::NONE;
    for part in s.split('+') {
        match part {
            "cmd" => m |= Modifiers::COMMAND,
            "shift" => m |= Modifiers::SHIFT,
            "alt" | "option" => m |= Modifiers::ALT,
            "ctrl" => m |= Modifiers::CTRL,
            _ => {}
        }
    }
    m
}

fn press(h: &App<'_>, at: Pos2, button: PointerButton, pressed: bool) {
    h.event(Event::PointerButton { pos: at, button, pressed, modifiers: Modifiers::NONE });
}

fn parse(path: &Path) -> Result<Vec<Value>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines = Vec::new();
    for (n, l) in text.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        let l = l.replace("$TMP", &tmp_dir().to_string_lossy());
        lines.push(serde_json::from_str(&l).with_context(|| format!("{} line {}", path.display(), n + 1))?);
    }
    Ok(lines)
}

/// A scratch folder for files tasks write and read back (`$TMP` in a step).
fn tmp_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("shapecraft-bench-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

/// What one run found: each check with whether it held, and errors the app reported.
#[derive(Default)]
struct Outcome {
    checks: Vec<(bool, String)>,
    errors: Vec<String>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--bench") {
        return bench(&args[1..]);
    }
    let (Some(script), Some(out)) = (args.first(), args.get(1)) else {
        bail!("usage: shapecraft-snap SCENARIO.jsonl OUT_DIR | --bench TASKS_DIR [OUT_DIR] [--only NAME]")
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let mut outcome = Outcome::default();
    run(&parse(Path::new(script))?, Some(&out), &mut outcome)?;
    for (ok, what) in &outcome.checks {
        println!("expect {}: {what}", if *ok { "ok" } else { "FAILED" });
    }
    Ok(())
}

fn run(lines: &[Value], out: Option<&Path>, outcome: &mut Outcome) -> Result<()> {
    let projects = std::env::temp_dir().join(format!("shapecraft-snap-{}-{}", std::process::id(), rand_tag()));
    let size = lines.first().and_then(|v| v.get("size")).and_then(|v| Some(pos2(v.get(0)?.as_f64()? as f32, v.get(1)?.as_f64()? as f32)));
    let size = size.unwrap_or(pos2(1840.0, 1324.0));
    // `{"size": [w, h], "scale": 2}`: pixels per point (2 renders like a Retina screen, for artwork).
    let scale = lines.first().and_then(|v| v.get("scale")).and_then(Value::as_f64).unwrap_or(1.0).clamp(0.5, 4.0) as f32;
    let opts = Options {
        projects_root: projects.clone(),
        control_port: None,
        open: None,
        new_project: false,
        native_menu: false,
        settings_file: None,
        hosted: false,
    };
    let mut h = Harness::builder()
        .with_size(size.to_vec2())
        .with_pixels_per_point(scale)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| ShapeCraftApp::new(Arc::new(OcctKernel::new()), opts, &cc.egui_ctx));
    h.run_steps(3);
    let result = steps(&mut h, lines, out, outcome);
    let _ = std::fs::remove_dir_all(&projects);
    result
}

fn rand_tag() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

fn steps(h: &mut App<'_>, lines: &[Value], out: Option<&Path>, outcome: &mut Outcome) -> Result<()> {
    let mut held = None;
    let at = |h: &mut App<'_>, v: &Value| pos(h, v).with_context(|| format!("no screen point for {v}"));
    // Values a control step saved (`"save": {"name": "img", "at": "/image"}`), put back wherever a later step says "$img".
    let mut saved: Vec<(String, Value)> = Vec::new();
    for (i, step) in lines.iter().enumerate() {
        let step = &if saved.is_empty() && !step.to_string().contains("\"@") {
            step.clone()
        } else {
            let mut text = step.to_string();
            for (name, v) in &saved {
                text = text.replace(&format!("\"${name}\""), &v.to_string());
            }
            serde_json::from_str(&resolve_refs(h, &text)?)?
        };
        if let Some(method) = step.get("control").and_then(Value::as_str) {
            let params = step.get("params").cloned().unwrap_or(Value::Null);
            let label = step.get("tag").and_then(Value::as_str).unwrap_or(method).to_string();
            match h.state_mut().control_call(method, &params) {
                Ok(v) => {
                    if !v.is_null() && out.is_some() {
                        println!("{label}: {v}");
                    }
                    if let Some(want) = step.get("returns") {
                        outcome.checks.push(returns(want, &v, &label));
                    }
                    if let (Some(name), Some(at)) = (step["save"]["name"].as_str(), step["save"]["at"].as_str()) {
                        saved.push((name.to_string(), v.pointer(at).cloned().unwrap_or(Value::Null)));
                    }
                }
                Err(e) => {
                    println!("{method}: error: {e}");
                    outcome.errors.push(format!("{label}: {e}"));
                }
            }
            h.run_steps(2);
        } else if let Some(v) = step.get("hover") {
            let p = at(h, v)?;
            h.hover_at(p);
            h.run_steps(2);
            // Moving while held carries the drag: "release" lets go here, not where the hold began.
            if held.is_some() {
                held = Some(p);
            }
        } else if let Some(v) = step.get("click") {
            let p = at(h, v)?;
            let m = mods(step.get("mods").and_then(Value::as_str).unwrap_or(""));
            // Held keys show in the input's modifier state as well as on the click, as with a real keyboard.
            h.event(Event::ModifiersChanged(m));
            h.hover_at(p);
            h.run_steps(1);
            h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: m });
            h.run_steps(1);
            h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: m });
            h.run_steps(2);
            h.event(Event::ModifiersChanged(Modifiers::NONE));
        } else if let Some(v) = step.get("right_click") {
            let p = at(h, v)?;
            h.hover_at(p);
            h.run_steps(1);
            press(h, p, PointerButton::Secondary, true);
            h.run_steps(1);
            press(h, p, PointerButton::Secondary, false);
            h.run_steps(2);
        } else if let Some(v) = step.get("double_click") {
            let p = at(h, v)?;
            h.hover_at(p);
            h.run_steps(1);
            for _ in 0..2 {
                press(h, p, PointerButton::Primary, true);
                h.run_steps(1);
                press(h, p, PointerButton::Primary, false);
                h.run_steps(1);
            }
            h.run_steps(2);
        } else if let Some(d) = step.get("drag").or(step.get("right_drag")) {
            let button = if step.get("right_drag").is_some() { PointerButton::Secondary } else { PointerButton::Primary };
            let (Some(a), Some(b)) = (d.get(0), d.get(1)) else { bail!("drag needs two points") };
            let (a, b) = (at(h, a)?, at(h, b)?);
            h.hover_at(a);
            h.run_steps(1);
            press(h, a, button, true);
            for i in 1..=10 {
                h.hover_at(a + (b - a) * (i as f32 / 10.0));
                h.run_steps(1);
            }
            press(h, b, button, false);
            h.run_steps(2);
        } else if let Some(d) = step.get("hold") {
            // Press at the first point and move to the second without letting go (a later "release" ends it).
            let (Some(a), Some(b)) = (d.get(0), d.get(1)) else { bail!("hold needs two points") };
            let (a, b) = (at(h, a)?, at(h, b)?);
            h.hover_at(a);
            h.run_steps(1);
            press(h, a, PointerButton::Primary, true);
            for i in 1..=10 {
                h.hover_at(a + (b - a) * (i as f32 / 10.0));
                h.run_steps(1);
            }
            held = Some(b);
        } else if step.get("release").is_some() {
            if let Some(b) = held.take() {
                press(h, b, PointerButton::Primary, false);
                h.run_steps(2);
            }
        } else if let Some(k) = step.get("key").and_then(Value::as_str) {
            let k = key(k).with_context(|| format!("unknown key {k}"))?;
            let m = mods(step.get("mods").and_then(Value::as_str).unwrap_or(""));
            h.key_press_modifiers(m, k);
            h.run_steps(2);
        } else if let Some(t) = step.get("text").and_then(Value::as_str) {
            h.event(Event::Text(t.to_string()));
            h.run_steps(2);
        } else if let Some(on) = step.get("touch_screen").and_then(Value::as_bool) {
            // As the Android app starts: a touch screen of its own (touch layout, first-run question).
            shapecraft_ui::tablet::set_touch_screen(on);
        } else if let Some(on) = step.get("small_screen").and_then(Value::as_bool) {
            // An Android phone (under 600 dp across).
            shapecraft_ui::tablet::set_small_screen(on);
        } else if let Some(s) = step.get("wait").and_then(Value::as_f64) {
            h.run_steps(((s * 60.0).ceil() as usize).clamp(1, 600));
        } else if let Some(s) = step.get("sleep").and_then(Value::as_f64) {
            // Real time, for work on other threads (a "wait" is frames, done as fast as they draw).
            std::thread::sleep(std::time::Duration::from_secs_f64(s.clamp(0.0, 30.0)));
            h.run_steps(2);
        } else if let Some(name) = step.get("shot").and_then(Value::as_str) {
            let Some(out) = out else { continue };
            h.run_steps(2);
            let img = h.render().map_err(|e| anyhow::anyhow!(e))?;
            let path = out.join(format!("{name}.png"));
            img.save(&path)?;
            println!("wrote {}", path.display());
        } else if let Some(sz) = step.get("size") {
            // The first line sets the start size; a later one resizes the window.
            if i > 0 {
                let sz = at(h, sz)?;
                h.set_size(sz.to_vec2());
                h.run_steps(2);
            }
        } else if let Some(want) = step.get("expect") {
            h.run_steps(2);
            let model = h.state_mut().control_call("engine.execute", &json!({ "command": "inspect" })).map_err(|e| anyhow::anyhow!(e))?;
            let state = h.state_mut().control_call("ui.state", &Value::Null).unwrap_or(Value::Null);
            outcome.checks.extend(expect(want, &model, &state));
        } else if step.get("task").is_some() {
        } else {
            bail!("unknown step {step}");
        }
    }
    Ok(())
}

/// Puts face and edge numbers in for `"@face:BODY:x,y,z"` / `"@edge:BODY:x,y,z"`: the flat face whose plane holds the
/// point (nearest centre first; else the curved face with the nearest centre), or the edge whose middle is nearest it. What a user picks by clicking, without
/// tasks depending on how the kernel numbers faces after each change.
fn resolve_refs(h: &mut App<'_>, text: &str) -> Result<String> {
    let mut out = text.to_string();
    while let Some(start) = out.find("\"@face:").or_else(|| out.find("\"@edge:")) {
        let end = start + 1 + out[start + 1..].find('"').context("unterminated @ reference")?;
        let spec = &out[start + 2..end];
        let mut parts = spec.split(':');
        let (kind, body) = (parts.next().unwrap_or(""), parts.next().and_then(|b| b.parse::<u64>().ok()).context("@ needs a body")?);
        let p: Vec<f64> = parts.next().unwrap_or("").split(',').filter_map(|n| n.trim().parse().ok()).collect();
        let [x, y, z] = p[..] else { bail!("@{kind} needs a point x,y,z") };
        let pt = DVec3::new(x, y, z);
        let model = h.state_mut().control_call("engine.execute", &json!({ "command": "inspect" })).map_err(|e| anyhow::anyhow!(e))?;
        let b =
            model["bodies"].as_array().and_then(|a| a.iter().find(|b| b["id"].as_u64() == Some(body))).cloned().context("no such body")?;
        let v3 = |v: &Value| DVec3::new(v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0));
        let index = if kind == "face" {
            let faces: Vec<&Value> = b["faces"].as_array().into_iter().flatten().collect();
            let flat = faces
                .iter()
                .filter(|f| f["planar"].as_bool() == Some(true) && (pt - v3(&f["center"])).dot(v3(&f["normal"])).abs() < 1e-3)
                .min_by(|a, c| (v3(&a["center"]) - pt).length().total_cmp(&(v3(&c["center"]) - pt).length()))
                .and_then(|f| f["face"].as_u64());
            // No flat face through the point: the curved face whose centre (a cylinder's is on its axis) is nearest.
            flat.or_else(|| {
                faces
                    .iter()
                    .filter(|f| f["planar"].as_bool() == Some(false))
                    .filter_map(|f| {
                        let m = h
                            .state_mut()
                            .control_call("engine.execute", &json!({ "command": "measure", "params": { "picks": [{ "kind": "face", "body": body, "face": f["face"] }] } }))
                            .ok()?;
                        Some((f["face"].as_u64()?, (v3(&m["items"][0]["center"]) - pt).length()))
                    })
                    .min_by(|a, c| a.1.total_cmp(&c.1))
                    .map(|(f, _)| f)
            })
        } else {
            let n = b["edges"].as_u64().unwrap_or(0).min(2000);
            (0..n)
                .filter_map(|e| {
                    let m = h
                        .state_mut()
                        .control_call("engine.execute", &json!({ "command": "measure", "params": { "picks": [{ "kind": "edge", "body": body, "edge": e }] } }))
                        .ok()?;
                    Some((e, (v3(&m["items"][0]["center"]) - pt).length()))
                })
                .min_by(|a, c| a.1.total_cmp(&c.1))
                .map(|(e, _)| e)
        }
        .with_context(|| format!("nothing at @{spec}"))?;
        out.replace_range(start..=end, &index.to_string());
    }
    Ok(out)
}

/// Checks the model against `want`. Keys: `bodies` (count), `visible_bodies`, `volume` (total mm³, or
/// `[value, relative tolerance]`, default 0.1%), `volumes` (each body's, any order), `area`, `faces` (total),
/// `bounds` (`[[min], [max]]` of all bodies, ±0.05 mm), `sketches`, `regions` (in the last sketch), `history`
/// (step count, or labels each step's label must start with), `history_has` (labels that must appear),
/// `tool` (`"idle"` or `"busy"`), `selection` (count), `banner` (text the hint banner must contain),
/// `failed_steps` (count of History steps marked failed), `volume_between` (`[lo, hi]`), `file` (a path that must exist and not be empty).
fn expect(want: &Value, model: &Value, state: &Value) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let bodies = model["bodies"].as_array().cloned().unwrap_or_default();
    let tol = |w: &Value| -> (f64, f64) {
        match w {
            Value::Array(a) => (a.first().and_then(Value::as_f64).unwrap_or(f64::NAN), a.get(1).and_then(Value::as_f64).unwrap_or(1e-3)),
            v => (v.as_f64().unwrap_or(f64::NAN), 1e-3),
        }
    };
    let near = |got: f64, (want, rel): (f64, f64)| (got - want).abs() <= rel * want.abs().max(1.0);
    let sum = |key: &str| bodies.iter().filter_map(|b| b[key].as_f64()).sum::<f64>();
    let history: Vec<String> =
        model["history"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let Some(want) = want.as_object() else {
        return vec![(false, format!("expect needs an object, got {want}"))];
    };
    for (k, w) in want {
        let (ok, got) = match k.as_str() {
            "bodies" => (bodies.len() as u64 == w.as_u64().unwrap_or(u64::MAX), json!(bodies.len())),
            "visible_bodies" => {
                let n = bodies.iter().filter(|b| b["visible"].as_bool() == Some(true)).count();
                (n as u64 == w.as_u64().unwrap_or(u64::MAX), json!(n))
            }
            "volume" => (near(sum("volume_mm3"), tol(w)), json!(sum("volume_mm3"))),
            "area" => (near(sum("area_mm2"), tol(w)), json!(sum("area_mm2"))),
            "volume_between" => {
                let v = sum("volume_mm3");
                (v >= w[0].as_f64().unwrap_or(f64::NAN) && v <= w[1].as_f64().unwrap_or(f64::NAN), json!(v))
            }
            "file" => {
                let len = w.as_str().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
                (len > 0, json!(len))
            }
            "volumes" => {
                let mut got: Vec<f64> = bodies.iter().filter_map(|b| b["volume_mm3"].as_f64()).collect();
                let mut wanted: Vec<f64> = w.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                got.sort_by(f64::total_cmp);
                wanted.sort_by(f64::total_cmp);
                let ok = got.len() == wanted.len() && got.iter().zip(&wanted).all(|(g, w)| near(*g, (*w, 1e-3)));
                (ok, json!(got))
            }
            "faces" => {
                let n: u64 = bodies.iter().filter_map(|b| b["face_count"].as_u64()).sum();
                (Some(n) == w.as_u64(), json!(n))
            }
            "bounds" => {
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for b in &bodies {
                    for i in 0..3 {
                        lo[i] = lo[i].min(b["bounds"]["min"][i].as_f64().unwrap_or(f64::INFINITY));
                        hi[i] = hi[i].max(b["bounds"]["max"][i].as_f64().unwrap_or(f64::NEG_INFINITY));
                    }
                }
                let wv = |j: usize, i: usize| w[j][i].as_f64().unwrap_or(f64::NAN);
                let ok = (0..3).all(|i| (lo[i] - wv(0, i)).abs() <= 0.05 && (hi[i] - wv(1, i)).abs() <= 0.05);
                (ok, json!([lo, hi]))
            }
            "failed_steps" => {
                let n = model["failed_steps"].as_array().map_or(0, Vec::len);
                (Some(n as u64) == w.as_u64(), model["failed_steps"].clone())
            }
            "sketches" => {
                let n = model["sketches"].as_array().map_or(0, Vec::len);
                (Some(n as u64) == w.as_u64(), json!(n))
            }
            "regions" => {
                let n = model["sketches"].as_array().and_then(|a| a.last()).and_then(|s| s["regions"].as_array()).map_or(0, Vec::len);
                (Some(n as u64) == w.as_u64(), json!(n))
            }
            "history" => match w {
                Value::Array(labels) => {
                    let ok = labels.len() == history.len()
                        && labels.iter().zip(&history).all(|(l, h)| l.as_str().is_some_and(|l| h.starts_with(l)));
                    (ok, json!(history))
                }
                n => (Some(history.len() as u64) == n.as_u64(), json!(history)),
            },
            "history_has" => {
                let ok =
                    w.as_array().is_some_and(|a| a.iter().all(|l| l.as_str().is_some_and(|l| history.iter().any(|h| h.starts_with(l)))));
                (ok, json!(history))
            }
            "tool" => {
                let idle = state["tool"].as_str().is_some_and(|t| t.contains("(0)"));
                let want_idle = w.as_str() == Some("idle");
                (idle == want_idle, state["tool"].clone())
            }
            "selection" => {
                let n = state["selection"].as_array().map_or(0, Vec::len);
                (Some(n as u64) == w.as_u64(), json!(n))
            }
            "banner" => {
                let text = state["banner"].to_string();
                (w.as_str().is_some_and(|s| text.contains(s)), state["banner"].clone())
            }
            other => (false, json!(format!("unknown expectation {other}"))),
        };
        out.push((ok, format!("{k} = {w} (got {got})")));
    }
    out
}

/// Checks a control call's answer: `{"at": "/json/pointer", "near": n}` (±0.1%, or `"tol"` absolute), `"is": value`,
/// `"min": n` (a number, or an array's length), or `"same": "/other/pointer"` / `"neg": …` (equal to that value, or
/// to minus it, ±`tol`, default 1e-4).
fn returns(want: &Value, got: &Value, label: &str) -> (bool, String) {
    let at = want["at"].as_str().unwrap_or("");
    let v = got.pointer(at).cloned().unwrap_or(Value::Null);
    let ok = if let Some(n) = want["near"].as_f64() {
        let tol = want["tol"].as_f64().unwrap_or(1e-3 * n.abs().max(1.0));
        v.as_f64().is_some_and(|g| (g - n).abs() <= tol)
    } else if let Some(m) = want["min"].as_f64() {
        v.as_f64().or(v.as_array().map(|a| a.len() as f64)).is_some_and(|g| g >= m)
    } else if let Some(other) = want["same"].as_str().or(want["neg"].as_str()) {
        let sign = if want.get("neg").is_some() { -1.0 } else { 1.0 };
        let tol = want["tol"].as_f64().unwrap_or(1e-4);
        match (v.as_f64(), got.pointer(other).and_then(Value::as_f64)) {
            (Some(a), Some(b)) => (a - sign * b).abs() <= tol,
            _ => false,
        }
    } else if let Some(is) = want.get("is") {
        &v == is
    } else {
        false
    };
    (ok, format!("{label} {at} {} (got {v})", serde_json::to_string(want).unwrap_or_default()))
}

/// One task's result in the benchmark.
struct Scored {
    name: String,
    category: String,
    about: String,
    score: f64,
    notes: Vec<String>,
}

fn bench(args: &[String]) -> Result<()> {
    // A parameter a command ignores must fail its task rather than pass for an option we have.
    // SAFETY: set before any other thread starts (the harness creates its threads per task, later).
    unsafe { std::env::set_var("SHAPECRAFT_STRICT_PARAMS", "1") };
    let only = args.iter().position(|a| a == "--only").and_then(|i| args.get(i + 1)).cloned();
    let positional: Vec<&String> =
        args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || args[i - 1] != "--only")).map(|(_, a)| a).collect();
    let tasks_dir = PathBuf::from(positional.first().context("--bench needs a tasks folder")?);
    let root = tasks_dir.parent().map(Path::to_path_buf).unwrap_or_default();
    let out_dir = positional.get(1).map(PathBuf::from).unwrap_or_else(|| root.clone());
    let categories: Value = serde_json::from_str(&std::fs::read_to_string(root.join("categories.json")).context("categories.json")?)?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(&tasks_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut results = Vec::new();
    for file in files {
        let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let lines = parse(&file)?;
        let meta = lines.iter().find_map(|l| l.get("task")).cloned().unwrap_or(Value::Null);
        let category = meta["category"].as_str().unwrap_or("uncategorised").to_string();
        let about = meta["about"].as_str().unwrap_or("").to_string();
        let allow_errors = meta["allow_errors"].as_bool().unwrap_or(false);
        let mut outcome = Outcome::default();
        let ran = std::panic::catch_unwind(AssertUnwindSafe(|| run(&lines, None, &mut outcome)));
        let mut notes: Vec<String> = Vec::new();
        let score = match ran {
            Err(_) => {
                notes.push("PANIC".into());
                0.0
            }
            Ok(Err(e)) => {
                notes.push(format!("stopped: {e:#}"));
                0.0
            }
            Ok(Ok(())) => {
                let mut checks = outcome.checks;
                // A command or tool we don't have at all scores nothing, not partial credit for the checks around it.
                if outcome.errors.iter().any(|e| e.contains("unknown command") || e.contains("unknown parameter")) {
                    checks.push((false, "missing feature".into()));
                    checks.retain(|c| !c.0);
                }
                if !allow_errors {
                    checks.push((outcome.errors.is_empty(), format!("no errors (got {:?})", outcome.errors)));
                }
                notes.extend(checks.iter().filter(|c| !c.0).map(|c| c.1.clone()));
                if checks.is_empty() { 0.0 } else { checks.iter().filter(|c| c.0).count() as f64 / checks.len() as f64 }
            }
        };
        println!("{:>5.0}%  {name}{}", score * 100.0, if notes.is_empty() { String::new() } else { format!("  — {}", notes.join("; ")) });
        results.push(Scored { name, category, about, score, notes });
    }
    if only.is_some() {
        // A partial run isn't a measurement: leave the last full report alone.
        let mean = results.iter().map(|r| r.score).sum::<f64>() / results.len().max(1) as f64;
        println!("{} tasks, mean {:.1}% (partial run: RESULTS.md not written)", results.len(), mean * 100.0);
        return Ok(());
    }
    report(&categories, &results, &out_dir)
}

fn report(categories: &Value, results: &[Scored], out_dir: &Path) -> Result<()> {
    let mut by_cat: BTreeMap<&str, Vec<&Scored>> = BTreeMap::new();
    for r in results {
        by_cat.entry(&r.category).or_default().push(r);
    }
    let cats = categories.as_object().cloned().unwrap_or_default();
    let mut rows = Vec::new();
    let (mut weighted, mut weights) = (0.0, 0.0);
    for (key, c) in &cats {
        let w = c["weight"].as_f64().unwrap_or(0.0);
        let tasks = by_cat.get(key.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        // What a script can't drive (how navigation feels) comes in as hand-scored entries, each counting as a task.
        let manual: Vec<f64> = c["manual"].as_array().map(|a| a.iter().filter_map(|m| m["score"].as_f64()).collect()).unwrap_or_default();
        let all: Vec<f64> = tasks.iter().map(|t| t.score).chain(manual.iter().copied()).collect();
        let score = if all.is_empty() { None } else { Some(all.iter().sum::<f64>() / all.len() as f64) };
        if let Some(s) = score {
            weighted += s * w;
            weights += w;
        }
        rows.push((key.clone(), c["label"].as_str().unwrap_or(key).to_string(), w, all.len(), score));
    }
    let overall = if weights > 0.0 { weighted / weights } else { 0.0 };
    let json = json!({
        "overall": overall,
        "categories": rows.iter().map(|(k, l, w, n, s)| json!({ "key": k, "label": l, "weight": w, "tasks": n, "score": s })).collect::<Vec<_>>(),
        "tasks": results.iter().map(|r| json!({ "name": r.name, "category": r.category, "score": r.score, "notes": r.notes })).collect::<Vec<_>>(),
    });
    std::fs::write(out_dir.join("results.json"), serde_json::to_string_pretty(&json)?)?;
    let mut md = String::from("# Task benchmark results\n\n> Written by `shapecraft-snap --bench bench/tasks`. Do not edit by hand.\n\n");
    md += &format!("**Measured overall (usage-weighted): {:.1}%** over {} tasks\n\n", overall * 100.0, results.len());
    md += "| Category | Weight | Tasks | Score |\n|---|---:|---:|---:|\n";
    for (_, label, w, n, s) in &rows {
        md += &format!("| {label} | {w} | {n} | {} |\n", s.map_or("—".into(), |s| format!("{:.0}%", s * 100.0)));
    }
    md += "\n## Hand-scored (not scriptable)\n\n";
    for (key, c) in &cats {
        for m in c["manual"].as_array().into_iter().flatten() {
            md += &format!("- {key}: {:.0}% — {}\n", m["score"].as_f64().unwrap_or(0.0) * 100.0, m["why"].as_str().unwrap_or(""));
        }
    }
    md += "\n## Tasks\n\n| Task | Category | Score | What failed |\n|---|---|---:|---|\n";
    for r in results {
        let about = if r.about.is_empty() { String::new() } else { format!(" — {}", r.about) };
        md += &format!("| {}{about} | {} | {:.0}% | {} |\n", r.name, r.category, r.score * 100.0, r.notes.join("<br>").replace('|', "\\|"));
    }
    std::fs::write(out_dir.join("RESULTS.md"), md)?;
    println!("overall {:.1}% → {}", overall * 100.0, out_dir.join("RESULTS.md").display());
    Ok(())
}
