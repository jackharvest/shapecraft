//! Help ▸ Report a Problem… / Suggest an Idea…, a failed step's Report…, and the crash prompt: one sheet that files a
//! GitHub issue under the person's own name without leaving ShapeCraft.
//!
//! The person writes, sees exactly what goes (every detail is a switch, and Preview shows the text as sent), and
//! presses Send. The first time, Send signs in with GitHub: a short code to approve on github.com, from any browser.
//! GitHub's API can't attach pictures, so a screenshot goes on the clipboard to paste into the issue afterwards.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use shapecraft_report::account::{self, Account};
use shapecraft_report::draft::{Context, Draft, Failure, Include, Kind};
use shapecraft_report::github::{self, DeviceCode, Filed, Mine, Poll};

use crate::theme::{self, Tokens, radius};

/// What opened the sheet.
#[derive(Clone, Debug)]
pub enum Seed {
    Problem,
    Idea,
    /// A failed step's Report… (from its message).
    Failure(Failure),
    /// The prompt after ShapeCraft closed unexpectedly: the crash record.
    Crash(String),
}

/// What the background thread answers.
enum Msg {
    Code(Result<DeviceCode, String>),
    SignedIn(Result<Account, String>),
    Filed(Result<Filed, String>),
    Mine(Result<Vec<Mine>, String>),
}

#[derive(Clone, Default)]
struct Mailbox(Arc<Mutex<Vec<Msg>>>);

impl Mailbox {
    fn send(&self, m: Msg) {
        if let Ok(mut q) = self.0.lock() {
            q.push(m);
        }
    }
    fn take(&self) -> Vec<Msg> {
        self.0.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Compose,
    /// Waiting for GitHub's code, or for the person to approve it (`code` once it came).
    SignIn(Option<DeviceCode>),
    Sending,
    Sent(Filed),
    Mine,
}

/// The screenshot taken as the sheet opened (before it drew).
struct Shot {
    image: egui::ColorImage,
    texture: egui::TextureHandle,
}

/// The sheet and everything it holds between frames (a closed sheet keeps its draft until it's sent).
#[derive(Default)]
pub struct Report {
    pub open: bool,
    phase: Option<Phase>,
    draft: Draft,
    ctx: Context,
    inc: Include,
    preview: bool,
    shot: Option<Shot>,
    /// A screenshot was asked for at this time and hasn't come yet: the sheet waits (briefly) so it isn't in it.
    shot_asked: Option<f64>,
    shot_slot: Arc<Mutex<Option<egui::ColorImage>>>,
    account: Option<Account>,
    account_read: bool,
    mail: Mailbox,
    /// Stops a sign-in that's waiting for approval.
    stop: Arc<AtomicBool>,
    error: Option<String>,
    mine: Option<Result<Vec<Mine>, String>>,
    /// Send was pressed and goes on by itself after the sign-in.
    send_after_sign_in: bool,
    focus_once: bool,
    /// A pasting step is offered after sending (the screenshot went on the clipboard).
    shot_copied: bool,
    /// The browser link was opened (the fallback), so the clipboard holds the text.
    pub status: Option<String>,
}

/// What the app does for the sheet: open a page in the browser.
pub enum Out {
    Open(String),
}

/// Pictures can go on this clipboard (a desktop window; not a phone, a tablet's browser or a hosted copy).
pub fn can_copy_images(hosted: bool) -> bool {
    !hosted && cfg!(not(any(target_os = "android", target_os = "ios")))
}

impl Report {
    /// Opens the sheet for `seed`, with what's known about this computer. A screenshot of the window is taken first,
    /// when `screenshot` (the sheet waits for it).
    pub fn begin(&mut self, ui_ctx: &egui::Context, seed: Seed, context: Context, screenshot: bool) {
        let same_kind = self.open || !self.draft.is_empty();
        match seed {
            Seed::Problem => {
                if !same_kind {
                    self.draft = Draft::default();
                }
                self.draft.kind = Kind::Problem;
                self.ctx = Context { failure: None, crash: None, ..context };
            }
            Seed::Idea => {
                if !same_kind {
                    self.draft = Draft::default();
                }
                self.draft.kind = Kind::Idea;
                self.ctx = Context { failure: None, crash: None, ..context };
            }
            Seed::Failure(f) => {
                self.draft = Draft { kind: Kind::Problem, ..Default::default() };
                self.ctx = Context { failure: Some(f), crash: None, ..context };
            }
            Seed::Crash(c) => {
                self.draft = Draft { kind: Kind::Problem, title: "ShapeCraft closed unexpectedly".into(), ..Default::default() };
                self.ctx = Context { failure: None, crash: Some(c), ..context };
            }
        }
        self.inc = Include { screenshot, ..Include::default() };
        self.phase = Some(Phase::Compose);
        self.error = None;
        self.preview = false;
        self.focus_once = true;
        self.open = true;
        self.shot = None;
        self.shot_copied = false;
        if screenshot {
            self.shot_asked = Some(ui_ctx.input(|i| i.time));
            let slot = self.shot_slot.clone();
            let repaint = ui_ctx.clone();
            let cb = egui::ScreenshotCallback::new(move |image| {
                if let Ok(mut s) = slot.lock() {
                    *s = Some((*image).clone());
                }
                repaint.request_repaint();
            });
            ui_ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
        } else {
            self.shot_asked = None;
        }
        if !self.account_read {
            self.account = account::load();
            self.account_read = true;
        }
    }

    /// The person's own reports (Help ▸ Your Reports…).
    pub fn begin_mine(&mut self) {
        if !self.account_read {
            self.account = account::load();
            self.account_read = true;
        }
        self.open = true;
        self.phase = Some(Phase::Mine);
        self.error = None;
        self.load_mine();
    }

    /// Who is signed in, for Preferences.
    pub fn signed_in(&mut self) -> Option<String> {
        if !self.account_read {
            self.account = account::load();
            self.account_read = true;
        }
        self.account.as_ref().map(Account::handle)
    }

    pub fn sign_out(&mut self) {
        account::forget();
        self.account = None;
        self.account_read = true;
        self.mine = None;
    }

    /// Types into the open draft (scripts and offscreen pictures).
    pub fn fill(&mut self, title: &str, text: &str) {
        if !title.is_empty() {
            self.draft.title = title.to_string();
        }
        if !text.is_empty() {
            self.draft.text = text.to_string();
        }
    }

