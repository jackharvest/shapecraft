//! Imports run on their own thread (#22): a big STEP or .shapr file can take minutes in the kernel, and the window
//! must keep drawing and answering meanwhile. The import works on a fork of the session; when it ends, the fork
//! (with the import in its undo history) takes the session's place. Until then the window shows the import, and
//! nothing else edits the project. Cancel stops the kernel at its next check and drops whatever comes back.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;
use shapecraft_engine::{EngineError, Outcome, Session};
use shapecraft_render::StandardView;

use super::Workspace;
use crate::theme::{self, Tokens};

type Done = (Session, Result<Outcome, EngineError>);

/// The import in flight.
pub struct Importing {
    pub file: String,
    started: Instant,
    rx: Receiver<Done>,
}

impl Workspace {
    /// Starts importing on a worker thread; the result comes in through [`Workspace::poll_import`].
    pub fn start_import(&mut self, params: Value, file: String) {
        if self.importing.is_some() {
            self.toast("Wait for the import that's running to finish");
            return;
        }
        let mut fork = self.session.fork();
        let (tx, rx) = channel();
        fork.kernel.cancel(false);
        let started = std::thread::Builder::new().name("import".into()).spawn(move || {
            fork.kernel.cancellable(true);
            let out = fork.execute("import_file", &params);
            let _ = tx.send((fork, out));
        });
        match started {
            Ok(_) => self.importing = Some(Importing { file, started: Instant::now(), rx }),
            Err(e) => self.toast(format!("Couldn't start the import: {e}")),
        }
    }

    /// Takes a finished import in. Returns true while one is still running.
    pub(super) fn poll_import(&mut self, ctx: &egui::Context) -> bool {
        let Some(imp) = &self.importing else { return false };
        match imp.rx.try_recv() {
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.importing = None;
                self.toast("The import stopped without a result");
                false
            }
            Ok((fork, out)) => {
                self.importing = None;
                match out {
                    Ok(out) => {
                        self.session = fork;
                        self.remember("import_file");
                        self.imported(&out.value);
                    }
                    Err(EngineError::NotAvailable(m) | EngineError::BadParams(m)) => self.toast(m),
                    Err(e) => self.toast(e.to_string()),
                }
                false
            }
        }
    }

    /// After an import: the default view, and a word about what was left out or cut short.
    pub fn imported(&mut self, done: &Value) {
        self.standard_view(StandardView::Default);
        // Solids with geometry we can't read yet are left out; say so rather than lose them quietly.
        let skipped = done["unreadable"].as_array().map_or(0, Vec::len);
        if skipped > 0 {
            self.toast(format!(
                "{skipped} {} in this file couldn't be read yet and {} left out",
                if skipped == 1 { "solid" } else { "solids" },
                if skipped == 1 { "was" } else { "were" }
            ));
        }
        let cut = done["repair_cut"].as_u64().unwrap_or(0);
        if cut > 0 {
            self.toast(format!(
                "Repairing {cut} {} took too long; {} came in with less repair",
                if cut == 1 { "body" } else { "bodies" },
                if cut == 1 { "it" } else { "they" }
            ));
        }
    }

    /// Cancel: the kernel stops at its next check; the thread's result, whenever it comes, is dropped.
    fn cancel_import(&mut self) {
        if self.importing.take().is_some() {
            self.session.kernel.cancel(true);
            self.toast("Import canceled");
        }
    }
}

