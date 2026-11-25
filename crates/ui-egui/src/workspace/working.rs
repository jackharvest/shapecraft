//! Heavy steps off the window's thread (#59). Some steps (Offset Face first) can take minutes, or on some parts far
//! longer, on a slow processor; run on the thread that draws the window, the desktop decides the app has hung. They
//! run on a fork of the session instead, as imports do. A step that's done within a moment lands the same frame, as
//! before; a longer one shows a small card with Cancel while the window keeps drawing, and its fork takes the
//! session's place when it ends. Live previews of those steps work the same way, with a time limit, and the newest
//! value is worked out once the one before is back.

use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::Value;
use shapecraft_doc::Document;
use shapecraft_engine::{EngineError, Outcome, Session};

use super::{Preview, Workspace};
use crate::theme::{self, Tokens, radius};

/// The steps that run off the window's thread: the pulled tools whose kernel work can run long on a slow processor.
pub(crate) const HEAVY: &[&str] = &["offset_face", "shell", "fillet", "chamfer", "thread", "loft", "fill"];
/// The History edits that rebuild the model from their step on: a late edit of a long history, or of an import's
/// steps, runs many steps again. They work off the window's thread the same way (but are no live step: the next
/// tool starts its own).
pub(crate) const HISTORY_EDITS: &[&str] = &["edit_step", "suppress_step", "delete_step", "set_breakpoint", "set_variable"];
/// How long the window waits for a step before it shows the card and lets it finish in the background.
pub(super) const WAIT_INLINE: Duration = Duration::from_millis(150);
/// The same for a live preview: the drag keeps its feel when previews are quick.
const PREVIEW_INLINE: Duration = Duration::from_millis(60);
/// A preview that takes longer than this is given up (the value can still be applied, with Cancel at hand).
const PREVIEW_LIMIT_S: f64 = 20.0;

type Done = (Session, Result<Outcome, EngineError>);

/// A heavy step running in the background.
pub struct Working {
    id: &'static str,
    /// A pulled tool's step (the next pull amends it), as against a History edit.
    live: bool,
    label: String,
    started: Instant,
    rx: Receiver<Done>,
    /// What the step was asked (for a report if it fails).
    params: Value,
}

/// A live preview being worked out in the background: its key and where the document comes back.
pub(crate) struct PreviewJob {
    key: String,
    rx: Receiver<Result<Document, EngineError>>,
}

/// How long a failed step may go on looking for a value that works, and how many it tries.
const ADVICE_BUDGET: Duration = Duration::from_millis(800);
const ADVICE_TRIES: usize = 7;

/// After a wall, radius or distance that didn't work: a smaller one that does, rounded down to read well and tried
/// as written (`None` when none is found in time). Only a step being applied asks, never a live preview.
fn working_value(s: &mut Session, id: &str, params: &Value, amend: bool) -> Option<String> {
    let key = match id {
        "shell" => "thickness",
        "fillet" if params.get("radius2").is_none_or(Value::is_null) => "radius",
        "chamfer" if params.get("distance2").is_none_or(Value::is_null) => "distance",
        "offset_face" if params.get("distance_type").and_then(Value::as_str).is_none_or(|t| t == "offset") => "distance",
        _ => return None,
    };
    let asked = params.get(key)?.as_f64().filter(|v| v.is_finite() && v.abs() > 1e-6)?;
    let units = s.doc.units;
    let until = Instant::now() + ADVICE_BUDGET;
    let mut works = |v: f64| {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let mut p = params.clone();
        p[key] = serde_json::json!(v);
        s.kernel.time_limit(Some(left.as_secs_f64()));
        let ok = if amend { s.preview_amend(id, &p) } else { s.preview(id, &p) }.is_ok();
        s.kernel.time_limit(None);
        Some(ok)
    };
    let (mut good, mut bad) = (0.0, asked);
    for _ in 0..ADVICE_TRIES {
        let mid = (good + bad) / 2.0;
        match works(mid) {
            Some(true) => good = mid,
            Some(false) => bad = mid,
            None => break,
        }
    }
    // Two figures in the project's unit, toward zero.
    let shown = good / units.scale();
    if shown == 0.0 {
        return None;
    }
    let step = 10f64.powf(shown.abs().log10().floor() - 1.0);
    let rounded = (shown / step).trunc() * step * units.scale();
    works(rounded)?.then(|| shapecraft_doc::format_length(rounded, units))
}