    /// Puts the sheet in a later state with made-up answers, to look at it offscreen: "sign_in", "code",
    /// "sending", "sent", "sent_shot", "mine", "error", "signed_in", "preview". Nothing goes to GitHub.
    pub fn show_state(&mut self, state: &str, login: &str) -> Result<(), String> {
        let login = if login.is_empty() { "octocat" } else { login };
        let code = DeviceCode {
            device_code: String::new(),
            user_code: "WDJB-MJHT".into(),
            verification_uri: shapecraft_report::DEVICE_PAGE.into(),
            expires_in: 900,
            interval: 5,
        };
        let filed = Filed { number: 91, url: "https://github.com/jackharvest/shapecraft/issues/91".into() };
        self.open = true;
        self.shot_asked = None;
        match state {
            "sign_in" => self.phase = Some(Phase::SignIn(None)),
            "code" => self.phase = Some(Phase::SignIn(Some(code))),
            "sending" => self.phase = Some(Phase::Sending),
            "sent" => self.phase = Some(Phase::Sent(filed)),
            "sent_shot" => {
                self.shot_copied = true;
                self.phase = Some(Phase::Sent(filed));
            }
            "mine" => {
                self.account = Some(Account { login: login.into(), token: String::new() });
                self.account_read = true;
                self.mine = Some(Ok(vec![
                    Mine {
                        number: 91,
                        title: "Fillet: the radius is too large for this edge".into(),
                        open: true,
                        comments: 2,
                        url: filed.url.clone(),
                        updated: "2026-11-20T09:12:00Z".into(),
                    },
                    Mine {
                        number: 77,
                        title: "Gears from a pitch and a tooth count".into(),
                        open: true,
                        comments: 0,
                        url: filed.url.clone(),
                        updated: "2026-11-18T16:40:00Z".into(),
                    },
                    Mine {
                        number: 52,
                        title: "The Items list jumps when a folder opens".into(),
                        open: false,
                        comments: 4,
                        url: filed.url,
                        updated: "2026-11-09T11:02:00Z".into(),
                    },
                ]));
                self.phase = Some(Phase::Mine);
            }
            "error" => {
                self.phase = Some(Phase::Compose);
                self.error = Some("GitHub didn't open the issue: Validation Failed (422)".into());
            }
            "signed_in" => {
                self.account = Some(Account { login: login.into(), token: String::new() });
                self.account_read = true;
            }
            "preview" => self.preview = true,
            other => {
                return Err(format!("show is sign_in, code, sending, sent, sent_shot, mine, error, signed_in or preview, not '{other}'"));
            }
        }
        Ok(())
    }

    /// Keys typed while the sheet is up belong to it.
    pub fn takes_keys(&self) -> bool {
        self.open
    }

    fn load_mine(&mut self) {
        let Some(a) = self.account.clone() else {
            self.mine = None;
            return;
        };
        self.mine = None;
        let mail = self.mail.clone();
        let _ = std::thread::Builder::new().name("report-mine".into()).spawn(move || {
            mail.send(Msg::Mine(github::my_reports(&a.token, &a.login)));
        });
    }

    fn start_sign_in(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();
        self.phase = Some(Phase::SignIn(None));
        self.error = None;
        let mail = self.mail.clone();
        let _ = std::thread::Builder::new().name("report-sign-in".into()).spawn(move || {
            let code = match github::start_sign_in() {
                Ok(c) => c,
                Err(e) => return mail.send(Msg::Code(Err(e))),
            };
            mail.send(Msg::Code(Ok(code.clone())));
            let started = std::time::Instant::now();
            let mut wait = code.interval;
            loop {
                for _ in 0..wait * 10 {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                if started.elapsed().as_secs() > code.expires_in {
                    return mail.send(Msg::SignedIn(Err("The code ran out before it was approved. Try again.".into())));
                }
                match github::poll_sign_in(&code) {
                    Ok(Poll::Waiting(w)) => wait = w,
                    Ok(Poll::Approved(token)) => {
                        let account = github::who(&token).map(|login| Account { login, token });
                        if let Ok(a) = &account
                            && let Err(e) = account::save(a)
                        {
                            log::warn!("report sign-in not kept for next time: {e}");
                        }
                        return mail.send(Msg::SignedIn(account));
                    }
                    Ok(Poll::Expired) => {
                        return mail.send(Msg::SignedIn(Err("The code ran out before it was approved. Try again.".into())));
                    }
                    Ok(Poll::Denied) => return mail.send(Msg::SignedIn(Err("The sign-in was canceled on GitHub.".into()))),
                    // A dropped connection while waiting: keep waiting.
                    Err(e) => {
                        log::info!("report sign-in poll: {e}");
                        wait = (wait + 5).min(30);
                    }
                }
            }
        });
    }

    fn send(&mut self) {
        let Some(a) = self.account.clone() else {
            self.send_after_sign_in = true;
            self.start_sign_in();
            return;
        };
        self.phase = Some(Phase::Sending);
        self.error = None;
        let title = self.draft.send_title(&self.ctx);
        let body = self.body();
        let mail = self.mail.clone();
        let _ = std::thread::Builder::new().name("report-send".into()).spawn(move || {
            mail.send(Msg::Filed(github::file_issue(&a.token, &title, &body)));
        });
    }

    fn body(&self) -> String {
        let inc = Include { screenshot: self.inc.screenshot && self.shot.is_some(), ..self.inc };
        self.draft.body(&self.ctx, inc, shapecraft_report::draft::home_text().as_deref())
    }

    /// Answers from the threads, once a frame.
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(image) = self.shot_slot.lock().ok().and_then(|mut s| s.take()) {
            let texture = ctx.load_texture("report-shot", image.clone(), egui::TextureOptions::LINEAR);
            self.shot = Some(Shot { image, texture });
            self.shot_asked = None;
        }
        for m in self.mail.take() {
            match m {
                Msg::Code(Ok(code)) => {
                    if matches!(self.phase, Some(Phase::SignIn(_))) {
                        self.phase = Some(Phase::SignIn(Some(code)));
                    }
                }
                Msg::Code(Err(e)) | Msg::SignedIn(Err(e)) => {
                    self.phase = Some(Phase::Compose);
                    self.send_after_sign_in = false;
                    self.error = Some(e);
                }
                Msg::SignedIn(Ok(a)) => {
                    log::info!("signed in to GitHub for reports as {}", a.handle());
                    self.account = Some(a);
                    if self.send_after_sign_in {
                        self.send_after_sign_in = false;
                        self.send();
                    } else {
                        self.phase = Some(Phase::Compose);
                    }
                }
                Msg::Filed(Ok(f)) => {
                    log::info!("report filed as issue #{}", f.number);
                    if self.inc.screenshot
                        && let Some(s) = &self.shot
                    {
                        ctx.copy_image(s.image.clone());
                        self.shot_copied = true;
                    }
                    self.draft = Draft::default();
                    self.phase = Some(Phase::Sent(f));
                }
                Msg::Filed(Err(e)) => {
                    if e == github::SIGNED_OUT {
                        account::forget();
                        self.account = None;
                    }
                    self.phase = Some(Phase::Compose);
                    self.error = Some(e);
                }
                Msg::Mine(r) => {
                    if r.as_ref().err().is_some_and(|e| e == github::SIGNED_OUT) {
                        account::forget();
                        self.account = None;
                    }
                    self.mine = Some(r);
                }
            }
            ctx.request_repaint();
        }
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.open = false;
        self.send_after_sign_in = false;
        self.shot_asked = None;
        if matches!(self.phase, Some(Phase::Sent(_))) {
            self.ctx = Context::default();
            self.shot = None;
        }
    }

