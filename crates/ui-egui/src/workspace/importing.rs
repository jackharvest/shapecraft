//! Imports run on their own thread (#22): a big STEP or .shapr file can take minutes in the kernel, and the window
//! must keep drawing and answering meanwhile. The import works on a fork of the session; when it ends, the fork
//! (with the import in its undo history) takes the session's place. Until then a card shows the file and the time,
//! and nothing else edits the project. Cancel stops the kernel at its next check and drops whatever comes back.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::Value;
use shapecraft_engine::{EngineError, Outcome, Session};
use shapecraft_render::StandardView;

use super::Workspace;
use crate::theme::{self, Tokens, radius};

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

/// The card over the project while an import runs: the file, the time so far, a moving bar and Cancel.
pub(super) fn card(ws: &mut Workspace, ui: &mut Ui, full: Rect) {
    let Some(imp) = &ws.importing else { return };
    let secs = imp.started.elapsed().as_secs_f32();
    let file = imp.file.clone();
    let r = Rect::from_center_size(full.center(), vec2(380.0, 132.0));
    let p = ui.painter().clone();
    p.rect_filled(full, 0.0, Color32::from_black_alpha(60));
    p.rect_filled(r.translate(vec2(0.0, 6.0)).expand(2.0), radius(12), Color32::from_black_alpha(90));
    p.rect_filled(r, radius(10), theme::c(0x1e1e1e));
    p.rect_stroke(r, radius(10), Stroke::new(1.0, theme::c(0x4b4b4b)), StrokeKind::Outside);
    p.text(r.left_top() + vec2(20.0, 26.0), Align2::LEFT_CENTER, "Importing", theme::strong(15.0), Tokens::text_strong());
    let name = p.layout_no_wrap(file, theme::body(12.0), Tokens::text_dim());
    let clip = Rect::from_min_max(r.left_top() + vec2(20.0, 40.0), pos2(r.right() - 20.0, r.top() + 58.0));
    p.with_clip_rect(clip).galley(clip.min, name, Tokens::text_dim());
    // An indeterminate bar: the kernel can't say how far along it is.
    let bar = Rect::from_min_max(pos2(r.left() + 20.0, r.top() + 70.0), pos2(r.right() - 20.0, r.top() + 74.0));
    p.rect_filled(bar, radius(2), theme::c(0x343436));
    let w = bar.width() * 0.3;
    let x = bar.left() + (bar.width() + w) * ((secs * 0.6).fract()) - w;
    let seg = Rect::from_min_max(pos2(x.max(bar.left()), bar.top()), pos2((x + w).min(bar.right()), bar.bottom()));
    if seg.width() > 0.0 {
        p.rect_filled(seg, radius(2), theme::c(0x2f55d4));
    }
    let fy = r.bottom() - 26.0;
    p.text(pos2(r.left() + 20.0, fy), Align2::LEFT_CENTER, format!("{secs:.0} s"), theme::body(12.0), Tokens::text_dim());
    let cancel = Rect::from_min_size(pos2(r.right() - 20.0 - 62.0, fy - 10.5), vec2(62.0, 21.0));
    let cr = ui.interact(cancel, ui.scope_id().with("import-running-cancel"), Sense::click());
    p.rect_filled(cancel, radius(5), if cr.hovered() { theme::c(0x6a6a6a) } else { theme::c(0x565656) });
    p.text(cancel.center(), Align2::CENTER_CENTER, "Cancel", theme::body(13.0), Color32::WHITE);
    if cr.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.cancel_import();
    }
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