impl Workspace {
    /// Runs a heavy step (`amend`: in place of the last one) on a fork. Some(result) when it was done within a
    /// moment (the session already holds it), None while it goes on in the background.
    pub(crate) fn run_heavy(&mut self, id: &'static str, params: Value, amend: bool) -> Option<Result<Outcome, EngineError>> {
        let mut fork = self.session.fork();
        let asked = params.clone();
        let (tx, rx) = channel();
        self.session.kernel.cancel(false);
        // SHAPECRAFT_HEAVY_DELAY_MS (debug builds): the step waits first, to see the card on a quick part.
        let delay = if cfg!(debug_assertions) {
            std::env::var("SHAPECRAFT_HEAVY_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0).min(60_000)
        } else {
            0
        };
        let spawned = std::thread::Builder::new().name(format!("step {id}")).spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            fork.kernel.cancellable(true);
            let out = if amend { fork.amend(id, &params) } else { fork.execute(id, &params) };
            let out = out.map_err(|e| match working_value(&mut fork, id, &params, amend) {
                Some(v) => EngineError::NotAvailable(format!("{}. Try {v}.", e.to_string().trim_end_matches('.'))),
                None => e,
            });
            let _ = tx.send((fork, out));
        });
        if let Err(e) = spawned {
            return Some(Err(EngineError::NotAvailable(format!("couldn't start working: {e}"))));
        }
        match rx.recv_timeout(self.heavy_wait) {
            Ok((fork, out)) => {
                if out.is_ok() {
                    self.session = fork;
                }
                Some(out)
            }
            Err(RecvTimeoutError::Timeout) => {
                let label = if HISTORY_EDITS.contains(&id) {
                    "Rebuilding the History".to_string()
                } else {
                    shapecraft_engine::cmd::find(id).map_or(id, |c| c.label).to_string()
                };
                log::info!("{label}: working on it in the background");
                self.working = Some(Working { id, live: HEAVY.contains(&id), label, started: Instant::now(), rx, params: asked });
                None
            }
            Err(RecvTimeoutError::Disconnected) => Some(Err(EngineError::Panicked(id.to_string()))),
        }
    }

    /// Takes a finished background step in. Returns true while one is still running.
    pub(super) fn poll_working(&mut self, ctx: &egui::Context) -> bool {
        let Some(w) = &self.working else { return false };
        match w.rx.try_recv() {
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.working = None;
                self.toast("The step stopped without a result");
                false
            }
            Ok((fork, out)) => {
                let Some(w) = self.working.take() else { return false };
                match out {
                    Ok(out) => {
                        self.session = fork;
                        self.remember(w.id);
                        self.say_note(&out.value);
                        if w.live {
                            self.live_step = Some(self.session.revision);
                        } else {
                            self.live_step = None;
                        }
                        self.preview = None;
                    }
                    Err(EngineError::NotAvailable(m) | EngineError::BadParams(m)) => self.fail_toast(w.id, &w.params, m),
                    Err(e) => self.fail_toast(w.id, &w.params, e.to_string()),
                }
                false
            }
        }
    }

    /// Cancel: the kernel stops at its next check, and whatever the step still sends back is dropped.
    fn cancel_working(&mut self) {
        if let Some(w) = self.working.take() {
            self.session.kernel.cancel(true);
            log::info!("{}: canceled after {:.1} s", w.label, w.started.elapsed().as_secs_f32());
            self.toast(format!("{} canceled", w.label));
            self.preview = None;
        }
    }

    /// The live preview of a heavy step: worked out on a fork with a time limit. The document shows as soon as it's
    /// back; until then the last one stays.
    pub(super) fn heavy_preview(&mut self, id: &'static str, params: &Value, key: String) {
        if let Some(job) = &self.preview_job {
            match job.rx.try_recv() {
                Ok(doc) => {
                    let key = job.key.clone();
                    self.preview_job = None;
                    self.preview = Some(Preview::new(key, self.now, doc));
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => self.preview_job = None,
            }
        }
        if self.preview.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        let mut fork = self.session.fork_for_preview();
        let amend = self.is_live();
        let params = params.clone();
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new().name(format!("preview {id}")).spawn(move || {
            fork.kernel.time_limit(Some(PREVIEW_LIMIT_S));
            let doc = if amend { fork.preview_amend(id, &params) } else { fork.preview(id, &params) };
            fork.kernel.time_limit(None);
            let _ = tx.send(doc);
        });
        if spawned.is_err() {
            return;
        }
        match rx.recv_timeout(PREVIEW_INLINE) {
            Ok(doc) => self.preview = Some(Preview::new(key, self.now, doc)),
            Err(_) => self.preview_job = Some(PreviewJob { key, rx }),
        }
    }

    /// Whether a live preview is still being worked out (the view keeps drawing meanwhile).
    pub(crate) fn preview_pending(&self) -> bool {
        self.preview_job.is_some()
    }
}