    /// Draws the sheet (when open) and returns what the app should do for it.
    pub fn window(&mut self, ui: &mut Ui) -> Option<Out> {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if !self.open {
            return None;
        }
        let now = ctx.input(|i| i.time);
        // The screenshot comes a frame or two later; the sheet stays out of it (half a second at most).
        if let Some(t) = self.shot_asked {
            if now - t < 0.5 {
                ctx.request_repaint();
                return None;
            }
            self.shot_asked = None;
            self.inc.screenshot = false;
        }
        let busy =
            matches!(self.phase, Some(Phase::Sending | Phase::SignIn(_))) || (self.phase == Some(Phase::Mine) && self.mine.is_none());
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut out = None;
        let id = ui.scope_id().with("report-sheet");
        let full = ctx.content_rect();
        let touch = crate::tablet::touch_ui();
        let w = (full.width() - 24.0).min(if touch { 600.0 } else { 540.0 });
        let h = (full.height() - 40.0).clamp(200.0, 680.0);
        let card = Rect::from_center_size(full.center(), vec2(w, h));
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let mut close = escape;
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(full.min).show(&ctx, |ui| {
            let behind = ui.allocate_rect(full, Sense::click());
            let _ = behind;
            let p = ui.painter().clone();
            p.rect_filled(full, 0.0, Color32::from_black_alpha(110));
            p.rect_filled(card.translate(vec2(0.0, 8.0)), radius(14), Color32::from_black_alpha(90));
            p.rect_filled(card, radius(14), Tokens::panel());
            p.rect_stroke(card, radius(14), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
            let _ = ui.interact(card, id.with("card"), Sense::click());
            match self.phase.clone().unwrap_or(Phase::Compose) {
                Phase::Compose => self.compose(ui, card, &mut close, &mut out),
                Phase::SignIn(code) => self.sign_in_pane(ui, card, code.as_ref(), &mut out),
                Phase::Sending => spinner_pane(ui, card, now, "Sending…", "Opening the issue on GitHub."),
                Phase::Sent(f) => self.sent_pane(ui, card, &f, &mut close, &mut out),
                Phase::Mine => self.mine_pane(ui, card, &mut close, &mut out),
            }
        });
        ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
        if close {
            if matches!(self.phase, Some(Phase::SignIn(_))) {
                // Escape during the sign-in goes back to the report, not away from it.
                self.stop.store(true, Ordering::Relaxed);
                self.send_after_sign_in = false;
                self.phase = Some(Phase::Compose);
            } else if !matches!(self.phase, Some(Phase::Sending)) {
                self.close();
            }
        }
        out
    }

    fn compose(&mut self, ui: &mut Ui, card: Rect, close: &mut bool, out: &mut Option<Out>) {
        let x0 = card.left() + 20.0;
        let x1 = card.right() - 20.0;
        let p = ui.painter().clone();
        // Header: Problem | Idea, and who it goes out as.
        let head_y = card.top() + 28.0;
        let title = match self.draft.kind {
            Kind::Problem => "Report a Problem",
            Kind::Idea => "Suggest an Idea",
        };
        p.text(pos2(x0, head_y), Align2::LEFT_CENTER, crate::i18n::tr_s(title), theme::strong(16.0), Tokens::text_strong());
        if self.ctx.failure.is_none()
            && self.ctx.crash.is_none()
            && let Some(k) = segmented(ui, &p, x1, head_y, "report-kind", &["Problem", "Idea"], usize::from(self.draft.kind == Kind::Idea))
        {
            self.draft.kind = if k == 0 { Kind::Problem } else { Kind::Idea };
        }
        p.text(
            pos2(x0, head_y + 22.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("Becomes a public issue on ShapeCraft's GitHub, under your name."),
            theme::body(12.0),
            Tokens::text_dim(),
        );
        let foot_h = 64.0;
        let body = Rect::from_min_max(pos2(card.left() + 1.0, head_y + 40.0), pos2(card.right() - 1.0, card.bottom() - foot_h));
        p.hline(card.x_range(), body.top(), Stroke::new(1.0, Tokens::divider()));
        p.hline(card.x_range(), body.bottom(), Stroke::new(1.0, Tokens::divider()));

        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink2(vec2(19.0, 0.0))));
        inner.set_clip_rect(body);
        let send_key = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut inner, |ui| {
            ui.add_space(14.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            if let Some(e) = &self.error {
                error_bar(ui, e);
                ui.add_space(6.0);
            }
            if self.preview {
                let text = format!("{}\n\n{}", self.draft.send_title(&self.ctx), self.body());
                field_label(ui, "Exactly what will be sent");
                let mut shown = text;
                let h = ui.available_height().max(200.0) - 20.0;
                text_box(ui, "report-preview", &mut shown, h, "", true, false);
                return;
            }
            let failure = self.ctx.failure.clone();
            field_label(ui, "Title");
            let hint = failure.as_ref().map(Failure::title).unwrap_or_else(|| match self.draft.kind {
                Kind::Problem => "A short summary".into(),
                Kind::Idea => "Your idea in a few words".into(),
            });
            let t = text_box(ui, "report-title", &mut self.draft.title, 30.0, &hint, false, true);
            if self.focus_once && failure.is_none() {
                t.request_focus();
                self.focus_once = false;
            }
            ui.add_space(6.0);
            match self.draft.kind {
                Kind::Problem => {
                    field_label(ui, "What happened?");
                    let r = text_box(
                        ui,
                        "report-text",
                        &mut self.draft.text,
                        92.0,
                        "What you saw, and what you were trying to do",
                        false,
                        false,
                    );
                    if self.focus_once {
                        r.request_focus();
                        self.focus_once = false;
                    }
                    ui.add_space(6.0);
                    field_label(ui, "Steps to reproduce (optional)");
                    text_box(
                        ui,
                        "report-steps",
                        &mut self.draft.steps,
                        64.0,
                        "1. Draw a rectangle\n2. Extrude it 10 mm\n3. …",
                        false,
                        false,
                    );
                    ui.add_space(6.0);
                    field_label(ui, "What did you expect? (optional)");
                    text_box(ui, "report-expected", &mut self.draft.expected, 30.0, "", false, true);
                }
                Kind::Idea => {
                    field_label(ui, "Your idea");
                    let r = text_box(
                        ui,
                        "report-text",
                        &mut self.draft.text,
                        150.0,
                        "What would you like ShapeCraft to do, and what would it help you make?",
                        false,
                        false,
                    );
                    if self.focus_once {
                        r.request_focus();
                        self.focus_once = false;
                    }
                }
            }
            ui.add_space(12.0);
            field_label(ui, "Send along");
            let mut rows: Vec<(&str, String, Which)> = Vec::new();
            if self.shot.is_some() {
                rows.push(("Screenshot", "Copied for you to paste into the issue after sending".into(), Which::Shot));
            }
            if let Some(f) = &failure {
                rows.push(("What failed", f.title(), Which::Failure));
            }
            if self.ctx.crash.is_some() {
                rows.push(("Crash record", "What ShapeCraft was doing when it closed".into(), Which::Crash));
            }
            rows.push((
                "Version and system",
                [self.ctx.version.as_str(), self.ctx.system.as_str(), self.ctx.graphics.as_str()]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" · "),
                Which::System,
            ));
            if !self.ctx.log.is_empty() {
                rows.push((
                    "Recent log",
                    format!("The last {} lines, your home folder shown as ~", self.ctx.log.len().min(shapecraft_report::draft::LOG_LINES)),
                    Which::Log,
                ));
            }
            group(ui, rows.len(), |ui, k, rect| {
                let (label, sub, which) = &rows[k];
                let p = ui.painter();
                let text_right = if *which == Which::Shot { rect.right() - 128.0 } else { rect.right() - 60.0 };
                p.text(
                    pos2(rect.left() + 12.0, rect.center().y - 8.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(*label),
                    theme::body(13.0),
                    Tokens::text_strong(),
                );
                let sub = theme::elide(p, sub, theme::body(11.0), text_right - rect.left() - 12.0);
                p.text(
                    pos2(rect.left() + 12.0, rect.center().y + 9.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(sub),
                    theme::body(11.0),
                    Tokens::text_dim(),
                );
                if *which == Which::Shot
                    && let Some(s) = &self.shot
                {
                    let [iw, ih] = s.image.size;
                    let th = rect.height() - 10.0;
                    let tw = (th * iw as f32 / ih.max(1) as f32).min(64.0);
                    let r = Rect::from_center_size(pos2(rect.right() - 60.0 - tw / 2.0 - 4.0, rect.center().y), vec2(tw, th));
                    p.image(s.texture.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                    p.rect_stroke(r, radius(3), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
                }
                let on = match which {
                    Which::Shot => &mut self.inc.screenshot,
                    Which::Failure => &mut self.inc.failure,
                    Which::Crash => &mut self.inc.crash,
                    Which::System => &mut self.inc.system,
                    Which::Log => &mut self.inc.log,
                };
                switch(ui, rect.right() - 12.0, rect.center().y, ("report-inc", k), on);
            });
            ui.add_space(12.0);
        });

        // Footer: who it goes out as, Preview, Cancel, Send.
        let fy = card.bottom() - foot_h / 2.0;
        let mut x = x1;
        let label = if self.account.is_some() { "Send" } else { "Sign In and Send" };
        let can = !self.draft.is_empty() || self.ctx.failure.is_some() || self.ctx.crash.is_some();
        let (send, r) = button(ui, x, fy, label, Style::Primary, can);
        x = r.left() - 8.0;
        let (cancel, r) = button(ui, x, fy, "Cancel", Style::Plain, true);
        x = r.left() - 8.0;
        let (prev, _) = button(ui, x, fy, if self.preview { "Edit" } else { "Preview" }, Style::Plain, true);
        if prev {
            self.preview = !self.preview;
        }
        if cancel {
            *close = true;
        }
        if (send || (send_key && can)) && can {
            self.send();
        }
        // Left: the account.
        let p = ui.painter();
        match &self.account {
            Some(a) => {
                let c = pos2(x0 + 11.0, fy);
                avatar(p, c, &a.login);
                let name = p.text(
                    pos2(x0 + 28.0, fy - 7.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(a.handle()),
                    theme::body(12.5),
                    Tokens::text_strong(),
                );
                let _ = name;
                let mine = link(ui, pos2(x0 + 28.0, fy + 9.0), "Your Reports", "report-mine-link");
                let out_x = mine.right() + 10.0;
                let sign_out = link(ui, pos2(out_x, fy + 9.0), "Sign Out", "report-sign-out");
                if ui.interact(mine, ui.scope_id().with("report-mine-link"), Sense::click()).clicked() {
                    self.begin_mine();
                }
                if ui.interact(sign_out, ui.scope_id().with("report-sign-out"), Sense::click()).clicked() {
                    self.sign_out();
                }
            }
            None => {
                p.text(
                    pos2(x0, fy - 7.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s("Not signed in"),
                    theme::body(12.5),
                    Tokens::text_strong(),
                );
                let r = link(ui, pos2(x0, fy + 9.0), "Send from the browser instead", "report-browser");
                if ui.interact(r, ui.scope_id().with("report-browser"), Sense::click()).clicked() {
                    let title = self.draft.send_title(&self.ctx);
                    let body = self.body();
                    let (url, fits) = shapecraft_report::draft::browser_link(&title, &body);
                    if !fits {
                        ui.ctx().copy_text(body);
                        self.status = Some("The report is on the clipboard: paste it into GitHub's form.".into());
                    }
                    *out = Some(Out::Open(url));
                }
            }
        }
    }

    fn sign_in_pane(&mut self, ui: &mut Ui, card: Rect, code: Option<&DeviceCode>, out: &mut Option<Out>) {
        let p = ui.painter().clone();
        let cx = card.center().x;
        let mut y = card.top() + 46.0;
        github_mark(&p, pos2(cx, y), 18.0);
        y += 40.0;
        p.text(pos2(cx, y), Align2::CENTER_CENTER, crate::i18n::tr_s("Sign in with GitHub"), theme::strong(17.0), Tokens::text_strong());
        y += 26.0;
        p.text(
            pos2(cx, y),
            Align2::CENTER_CENTER,
            crate::i18n::tr_s("Once, so reports go out under your own name."),
            theme::body(12.5),
            Tokens::text_dim(),
        );
        y += 40.0;
        match code {
            None => {
                spinner(&p, pos2(cx, y + 20.0), ui.input(|i| i.time));
                p.text(
                    pos2(cx, y + 52.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s("Asking GitHub for a code…"),
                    theme::body(12.5),
                    Tokens::text_dim(),
                );
            }
            Some(code) => {
                p.text(
                    pos2(cx, y),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s("Enter this code on GitHub:"),
                    theme::body(13.0),
                    Tokens::text(),
                );
                y += 38.0;
                // The code, one box per character, big enough to read across the room.
                let chars: Vec<char> = code.user_code.chars().collect();
                let cw = 30.0;
                let total = chars.iter().map(|c| if *c == '-' { 18.0 } else { cw + 6.0 }).sum::<f32>() - 6.0;
                let mut x = cx - total / 2.0;
                for ch in &chars {
                    if *ch == '-' {
                        p.text(pos2(x + 6.0, y), Align2::CENTER_CENTER, "–", theme::strong(20.0), Tokens::text_dim());
                        x += 18.0;
                        continue;
                    }
                    let r = Rect::from_min_size(pos2(x, y - 21.0), vec2(cw, 42.0));
                    p.rect_filled(r, radius(7), Tokens::field());
                    p.rect_stroke(r, radius(7), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
                    p.text(r.center(), Align2::CENTER_CENTER, ch, theme::mono(22.0), Tokens::text_strong());
                    x += cw + 6.0;
                }
                y += 50.0;
                let (go, _) = button(ui, cx + 116.0, y, "Copy Code and Open GitHub", Style::Primary, true);
                if go {
                    ui.ctx().copy_text(code.user_code.clone());
                    *out = Some(Out::Open(code.verification_uri.clone()));
                }
                y += 34.0;
                let shown = code.verification_uri.trim_start_matches("https://");
                p.text(
                    pos2(cx, y),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s(format!("or go to {shown} on any phone or computer")),
                    theme::body(12.0),
                    Tokens::text_dim(),
                );
                y += 34.0;
                let waiting = "Waiting for you to approve it…";
                let tw = p.layout_no_wrap(crate::i18n::tr_s(waiting), theme::body(12.5), Tokens::text()).size().x;
                let left = cx - (tw + 22.0) / 2.0;
                spinner(&p, pos2(left + 9.0, y), ui.input(|i| i.time));
                p.text(pos2(left + 22.0, y), Align2::LEFT_CENTER, crate::i18n::tr_s(waiting), theme::body(12.5), Tokens::text());
                y += 30.0;
                let signup = "No GitHub account? Make one free";
                let lw = p.layout_no_wrap(crate::i18n::tr_s(signup), theme::body(12.0), Tokens::icon_blue()).size().x;
                let r = link(ui, pos2(cx - lw / 2.0, y), signup, "report-signup");
                if ui.interact(r, ui.scope_id().with("report-signup-hit"), Sense::click()).clicked() {
                    *out = Some(Out::Open(shapecraft_report::SIGN_UP_PAGE.into()));
                }
            }
        }
        let fine = "ShapeCraft can only open and read issues on its own repository. Nothing else of yours.";
        p.text(pos2(cx, card.bottom() - 70.0), Align2::CENTER_CENTER, crate::i18n::tr_s(fine), theme::body(11.0), Tokens::text_faint());
        let (back, _) = button(ui, cx + 30.0, card.bottom() - 32.0, "Back", Style::Plain, true);
        if back {
            self.stop.store(true, Ordering::Relaxed);
            self.send_after_sign_in = false;
            self.phase = Some(Phase::Compose);
        }
    }

    fn sent_pane(&mut self, ui: &mut Ui, card: Rect, f: &Filed, close: &mut bool, out: &mut Option<Out>) {
        let p = ui.painter().clone();
        let cx = card.center().x;
        let mut y = card.top() + 70.0;
        // A green tick in a ring.
        p.circle_filled(pos2(cx, y), 26.0, theme::fixed(0x2e9e5b));
        p.add(egui::Shape::line(
            vec![pos2(cx - 11.0, y + 1.0), pos2(cx - 3.0, y + 9.0), pos2(cx + 12.0, y - 8.0)],
            Stroke::new(3.5, Color32::WHITE),
        ));
        y += 52.0;
        p.text(
            pos2(cx, y),
            Align2::CENTER_CENTER,
            crate::i18n::tr_s(format!("Sent as issue #{}", f.number)),
            theme::strong(17.0),
            Tokens::text_strong(),
        );
        y += 26.0;
        p.text(
            pos2(cx, y),
            Align2::CENTER_CENTER,
            crate::i18n::tr_s("Thank you. You'll get GitHub's notifications when someone answers."),
            theme::body(12.5),
            Tokens::text_dim(),
        );
        y += 50.0;
        if self.shot_copied {
            let step = Rect::from_center_size(pos2(cx, y + 34.0), vec2((card.width() - 60.0).min(420.0), 92.0));
            p.rect_filled(step, radius(10), Tokens::chip());
            p.rect_stroke(step, radius(10), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
            p.text(
                pos2(step.left() + 16.0, step.top() + 20.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s("One more step: the screenshot"),
                theme::strong(13.0),
                Tokens::text_strong(),
            );
            let keys = if cfg!(target_os = "macos") { "⌘V" } else { "Ctrl+V" };
            p.text(
                pos2(step.left() + 16.0, step.top() + 42.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(format!("It's on your clipboard. Open the issue and press {keys} in the comment box.")),
                theme::body(12.0),
                Tokens::text_dim(),
            );
            let (go, _) = button(ui, step.right() - 14.0, step.bottom() - 20.0, "Open Issue to Paste", Style::Primary, true);
            if go {
                *out = Some(Out::Open(format!("{}#issuecomment-new", f.url)));
            }
            y += 100.0;
        }
        let _ = y;
        let (done, r) = button(
            ui,
            card.right() - 20.0,
            card.bottom() - 32.0,
            "Done",
            if self.shot_copied { Style::Plain } else { Style::Primary },
            true,
        );
        let (view, r) = button(ui, r.left() - 8.0, card.bottom() - 32.0, "View on GitHub", Style::Plain, true);
        let (mine, _) = button(ui, r.left() - 8.0, card.bottom() - 32.0, "Your Reports", Style::Plain, true);
        if view {
            *out = Some(Out::Open(f.url.clone()));
        }
        if mine {
            self.begin_mine();
        }
        if done {
            *close = true;
        }
    }

    fn mine_pane(&mut self, ui: &mut Ui, card: Rect, close: &mut bool, out: &mut Option<Out>) {
        let p = ui.painter().clone();
        let x0 = card.left() + 20.0;
        let head_y = card.top() + 28.0;
        p.text(pos2(x0, head_y), Align2::LEFT_CENTER, crate::i18n::tr_s("Your Reports"), theme::strong(16.0), Tokens::text_strong());
        if let Some(a) = &self.account {
            p.text(
                pos2(card.right() - 20.0, head_y),
                Align2::RIGHT_CENTER,
                crate::i18n::tr_s(a.handle()),
                theme::body(12.5),
                Tokens::text_dim(),
            );
        }
        let body = Rect::from_min_max(pos2(card.left() + 1.0, head_y + 22.0), pos2(card.right() - 1.0, card.bottom() - 64.0));
        p.hline(card.x_range(), body.top(), Stroke::new(1.0, Tokens::divider()));
        p.hline(card.x_range(), body.bottom(), Stroke::new(1.0, Tokens::divider()));
        let now = ui.input(|i| i.time);
        match (&self.account, &self.mine) {
            (None, _) => {
                p.text(
                    body.center() - vec2(0.0, 12.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s("Sign in to see the reports you've sent."),
                    theme::body(13.0),
                    Tokens::text(),
                );
                let (go, _) = button(ui, body.center().x + 40.0, body.center().y + 22.0, "Sign In", Style::Primary, true);
                if go {
                    self.send_after_sign_in = false;
                    self.start_sign_in();
                }
            }
            (Some(_), None) => {
                spinner(&p, body.center(), now);
            }
            (Some(_), Some(Err(e))) => {
                let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(20.0)));
                error_bar(&mut inner, e);
            }
            (Some(_), Some(Ok(list))) if list.is_empty() => {
                p.text(
                    body.center(),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s("Nothing sent yet. Your reports and ideas will show up here."),
                    theme::body(13.0),
                    Tokens::text_dim(),
                );
            }
            (Some(_), Some(Ok(list))) => {
                let list = list.clone();
                let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink2(vec2(12.0, 8.0))));
                inner.set_clip_rect(body);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut inner, |ui| {
                    for m in &list {
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                        let p = ui.painter();
                        if resp.hovered() {
                            p.rect_filled(rect, radius(6), Tokens::row_hover());
                        }
                        let (pill, color) = if m.open { ("Open", theme::fixed(0x2e9e5b)) } else { ("Closed", theme::fixed(0x8250df)) };
                        let pr = Rect::from_min_size(pos2(rect.right() - 62.0, rect.center().y - 10.0), vec2(56.0, 20.0));
                        p.rect_filled(pr, radius(10), color);
                        p.text(pr.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(pill), theme::strong(11.0), Color32::WHITE);
                        let title = theme::elide(p, &m.title, theme::body(13.0), pr.left() - rect.left() - 20.0);
                        p.text(
                            pos2(rect.left() + 8.0, rect.center().y - 8.0),
                            Align2::LEFT_CENTER,
                            crate::i18n::tr_s(title),
                            theme::body(13.0),
                            Tokens::text_strong(),
                        );
                        let replies = match m.comments {
                            0 => "no replies yet".to_string(),
                            1 => "1 reply".to_string(),
                            n => format!("{n} replies"),
                        };
                        let day = m.updated.get(..10).unwrap_or("");
                        p.text(
                            pos2(rect.left() + 8.0, rect.center().y + 9.0),
                            Align2::LEFT_CENTER,
                            crate::i18n::tr_s(format!("#{} · {replies} · {day}", m.number)),
                            theme::body(11.0),
                            Tokens::text_dim(),
                        );
                        if resp.clicked() {
                            *out = Some(Out::Open(m.url.clone()));
                        }
                    }
                });
            }
        }
        let fy = card.bottom() - 32.0;
        let (done, r) = button(ui, card.right() - 20.0, fy, "Done", Style::Primary, true);
        let (new, r) = button(ui, r.left() - 8.0, fy, "New Report…", Style::Plain, true);
        let (refresh, _) = button(ui, r.left() - 8.0, fy, "Refresh", Style::Plain, self.account.is_some());
        if refresh {
            self.load_mine();
        }
        if new {
            self.phase = Some(Phase::Compose);
            self.focus_once = true;
        }
        if done {
            *close = true;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Shot,
    Failure,
    Crash,
    System,
    Log,
}

pub(crate) fn field_label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(theme::strong(11.5)).color(Tokens::text_dim()));
}

/// A text field drawn like the app's own (a dark well with a hairline), `h` tall; Return adds lines unless `single`.
pub(crate) fn text_box(ui: &mut Ui, id: &str, text: &mut String, h: f32, hint: &str, read_only: bool, single: bool) -> egui::Response {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, radius(7), Tokens::field());
    let focused = ui.memory(|m| m.has_focus(ui.make_persistent_id(id)));
    let edge = if focused { Tokens::accent() } else { Tokens::chip_border() };
    p.rect_stroke(rect, radius(7), Stroke::new(if focused { 1.5 } else { 1.0 }, edge), StrokeKind::Inside);
    let inner = rect.shrink2(vec2(9.0, 6.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    if single {
        child.add(
            egui::TextEdit::singleline(text)
                .id(ui.make_persistent_id(id))
                .hint_text(hint)
                .frame(egui::Frame::NONE)
                .font(theme::body(13.0))
                .text_color(Tokens::text_strong())
                .desired_width(inner.width()),
        )
    } else {
        egui::ScrollArea::vertical()
            .id_salt((id, "scroll"))
            .max_height(inner.height())
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .id(ui.make_persistent_id(id))
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .font(if read_only { theme::mono(11.5) } else { theme::body(13.0) })
                        .text_color(Tokens::text_strong())
                        .desired_width(inner.width())
                        .desired_rows(1)
                        .interactive(!read_only),
                )
            })
            .inner
    }
}

/// Rows in one rounded group, a hairline between them (as the Export dialog's switches).
fn group(ui: &mut Ui, n: usize, mut row: impl FnMut(&mut Ui, usize, Rect)) {
    const ROW: f32 = 46.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, ROW * n as f32), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, radius(8), Tokens::chip());
    p.rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    for k in 0..n {
        let r = Rect::from_min_size(pos2(rect.left(), rect.top() + ROW * k as f32), vec2(w, ROW));
        if k > 0 {
            p.hline((r.left() + 12.0)..=(r.right() - 12.0), r.top(), Stroke::new(1.0, Tokens::chip_border()));
        }
        row(ui, k, r);
    }
}

/// An iOS-style switch, right-aligned at `right`.
pub(crate) fn switch(ui: &Ui, right: f32, y: f32, id: (&str, usize), on: &mut bool) {
    let r = Rect::from_min_size(pos2(right - 38.0, y - 11.0), vec2(38.0, 22.0));
    let resp = ui.interact(r.expand(4.0), ui.scope_id().with(id), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(ui.scope_id().with((id, "anim")), *on, 0.12);
    let p = ui.painter();
    let fill = lerp_color(theme::c(0x3a3a3c), Tokens::accent(), t);
    p.rect_filled(r, radius(11), fill);
    let knob = egui::lerp((r.left() + 11.0)..=(r.right() - 11.0), t);
    p.circle_filled(pos2(knob, y), 9.0, theme::fixed(0xf2f2f2));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// A segmented control right-aligned at `right`; returns a newly picked segment.
fn segmented(ui: &Ui, p: &egui::Painter, right: f32, y: f32, id: &str, items: &[&str], current: usize) -> Option<usize> {
    let w = 76.0;
    let r = Rect::from_min_size(pos2(right - w * items.len() as f32, y - 12.0), vec2(w * items.len() as f32, 24.0));
    p.rect_filled(r, radius(7), theme::c(0x2c2c2c));
    p.rect_stroke(r, radius(7), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let mut picked = None;
    for (k, item) in items.iter().enumerate() {
        let s = Rect::from_min_size(pos2(r.left() + w * k as f32, r.top()), vec2(w, 24.0));
        if k == current {
            p.rect_filled(s.shrink(2.0), radius(5), theme::c(0x5c5c5e));
        }
        p.text(s.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*item), theme::body(13.0), Tokens::text_strong());
        if ui.interact(s, ui.scope_id().with((id, k)), Sense::click()).clicked() && k != current {
            picked = Some(k);
        }
    }
    picked
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    Primary,
    Plain,
}

/// A push button right-aligned at `right`, centred on `y`; (clicked, its rect).
pub(crate) fn button(ui: &Ui, right: f32, y: f32, label: &str, style: Style, enabled: bool) -> (bool, Rect) {
    let touch = crate::tablet::touch_ui();
    let h = if touch { 34.0 } else { 26.0 };
    let g = ui.painter().layout_no_wrap(crate::i18n::tr_s(label), theme::body(13.0), Color32::WHITE);
    let r = Rect::from_min_size(pos2(right - g.size().x - 26.0, y - h / 2.0), vec2(g.size().x + 26.0, h));
    let resp = ui.interact(r, ui.scope_id().with(("report-btn", label)), if enabled { Sense::click() } else { Sense::hover() });
    let fill = match (style, enabled, resp.hovered()) {
        (_, false, _) => theme::c(0x3a3a3c),
        (Style::Primary, true, true) => theme::fixed(0x3a62e8),
        (Style::Primary, true, false) => Tokens::accent(),
        (Style::Plain, true, true) => theme::c(0x6a6a6a),
        (Style::Plain, true, false) => theme::c(0x565656),
    };
    let p = ui.painter();
    p.rect_filled(r, radius(6), fill);
    // White on blue, and on the grey button in the dark theme; the light theme's grey is pale, so dark ink there.
    let ink = match style {
        _ if !enabled => Tokens::text_ghost(),
        Style::Plain if theme::is_light() => Tokens::text_strong(),
        _ => Color32::WHITE,
    };
    p.text(r.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(label), theme::body(13.0), ink);
    (enabled && resp.clicked(), r)
}

/// Blue link text starting at `at` (left, centred on y); its rect.
pub(crate) fn link(ui: &Ui, at: Pos2, text: &str, id: &str) -> Rect {
    let p = ui.painter();
    let g = p.layout_no_wrap(crate::i18n::tr_s(text), theme::body(12.0), Tokens::icon_blue());
    let r = Rect::from_min_size(pos2(at.x, at.y - g.size().y / 2.0), g.size());
    let hovered = ui.rect_contains_pointer(r);
    let _ = id;
    p.galley(r.min, g, Tokens::icon_blue());
    if hovered {
        p.hline(r.x_range(), r.bottom(), Stroke::new(1.0, Tokens::icon_blue()));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

pub(crate) fn error_bar(ui: &mut Ui, text: &str) {
    let w = ui.available_width();
    let g = ui.painter().layout(crate::i18n::tr_s(text), theme::body(12.5), Tokens::text_strong(), w - 24.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, g.size().y + 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, radius(7), theme::c(0x58302f));
    p.galley(pos2(rect.left() + 12.0, rect.top() + 8.0), g, Tokens::text_strong());
}

/// A round badge with the login's first letter (no picture is fetched).
fn avatar(p: &egui::Painter, c: Pos2, login: &str) {
    let hue = login.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))) % 360;
    let color = egui::ecolor::Hsva::new(hue as f32 / 360.0, 0.45, 0.62, 1.0);
    p.circle_filled(c, 11.0, Color32::from(color));
    let letter = login.chars().next().map(|c| c.to_ascii_uppercase().to_string()).unwrap_or_default();
    p.text(c, Align2::CENTER_CENTER, crate::i18n::tr_s(letter), theme::strong(12.0), Color32::WHITE);
}

/// GitHub's mark, drawn simply (a ring with a cat's ears would be theirs to draw; this is a plain round badge).
fn github_mark(p: &egui::Painter, c: Pos2, r: f32) {
    p.circle_filled(c, r + 4.0, theme::c(0x3a3a3c));
    p.circle_stroke(c, r - 3.0, Stroke::new(2.4, Tokens::text_strong()));
    p.circle_filled(c, 3.5, Tokens::text_strong());
}

pub(crate) fn spinner(p: &egui::Painter, c: Pos2, now: f64) {
    let n = 12;
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        let phase = ((now * 1.2).fract() as f32 * n as f32 - k as f32).rem_euclid(n as f32) / n as f32;
        let alpha = (255.0 * (1.0 - phase)).clamp(40.0, 255.0) as u8;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 5.0, c + d * 9.0], Stroke::new(2.0, Color32::from_white_alpha(alpha)));
    }
}

fn spinner_pane(ui: &mut Ui, card: Rect, now: f64, title: &str, sub: &str) {
    let p = ui.painter();
    let c = card.center();
    spinner(p, c - vec2(0.0, 30.0), now);
    p.text(c + vec2(0.0, 4.0), Align2::CENTER_CENTER, crate::i18n::tr_s(title), theme::strong(15.0), Tokens::text_strong());
    p.text(c + vec2(0.0, 26.0), Align2::CENTER_CENTER, crate::i18n::tr_s(sub), theme::body(12.5), Tokens::text_dim());
}

/// This system in a few words: "macOS 26.1 (aarch64)".
pub fn system_text() -> String {
    let arch = std::env::consts::ARCH;
    let name = os_name();
    format!("{name} ({arch})")
}

fn os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        let v = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        return format!("macOS {}", v.unwrap_or_default()).trim().to_string();
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let pretty = text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|s| s.trim_matches('"').to_string());
        return pretty.unwrap_or_else(|| "Linux".into());
    }
    #[cfg(windows)]
    {
        let key = windows_registry::LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok();
        let get = |n: &str| key.as_ref().and_then(|k| k.get_string(n).ok());
        let build: u32 = get("CurrentBuildNumber").and_then(|b| b.parse().ok()).unwrap_or(0);
        // Windows 11 still names itself "Windows 10" in ProductName; the build tells them apart.
        let name = if build >= 22000 { "Windows 11".to_string() } else { "Windows 10".to_string() };
        return format!("{name} {} (build {build})", get("DisplayVersion").unwrap_or_default());
    }
    #[allow(unreachable_code)]
    std::env::consts::OS.to_string()
}