/// While an import runs, the window below the title bar turns into a dark backdrop with a soft blue glow and a warm
/// core, a round badge with a turning blue arc, "Importing", the file's name and what's happening, and a quiet
/// Cancel underneath (Escape too).
pub(super) fn card(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(imp) = &ws.importing else { return };
    let secs = imp.started.elapsed().as_secs_f32();
    let file = imp.file.clone();
    let title_h = match (crate::tablet::touch_ui(), full.height() > full.width()) {
        (false, _) => Tokens::TITLEBAR_H,
        (true, true) => 44.0,
        (true, false) => 0.0,
    };
    let body = Rect::from_min_max(full.min + vec2(0.0, title_h), full.max);
    let p = ui.painter().clone();
    p.rect_filled(body, 0.0, theme::c(0x272728));
    let mid = body.center() + vec2(0.0, -0.02 * body.height());
    // Glows: radial fans, their colour at the middle fading to nothing at the rim.
    glow(
        &p,
        mid + vec2(0.0, 40.0),
        vec2(0.30 * body.width(), 0.30 * body.height()),
        Color32::from_rgba_unmultiplied(0x25, 0x2b, 0x48, 230),
    );
    glow(&p, mid + vec2(0.0, 105.0), vec2(150.0, 95.0), Color32::from_rgba_unmultiplied(0x66, 0x40, 0x1c, 150));
    // The badge: a dark disc with a darker rim, our import mark inside, a blue arc going round its edge.
    let c = mid + vec2(0.0, -70.0);
    let r = 34.0;
    p.circle_filled(c, r, theme::c(0x1c1c1e));
    p.circle_filled(c, r - 3.0, theme::c(0x2c2c2e));
    crate::icons::paint(&p, Rect::from_center_size(c, vec2(28.0, 28.0)), crate::icons::Icon::Import, Color32::WHITE);
    let start = secs * 4.0;
    let arc: Vec<egui::Pos2> = (0..=24)
        .map(|k| {
            let a = start + k as f32 / 24.0 * 1.1;
            c + vec2(a.cos(), a.sin()) * (r - 1.5)
        })
        .collect();
    p.add(egui::Shape::line(arc, Stroke::new(3.0, theme::c(0x2e68ff))));
    p.text(mid + vec2(0.0, -10.0), Align2::CENTER_CENTER, "Importing", theme::strong(21.0), Color32::WHITE);
    p.text(mid + vec2(0.0, 22.0), Align2::CENTER_CENTER, file, theme::body(14.0), Color32::WHITE);
    p.text(mid + vec2(0.0, 50.0), Align2::CENTER_CENTER, "Reading the file…", theme::body(14.0), theme::c(0xa1a1a6));
    let g = p.layout_no_wrap("Cancel".to_string(), theme::body(13.0), theme::c(0x8e8e93));
    let cancel = Rect::from_center_size(mid + vec2(0.0, 92.0), g.size() + vec2(16.0, 8.0));
    let cr = ui.interact(cancel, ui.scope_id().with("import-running-cancel"), Sense::click());
    let ink = if cr.hovered() { Color32::WHITE } else { theme::c(0x8e8e93) };
    p.text(cancel.center(), Align2::CENTER_CENTER, "Cancel", theme::body(13.0), ink);
    if cr.hovered() {
        p.line_segment(
            [pos2(cancel.left() + 8.0, cancel.bottom() - 3.0), pos2(cancel.right() - 8.0, cancel.bottom() - 3.0)],
            Stroke::new(1.0, ink),
        );
    }
    if cr.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.cancel_import();
    }
}

/// A soft elliptical glow: `color` at `center`, fading to transparent at `radii`.
fn glow(p: &egui::Painter, center: egui::Pos2, radii: egui::Vec2, color: Color32) {
    let mut mesh = egui::Mesh::default();
    let n = 64;
    // Two rings so the fall-off isn't a straight cone: half strength at 55% of the way out.
    let rings = [(0.55, color.gamma_multiply(0.5)), (1.0, Color32::TRANSPARENT)];
    mesh.colored_vertex(center, color);
    for (f, col) in rings {
        for k in 0..n {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            mesh.colored_vertex(center + vec2(a.cos() * radii.x, a.sin() * radii.y) * f, col);
        }
    }
    for k in 0..n {
        let next = (k + 1) % n;
        mesh.add_triangle(0, 1 + k, 1 + next);
        let (a, b, c, d) = (1 + k, 1 + next, 1 + n + k, 1 + n + next);
        mesh.add_triangle(a, b, d);
        mesh.add_triangle(a, d, c);
    }
    p.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    /// Cancel is one flag for the whole kernel (the app runs one import at a time): these tests take turns.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn workspace() -> Workspace {
        let session = Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Import");
        Workspace::new(session, crate::settings::Settings::default())
    }

    /// A STEP file with one 10×20×5 block in it, made by the app itself.
    fn step_file(dir: &std::path::Path) -> std::path::PathBuf {
        let mut s = Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Block");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 20]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
        let path = dir.join("block.step");
        s.execute("export_step", &json!({"path": path})).unwrap();
        path
    }

    fn wait(ws: &mut Workspace) {
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(60);
        while ws.poll_import(&ctx) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// #22: the import runs on its own thread; the project waits (commands refused), then takes the result with
    /// one undo step.
    #[test]
    fn an_import_runs_off_the_ui_thread_and_lands_as_one_step() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("sc-import-{}-{}", std::process::id(), line!()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = step_file(&dir);
        let mut ws = workspace();
        ws.start_import(json!({ "path": path, "heal": "quality" }), "block.step".into());
        assert!(ws.importing.is_some());
        assert!(ws.run("sketch", json!({"plane": "xy"})).is_none(), "nothing edits the project meanwhile");
        wait(&mut ws);
        assert!(ws.importing.is_none());
        assert_eq!(ws.session.doc.bodies.len(), 1);
        assert!(ws.session.can_undo());
        ws.session.execute("undo", &json!({})).unwrap();
        assert!(ws.session.doc.bodies.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cancel drops whatever the thread brings back, and the project is as it was.
    #[test]
    fn a_canceled_import_leaves_the_project_alone() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("sc-import-{}-{}", std::process::id(), line!()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = step_file(&dir);
        let mut ws = workspace();
        ws.start_import(json!({ "path": path, "heal": "quality" }), "block.step".into());
        ws.cancel_import();
        assert!(ws.importing.is_none());
        std::thread::sleep(Duration::from_millis(200));
        assert!(ws.session.doc.bodies.is_empty());
        assert!(!ws.session.can_undo());
        ws.session.kernel.cancel(false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