/// The card while a step works in the background: what's running, for how long, and Cancel (Escape too). The model
/// stays in view behind it.
pub(super) fn card(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(w) = &ws.working else { return };
    let secs = w.started.elapsed().as_secs_f32();
    let label = w.label.clone();
    let r = Rect::from_center_size(pos2(full.center().x, full.bottom() - 150.0), vec2(320.0, 92.0));
    let p = ui.painter().clone();
    p.rect_filled(r.translate(vec2(0.0, 5.0)), radius(12), Color32::from_black_alpha(80));
    p.rect_filled(r, radius(12), Tokens::panel());
    p.rect_stroke(r, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
    // A turning arc, as on the import screen.
    let c = pos2(r.left() + 34.0, r.top() + 32.0);
    p.circle_stroke(c, 11.0, Stroke::new(3.0, Tokens::chip()));
    let start = secs * 4.0;
    let arc: Vec<egui::Pos2> = (0..=16)
        .map(|k| {
            let a = start + k as f32 / 16.0 * 1.6;
            c + vec2(a.cos(), a.sin()) * 11.0
        })
        .collect();
    p.add(egui::Shape::line(arc, Stroke::new(3.0, Tokens::accent())));
    p.text(
        pos2(r.left() + 58.0, r.top() + 24.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr_s(format!("{label}…")),
        theme::strong(14.0),
        Tokens::text_strong(),
    );
    let took =
        if secs < 60.0 { format!("Working for {secs:.0} s") } else { format!("Working for {:.0} min {:.0} s", secs / 60.0, secs % 60.0) };
    p.text(pos2(r.left() + 58.0, r.top() + 43.0), Align2::LEFT_CENTER, took, theme::body(12.0), Tokens::text_dim());
    let b = Rect::from_min_max(pos2(r.right() - 96.0, r.bottom() - 38.0), pos2(r.right() - 14.0, r.bottom() - 12.0));
    let resp = ui.interact(b, ui.scope_id().with("working-cancel"), Sense::click());
    p.rect_filled(b, radius(7), if resp.hovered() { Tokens::button_hover() } else { Tokens::button() });
    p.text(b.center(), Align2::CENTER_CENTER, crate::i18n::tr("Cancel"), theme::strong(13.0), Tokens::text_strong());
    ui.ctx().request_repaint();
    if resp.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.cancel_working();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    /// A 20 mm block and the face on top of it.
    fn block() -> (Workspace, u64, u32) {
        let mut s = Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Heavy");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
        let v = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 20})).unwrap().value;
        let body = v["body"].as_u64().unwrap();
        let b = s.doc.body(shapecraft_doc::ItemId(body)).unwrap().shape.clone();
        let top =
            (0..s.kernel.face_count(&b).unwrap()).find(|f| s.kernel.face_plane(&b, *f).unwrap().is_some_and(|p| p.normal.z > 0.9)).unwrap();
        (Workspace::new(s, crate::settings::Settings::default()), body, top)
    }

    fn volume(ws: &Workspace) -> f64 {
        let b = &ws.session.doc.bodies[0];
        ws.session.kernel.mass_props(&b.shape).unwrap().volume
    }

    /// A wall or a radius that's too big says one that works, and that one does.
    #[test]
    fn a_failed_step_says_a_value_that_works() {
        for (id, params, key) in [
            ("shell", json!({"thickness": 12}), "thickness"),
            ("fillet", json!({"radius": 35, "edges": [0]}), "radius"),
            ("chamfer", json!({"distance": 35, "edges": [0]}), "distance"),
        ] {
            let (mut ws, body, top) = block();
            ws.heavy_wait = Duration::from_secs(20);
            let mut params = params;
            params["body"] = json!(body);
            if id == "shell" {
                params["faces"] = json!([top]);
            }
            assert!(!ws.run_live(id, params.clone(), false), "{id} is too big");
            let said = ws.toasts.last().map(|t| t.text.clone()).unwrap_or_default();
            let v: f64 = said.rsplit_once("Try ").and_then(|(_, t)| t.split(' ').next()?.parse().ok()).unwrap_or_else(|| panic!("{said}"));
            assert!(v > 1.0, "{said}");
            params[key] = json!(v);
            assert!(ws.run_live(id, params, false), "{id}: {v} works");
        }
    }

    /// A step that works says nothing.
    #[test]
    fn a_good_step_gets_no_advice() {
        let (mut ws, body, top) = block();
        ws.heavy_wait = Duration::from_secs(20);
        assert!(ws.run_live("shell", json!({"body": body, "faces": [top], "thickness": 2}), false));
        assert!(ws.toasts.is_empty());
    }

    /// #59: a quick Offset Face lands at once, as before; pulled again it replaces its own step.
    #[test]
    fn a_quick_offset_lands_the_same_frame() {
        let (mut ws, body, top) = block();
        assert!(ws.run_live("offset_face", json!({"body": body, "faces": [top], "distance": 5}), false));
        assert!(ws.working.is_none());
        assert!((volume(&ws) - 20.0 * 20.0 * 25.0).abs() < 1e-3);
        let steps = ws.session.undo_depth();
        assert!(ws.run_live("offset_face", json!({"body": body, "faces": [top], "distance": 2}), false));
        assert_eq!(ws.session.undo_depth(), steps, "a second pull amends the first");
        assert!((volume(&ws) - 20.0 * 20.0 * 22.0).abs() < 1e-3);
    }

    /// #59: one that takes longer goes on in the background: the window keeps going, edits wait, and the result
    /// takes the session's place once it's back.
    #[test]
    fn a_long_offset_finishes_in_the_background() {
        let (mut ws, body, top) = block();
        ws.heavy_wait = Duration::ZERO;
        assert!(ws.run_live("offset_face", json!({"body": body, "faces": [top], "distance": 5}), false));
        // Nothing else edits the project meanwhile (the fork's result would replace it).
        if ws.working.is_some() {
            assert!(ws.run("select_all", json!({})).is_none());
        }
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(30);
        while ws.poll_working(&ctx) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ws.working.is_none());
        assert!((volume(&ws) - 20.0 * 20.0 * 25.0).abs() < 1e-3);
        assert_eq!(ws.live_step, Some(ws.session.revision), "the next pull amends it");
    }

    /// #59: Shell, Fillet and Chamfer go the same way: each finishes in the background when it takes a while, and a
    /// second pull of the same tool replaces its step.
    #[test]
    fn shell_fillet_and_chamfer_work_in_the_background_too() {
        let finish = |ws: &mut Workspace| {
            let ctx = egui::Context::default();
            let until = Instant::now() + Duration::from_secs(60);
            while ws.poll_working(&ctx) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(ws.working.is_none());
        };
        let (mut ws, body, top) = block();
        ws.heavy_wait = Duration::ZERO;
        assert!(ws.run_live("shell", json!({"body": body, "faces": [top], "thickness": 2}), false));
        finish(&mut ws);
        let solid = 20.0 * 20.0 * 20.0;
        assert!((volume(&ws) - (solid - 16.0 * 16.0 * 18.0)).abs() < 1e-3, "{}", volume(&ws));
        let steps = ws.session.undo_depth();
        assert!(ws.run_live("shell", json!({"body": body, "faces": [top], "thickness": 1}), false));
        finish(&mut ws);
        assert_eq!(ws.session.undo_depth(), steps, "a second pull amends the first");
        assert!((volume(&ws) - (solid - 18.0 * 18.0 * 19.0)).abs() < 1e-3, "{}", volume(&ws));

        for (id, key) in [("fillet", "radius"), ("chamfer", "distance")] {
            let (mut ws, body, _) = block();
            ws.heavy_wait = Duration::ZERO;
            assert!(ws.run_live(id, json!({"body": body, key: 2, "edges": [0]}), false));
            finish(&mut ws);
            let v = volume(&ws);
            assert!(v < solid - 0.5 && v > solid * 0.95, "{id}: {v}");
        }
    }

    /// A History edit that takes a while goes on in the background like a heavy step, lands once it's back, is one
    /// undo, and leaves no live step for the next tool to amend.
    #[test]
    fn a_history_edit_finishes_in_the_background() {
        let (mut ws, _, _) = block();
        let undo = ws.session.undo_depth();
        ws.heavy_wait = Duration::ZERO;
        assert!(ws.run("edit_step", json!({"step": 1, "params": {"distance": 10}})).is_some() || ws.working.is_some());
        if ws.working.is_some() {
            assert!(ws.run("select_all", json!({})).is_none(), "nothing else edits the project meanwhile");
        }
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(30);
        while ws.poll_working(&ctx) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ws.working.is_none());
        assert!((volume(&ws) - 20.0 * 20.0 * 10.0).abs() < 1e-3, "{}", volume(&ws));
        assert_eq!(ws.live_step, None);
        assert_eq!(ws.session.undo_depth(), undo + 1);
        ws.session.execute("undo", &json!({})).unwrap();
        assert!((volume(&ws) - 20.0 * 20.0 * 20.0).abs() < 1e-3);
    }

    /// #59, last step: a Union that takes a while goes on in the background behind the card, lands once it's
    /// back, puts the tool down at once, and is one undo.
    #[test]
    fn a_boolean_finishes_in_the_background() {
        let (mut ws, first, _) = block();
        ws.session.execute("sketch", &json!({"plane": "xy"})).unwrap();
        ws.session.execute("rectangle", &json!({"corner": [10, 10], "opposite": [30, 30]})).unwrap();
        let k = ws.session.active_sketch.unwrap().0;
        let v = ws.session.execute("extrude", &json!({"sketch": k, "region": 0, "distance": 20, "result": "new"})).unwrap().value;
        let second = v["body"].as_u64().unwrap();
        let undo = ws.session.undo_depth();
        ws.heavy_wait = Duration::ZERO;
        let ids = [shapecraft_doc::ItemId(first), shapecraft_doc::ItemId(second)];
        use crate::workspace::tools::{BooleanTool, Tool};
        ws.tool = Tool::Boolean(BooleanTool { op: "union", targets: ids.to_vec(), tools: Vec::new(), keep: 0 });
        ws.commit_tool();
        assert!(matches!(ws.tool, Tool::Idle), "the tool is put down while the union works");
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(30);
        while ws.poll_working(&ctx) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ws.working.is_none());
        assert_eq!(ws.session.doc.bodies.len(), 1, "joined");
        assert!((volume(&ws) - (8000.0 + 8000.0 - 2000.0)).abs() < 1e-3, "{}", volume(&ws));
        assert_eq!(ws.session.undo_depth(), undo + 1);
    }
}