/// The mark a running ShapeCraft leaves (its start time); a clean exit removes it.
fn running_mark() -> Option<std::path::PathBuf> {
    Some(dirs::config_dir()?.join("ShapeCraft").join("running"))
}

/// At start: the last run's crash record, when that run never closed cleanly and left one. Marks this run as running.
pub fn take_crash_offer() -> Option<String> {
    let mark = running_mark()?;
    let since: Option<u64> = std::fs::read_to_string(&mark).ok().and_then(|t| t.trim().parse().ok());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Some(dir) = mark.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&mark, now.to_string());
    crash_since(since?)
}

/// A clean exit: the next start has nothing to offer.
pub fn clean_exit() {
    if let Some(m) = running_mark() {
        let _ = std::fs::remove_file(m);
    }
}

/// "ShapeCraft closed unexpectedly": a small card at the foot of the window. Some(true) = Report…, Some(false) = Not Now.
pub fn crash_prompt(ui: &mut Ui) -> Option<bool> {
    let ctx = ui.ctx().clone();
    let full = ctx.content_rect();
    let w = (full.width() - 24.0).min(460.0);
    let card = Rect::from_center_size(pos2(full.center().x, full.bottom() - 90.0), vec2(w, 92.0));
    let id = ui.scope_id().with("crash-prompt");
    let mut answer = None;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(card.min).show(&ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(card.size(), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect.translate(vec2(0.0, 5.0)), radius(12), Color32::from_black_alpha(80));
        p.rect_filled(rect, radius(12), Tokens::panel());
        p.rect_stroke(rect, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
        p.text(
            pos2(rect.left() + 18.0, rect.top() + 24.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("ShapeCraft closed unexpectedly"),
            theme::strong(14.0),
            Tokens::text_strong(),
        );
        p.text(
            pos2(rect.left() + 18.0, rect.top() + 45.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("Tell us what you were doing, and we'll look into it."),
            theme::body(12.5),
            Tokens::text_dim(),
        );
        let (go, r) = button(ui, rect.right() - 16.0, rect.bottom() - 22.0, "Report…", Style::Primary, true);
        let (no, _) = button(ui, r.left() - 8.0, rect.bottom() - 22.0, "Not Now", Style::Plain, true);
        if go {
            answer = Some(true);
        } else if no {
            answer = Some(false);
        }
    });
    answer
}

/// The last crash record in `crash.log`, if it's newer than `since` (unix seconds).
pub fn crash_since(since: u64) -> Option<String> {
    let text = std::fs::read_to_string(crate::crash_log::path()?).ok()?;
    let last = text.rfind("--- unix time ")?;
    let entry = &text[last..];
    let secs: u64 = entry.trim_start_matches("--- unix time ").split(',').next()?.trim().parse().ok()?;
    (secs > since).then(|| entry.chars().take(16_000).collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_failure_opens_a_problem_with_its_details_on() {
        let ctx = egui::Context::default();
        let mut r = Report::default();
        r.account_read = true;
        let f = Failure { command: "fillet".into(), params: "{}".into(), error: "too large".into() };
        r.begin(&ctx, Seed::Failure(f.clone()), Context { version: "1".into(), ..Default::default() }, false);
        assert!(r.open && r.draft.kind == Kind::Problem && r.inc.failure);
        assert_eq!(r.ctx.failure, Some(f));
        assert_eq!(r.draft.send_title(&r.ctx), "Fillet: too large");
        // Without a screenshot the body says nothing about one.
        assert!(!r.body().contains("Screenshot"));
    }

    #[test]
    fn an_unsent_draft_survives_closing_and_switching_kind() {
        let ctx = egui::Context::default();
        let mut r = Report::default();
        r.account_read = true;
        r.begin(&ctx, Seed::Problem, Context::default(), false);
        r.draft.text = "half written".into();
        r.close();
        r.begin(&ctx, Seed::Idea, Context::default(), false);
        assert_eq!((r.draft.kind, r.draft.text.as_str()), (Kind::Idea, "half written"));
    }

    #[test]
    fn the_last_crash_is_found_by_its_time() {
        let entry = "--- unix time 1700000000, ShapeCraft 0.10.4 on macos aarch64, thread main\nboom\n";
        let text = format!("--- unix time 1600000000, old\nolder\n{entry}");
        let last = text.rfind("--- unix time ").unwrap();
        let secs: u64 = text[last..].trim_start_matches("--- unix time ").split(',').next().unwrap().trim().parse().unwrap();
        assert_eq!(secs, 1_700_000_000);
    }
}
