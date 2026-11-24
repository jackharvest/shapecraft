//! Plugins in the app: Preferences ▸ Plugins (installed, Browse, Add with its permissions, each plugin's settings),
//! the sheet a plugin opens (Generative Render), and the control channel's `plugins.*` methods.
//!
//! The plugins themselves run in `shapecraft-plugin-host` (sandboxed WebAssembly). This draws what they describe:
//! every control is ours, so a plugin looks and behaves like the rest of the app.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use egui::{Align2, Color32, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, pos2, vec2};
use serde_json::{Map, Value, json};
use shapecraft_plugin_host::index::{self, Index};
use shapecraft_plugin_host::panel::{Choice, Tone};
use shapecraft_plugin_host::store::KeyFile;
use shapecraft_plugin_host::{Capture, Control, Installed, Library, Loaded, Manifest, Package, Panel, Spend, Ticket, results};

use crate::report::{Style, button, error_bar, link, spinner, switch, text_box};
use crate::theme::{self, Tokens, radius};

/// The official plugin the Generative Render button offers when it isn't added yet.
pub const GENERATIVE: &str = "generative-render";

thread_local! {
    static STATE: RefCell<Plugins> = RefCell::new(Plugins::new());
}

fn with<R>(f: impl FnOnce(&mut Plugins) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// What a panel shows and what the person set in it.
#[derive(Default)]
struct PanelState {
    panel: Option<Panel>,
    values: Map<String, Value>,
    /// A newer description is on its way.
    refresh: Option<Ticket>,
    /// The values changed since the last description was asked for.
    dirty: bool,
    run: Option<(String, Ticket)>,
    message: Option<(Tone, String)>,
    /// Secrets being typed (not yet saved).
    drafts: HashMap<String, String>,
}

/// The sheet a plugin's panel opens in (style "sheet"), or a plain card.
struct Sheet {
    plugin: String,
    panel: String,
    capture: Option<Capture>,
    /// The view as it was when the sheet opened.
    snapshot: Option<TextureHandle>,
    /// The newest result since then (shown in place of the snapshot).
    shown: Option<results::Entry>,
    /// Writing the description (the prompt bar open); folds away once a result is on show.
    editing: bool,
    /// History instead of the picture.
    history: bool,
    /// A Delete waiting for its second press.
    pending_delete: Option<PathBuf>,
    /// Out of the way while Preferences is open over it.
    hidden: bool,
    /// Where it was dragged to (by its header), from the middle of the window.
    offset: egui::Vec2,
}

/// Adding a plugin: what it is and what it asks for, waiting for the person's OK.
struct Confirm {
    manifest: Manifest,
    source: Source,
}

/// Taking the view to a free chat: the view on the clipboard, the site open, the words to paste after it.
struct Handoff {
    plugin: String,
    site: String,
    url: String,
    text: String,
    /// The clipboard and the site are done once, on the first frame.
    started: bool,
    /// What was last copied ("picture" / "words"), for the card's line.
    copied: Option<&'static str>,
}

/// An assistant (control channel, MCP) asked to press a button that costs money: it waits for the person's Allow.
struct Approval {
    request: u64,
    plugin: String,
    plugin_name: String,
    panel: String,
    action: String,
    values: Value,
    capture: Option<Capture>,
    spend: Spend,
    /// When it was asked for (app clock; negative until the first frame shows it).
    asked: f64,
    state: Asked,
}

enum Asked {
    Waiting,
    Denied(String),
    /// Allowed and started at this time (ms since 1970, to pick out the pictures it made). It runs as the panel's
    /// own press, so the sheet shows its progress, Cancel, result or error as if the person had pressed Generate.
    Running {
        since_ms: u64,
    },
    Done(Result<Value, String>, Vec<String>),
}

/// How long a request waits for the person before it lapses (nothing is spent).
const APPROVAL_WAIT_S: f64 = 600.0;

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The card that follows "Add Plugin": what's happening, then what to do next.
enum AddCard {
    Working { name: String, icon: String, size: u64, since: f64 },
    Done { id: String, name: String, icon: String },
    Failed { name: String, icon: String, error: String, retry: Option<Box<index::Entry>> },
}

/// What a plugin card asks the rest of the app for.
pub(crate) enum Request {
    /// Preferences ▸ Plugins, with the plugin just added open.
    OpenPlugins,
}

enum Source {
    Listed(Box<index::Entry>),
    File(PathBuf),
}

pub struct Plugins {
    lib: Option<Library>,
    installed: Vec<Installed>,
    loaded: HashMap<String, Loaded>,
    start_errors: HashMap<String, String>,
    panels: HashMap<(String, String), PanelState>,
    results: HashMap<String, Vec<results::Entry>>,
    textures: HashMap<PathBuf, TextureHandle>,
    browse: Option<Receiver<Result<Index, String>>>,
    index: Option<Result<Index, String>>,
    confirm: Option<Confirm>,
    adding: Option<(String, Receiver<Result<Package, String>>)>,
    add_card: Option<AddCard>,
    /// A finished add, held until the card has shown "working" long enough to read.
    add_done: Option<Result<Installed, String>>,
    requests: Vec<Request>,
    handoff: Option<Handoff>,
    add_pick: crate::dialogs::Pick,
    /// The listing being added, for Try Again.
    retry: Option<Box<index::Entry>>,
    notice: Option<(Tone, String)>,
    sheet: Option<Sheet>,
    /// The Generative Render button pressed without the plugin: offer to add it.
    offer: bool,
    /// Preferences ▸ Plugins: Browse shown instead of Installed, and the plugin whose settings are open.
    browsing: bool,
    selected: Option<String>,
    file_pick: crate::dialogs::Pick,
    save_pick: Option<(PathBuf, crate::dialogs::Pick)>,
    urls: Vec<String>,
    /// Paid presses an assistant asked for, oldest first (answered ones stay until asked about, at most 32).
    approvals: Vec<Approval>,
    next_request: u64,
}

fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

impl Plugins {
    fn new() -> Self {
        let lib = Library::open();
        let installed = lib.as_ref().map(Library::list).unwrap_or_default();
        Self {
            lib,
            installed,
            loaded: HashMap::new(),
            start_errors: HashMap::new(),
            panels: HashMap::new(),
            results: HashMap::new(),
            textures: HashMap::new(),
            browse: None,
            index: None,
            confirm: None,
            adding: None,
            add_card: None,
            add_done: None,
            requests: Vec::new(),
            handoff: None,
            add_pick: Default::default(),
            retry: None,
            notice: None,
            sheet: None,
            offer: false,
            browsing: false,
            selected: None,
            file_pick: Default::default(),
            save_pick: None,
            urls: Vec::new(),
            approvals: Vec::new(),
            next_request: 1,
        }
    }

    fn reload_list(&mut self) {
        self.installed = self.lib.as_ref().map(Library::list).unwrap_or_default();
        let ids: Vec<String> = self.installed.iter().filter(|i| i.enabled).map(|i| i.manifest.id.clone()).collect();
        self.loaded.retain(|id, _| ids.contains(id));
    }

    fn installed(&self, id: &str) -> Option<&Installed> {
        self.installed.iter().find(|i| i.manifest.id == id)
    }

    /// The running plugin, started on first use.
    fn ensure(&mut self, id: &str) -> Option<&Loaded> {
        if !self.loaded.contains_key(id) {
            let inst = self.installed(id)?.clone();
            if !inst.enabled {
                return None;
            }
            let lib = self.lib.as_ref()?;
            match lib.start(&inst) {
                Ok(l) => {
                    self.start_errors.remove(id);
                    self.loaded.insert(id.to_string(), l);
                }
                Err(e) => {
                    log::warn!("plugin {id} didn't start: {e}");
                    self.start_errors.insert(id.to_string(), e.to_string());
                    return None;
                }
            }
        }
        self.loaded.get(id)
    }

    fn state(&mut self, plugin: &str, panel: &str) -> &mut PanelState {
        let dir = self.installed(plugin).map(|i| i.dir.clone());
        self.panels.entry((plugin.to_string(), panel.to_string())).or_insert_with(|| {
            let values = dir
                .and_then(|d| KeyFile::settings(&d).get(&format!("panel.{panel}")))
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            PanelState { values, dirty: true, ..Default::default() }
        })
    }

    fn results_of(&mut self, plugin: &str) -> &mut Vec<results::Entry> {
        let dir = self.installed(plugin).map(|i| i.dir.clone());
        self.results.entry(plugin.to_string()).or_insert_with(|| dir.map(|d| results::list(&d)).unwrap_or_default())
    }

    /// Everything that happens without drawing: answers coming back, new results, downloads.
    fn poll(&mut self, ctx: &egui::Context) {
        let mut busy = false;
        // Panels: refresh when the values changed; take new descriptions and finished actions.
        let keys: Vec<(String, String)> = self.panels.keys().cloned().collect();
        for (plugin, panel) in keys {
            if self.ensure(&plugin).is_none() {
                continue;
            }
            let Some(loaded) = self.loaded.get(&plugin) else { continue };
            let new_results = loaded.activity.take_results();
            let st_key = (plugin.clone(), panel.clone());
            let refresh_ticket = self
                .panels
                .get(&st_key)
                .filter(|st| st.dirty && st.refresh.is_none())
                .map(|st| loaded.panel(&panel, &Value::Object(st.values.clone())));
            if !new_results.is_empty() {
                let list = self.results_of(&plugin);
                for r in new_results.iter().rev() {
                    list.insert(0, r.clone());
                }
                if let Some(s) = self.sheet.as_mut().filter(|s| s.plugin == plugin)
                    && let Some(newest) = new_results.last()
                {
                    s.shown = Some(newest.clone());
                    s.editing = false;
                    s.history = false;
                }
            }
            let Some(st) = self.panels.get_mut(&st_key) else { continue };
            if let Some(t) = refresh_ticket {
                st.refresh = Some(t);
                st.dirty = false;
            }
            if let Some(answer) = st.refresh.as_ref().and_then(Ticket::poll) {
                st.refresh = None;
                match answer.map_err(|e| e.to_string()).and_then(|t| Panel::parse(&t)) {
                    Ok(p) => {
                        for c in &p.controls {
                            if let (Some(id), Some(v)) = (c.id(), c.default_value()) {
                                st.values.entry(id.to_string()).or_insert(v);
                            }
                        }
                        st.panel = Some(p);
                    }
                    Err(e) => st.message = Some((Tone::Error, e)),
                }
            }
            if let Some((action, t)) = &st.run
                && let Some(answer) = t.poll()
            {
                let action = action.clone();
                st.run = None;
                st.dirty = true;
                let answer_json = answer.as_ref().ok().and_then(|t| serde_json::from_str::<Value>(t).ok());
                st.message = Some(match answer {
                    Ok(text) => {
                        let msg = serde_json::from_str::<Value>(&text)
                            .ok()
                            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
                            .unwrap_or_default();
                        (Tone::Good, msg)
                    }
                    Err(shapecraft_plugin_host::Error::Cancelled) => (Tone::Dim, "Cancelled.".into()),
                    Err(e) if action == "secret-saved" && e.to_string().starts_with("no action") => {
                        (Tone::Good, "Saved on this computer.".into())
                    }
                    Err(e) => (Tone::Error, e.to_string()),
                });
                // An answer may fill in values ("set": the address Find ComfyUI found) or send the person to a web page
                // with the view on the clipboard ("handoff").
                if let Some(j) = answer_json {
                    if let Some(set) = j.get("set").and_then(Value::as_object) {
                        for (k, v) in set {
                            st.values.insert(k.clone(), v.clone());
                        }
                        let text = Value::Object(st.values.clone()).to_string();
                        if let Some(d) = self.installed(&plugin).map(|i| i.dir.clone()) {
                            let _ = KeyFile::settings(&d).set(&format!("panel.{panel}"), Some(&text));
                        }
                    }
                    if let Some(h) = j.get("handoff") {
                        let field = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                        if field("url").starts_with("https://") {
                            self.handoff = Some(Handoff {
                                plugin: plugin.clone(),
                                site: field("site"),
                                url: field("url"),
                                text: field("text"),
                                started: false,
                                copied: None,
                            });
                        }
                    }
                }
                // Newly saved sign-ins or keys change the other panels too.
                for ((p, _), other) in self.panels.iter_mut() {
                    if *p == plugin {
                        other.dirty = true;
                    }
                }
                break;
            }
            busy |= st.run.is_some() || st.refresh.is_some();
        }
        busy |= self.poll_approvals(ctx);
        if let Some(rx) = &self.browse {
            busy = true;
            if let Ok(r) = rx.try_recv() {
                self.index = Some(r);
                self.browse = None;
            }
        }
        if let Some((id, rx)) = &self.adding {
            busy = true;
            if let Ok(r) = rx.try_recv() {
                let id = id.clone();
                self.adding = None;
                let _ = id;
                self.add_done = Some(r.and_then(|pkg| {
                    self.lib.as_ref().ok_or("no plugins folder".to_string())?.install(&pkg, false).map_err(|e| e.to_string())
                }));
            }
        }
        // The card shows "working" for at least a moment, so a quick add still reads as something that happened.
        let now = ctx.input(|i| i.time);
        if let Some(AddCard::Working { since, .. }) = &self.add_card {
            busy = true;
            if *since >= 0.0
                && now - *since >= 1.2
                && let Some(done) = self.add_done.take()
            {
                self.finish_add(done);
            }
        } else if let Some(done) = self.add_done.take() {
            self.finish_add(done);
        }
        if let Some(Some(path)) = self.add_pick.taken()
            && let Some(plugin) = self.handoff.as_ref().map(|h| h.plugin.clone()).or_else(|| self.sheet.as_ref().map(|s| s.plugin.clone()))
        {
            let site = self.handoff.as_ref().map(|h| h.site.clone()).unwrap_or_else(|| "Added".into());
            let prompt = self
                .panels
                .get(&(plugin.clone(), "main".to_string()))
                .and_then(|st| st.values.get("prompt"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let dir = self.installed(&plugin).map(|i| i.dir.clone());
            let r =
                std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| shapecraft_plugin_host::images::decode(&b)).and_then(|pic| {
                    results::save(
                        &dir.ok_or("plugin not added")?,
                        &pic,
                        if prompt.is_empty() { "(no description)" } else { &prompt },
                        &format!("{site} · Free chat"),
                    )
                });
            match r {
                Ok(e) => {
                    self.results_of(&plugin).insert(0, e.clone());
                    if let Some(s) = self.sheet.as_mut() {
                        s.shown = Some(e);
                        s.editing = false;
                    }
                    self.handoff = None;
                }
                Err(e) => self.notice = Some((Tone::Error, format!("That picture couldn't be added: {e}"))),
            }
        }
        if let Some(Some(path)) = self.file_pick.taken() {
            self.confirm_file(path);
        }
        if let Some((src, pick)) = &mut self.save_pick
            && let Some(answer) = pick.taken()
        {
            if let Some(dest) = answer {
                let dest = if dest.extension().is_none() { dest.with_extension("png") } else { dest };
                if let Err(e) = std::fs::copy(&*src, &dest) {
                    self.notice = Some((Tone::Error, format!("The picture couldn't be saved: {e}")));
                }
            }
            self.save_pick = None;
        }
        self.urls.extend(shapecraft_plugin_host::runtime::take_urls());
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    fn finish_add(&mut self, done: Result<Installed, String>) {
        let (name, icon, retry) = match self.add_card.take() {
            Some(AddCard::Working { name, icon, .. }) => (name, icon, self.retry.take()),
            _ => (String::new(), String::new(), None),
        };
        match done {
            Ok(i) => {
                let (id, n, ic) = (i.manifest.id.clone(), i.manifest.name.clone(), i.manifest.icon.clone());
                self.added(i);
                self.notice = None;
                self.add_card = Some(AddCard::Done { id, name: n, icon: ic });
            }
            Err(e) => self.add_card = Some(AddCard::Failed { name, icon, error: e, retry }),
        }
    }

    fn added(&mut self, i: Installed) {
        let name = i.manifest.name.clone();
        let id = i.manifest.id.clone();
        self.loaded.remove(&id);
        self.panels.retain(|(p, _), _| *p != id);
        self.reload_list();
        self.notice = Some((Tone::Good, format!("{name} is added.")));
        self.selected = Some(id);
        self.browsing = false;
    }

    fn fetch_index(&mut self) {
        let (tx, rx) = mpsc::channel();
        let v = app_version().to_string();
        std::thread::spawn(move || {
            let _ = tx.send(index::fetch(&v));
        });
        self.browse = Some(rx);
        self.index = None;
    }

    fn start_add(&mut self, entry: &index::Entry) {
        self.confirm = Some(Confirm { manifest: entry.manifest.clone(), source: Source::Listed(Box::new(entry.clone())) });
    }

    fn confirm_file(&mut self, path: PathBuf) {
        match shapecraft_plugin_host::library::read_package(&path) {
            Ok(pkg) => self.confirm = Some(Confirm { manifest: pkg.manifest, source: Source::File(path) }),
            Err(e) => self.notice = Some((Tone::Error, format!("That file isn't a plugin this ShapeCraft can use: {e}"))),
        }
    }

    fn accept(&mut self, now: f64) {
        let Some(c) = self.confirm.take() else { return };
        let (name, icon) = (c.manifest.name.clone(), c.manifest.icon.clone());
        match c.source {
            Source::Listed(entry) => {
                let size = entry.size;
                self.retry = Some(entry.clone());
                let (tx, rx) = mpsc::channel();
                let v = app_version().to_string();
                std::thread::spawn(move || {
                    let _ = tx.send(index::download(&v, entry.as_ref()));
                });
                self.adding = Some((name.clone(), rx));
                self.add_card = Some(AddCard::Working { name, icon, size, since: now });
            }
            Source::File(path) => {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                self.retry = None;
                let r = shapecraft_plugin_host::library::read_package(&path)
                    .and_then(|pkg| {
                        self.lib.as_ref().ok_or(shapecraft_plugin_host::Error::Io("no plugins folder".into()))?.install(&pkg, true)
                    })
                    .map_err(|e| e.to_string());
                self.add_card = Some(AddCard::Working { name, icon, size, since: now });
                self.add_done = Some(r);
            }
        }
    }

    fn set_value(&mut self, plugin: &str, panel: &str, id: &str, v: Value) {
        let dir = self.installed(plugin).map(|i| i.dir.clone());
        let st = self.state(plugin, panel);
        if st.values.get(id) == Some(&v) {
            return;
        }
        st.values.insert(id.to_string(), v);
        st.dirty = true;
        let text = Value::Object(st.values.clone()).to_string();
        if let Some(d) = dir {
            let _ = KeyFile::settings(&d).set(&format!("panel.{panel}"), Some(&text));
        }
    }

    fn press(&mut self, plugin: &str, panel: &str, action: &str, capture: Option<Capture>) {
        let values = Value::Object(self.state(plugin, panel).values.clone());
        let Some(loaded) = self.ensure(plugin) else { return };
        let t = loaded.run(action, &values, capture);
        let st = self.state(plugin, panel);
        st.message = None;
        st.run = Some((action.to_string(), t));
    }

    /// Allowed presses: start them; finished ones: keep the answer and the pictures for the assistant.
    fn poll_approvals(&mut self, ctx: &egui::Context) -> bool {
        let now = ctx.input(|i| i.time);
        let mut busy = false;
        let mut finished = Vec::new();
        for (i, a) in self.approvals.iter_mut().enumerate() {
            match &a.state {
                Asked::Waiting => {
                    busy = true;
                    if a.asked < 0.0 {
                        a.asked = now;
                        // The person may be in another window: ask the system to point at ShapeCraft.
                        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                    } else if now - a.asked > APPROVAL_WAIT_S {
                        a.state = Asked::Denied("Nobody answered in ShapeCraft, so nothing was made or spent.".into());
                    }
                }
                Asked::Running { .. } => {
                    busy = true;
                    let st = self.panels.get(&(a.plugin.clone(), a.panel.clone()));
                    if st.is_none_or(|st| st.run.is_none()) {
                        let answer = match st.and_then(|st| st.message.clone()) {
                            Some((Tone::Error, e)) => Err(e),
                            Some((Tone::Dim, m)) if m.starts_with("Cancelled") => Err("Cancelled in ShapeCraft.".to_string()),
                            Some((_, m)) => Ok(json!({ "message": m })),
                            None => Ok(Value::Null),
                        };
                        finished.push((i, answer));
                    }
                }
                _ => {}
            }
        }
        for (i, answer) in finished {
            let (plugin, since) = match &self.approvals[i].state {
                Asked::Running { since_ms, .. } => (self.approvals[i].plugin.clone(), *since_ms),
                _ => continue,
            };
            // The panel loop files new pictures under the plugin; these are the ones made since the press.
            if let Some(l) = self.loaded.get(&plugin) {
                let new = l.activity.take_results();
                let list = self.results_of(&plugin);
                for r in new.iter().rev() {
                    list.insert(0, r.clone());
                }
            }
            let paths: Vec<String> =
                self.results_of(&plugin).iter().filter(|e| e.time >= since).map(|e| e.path.to_string_lossy().into_owned()).collect();
            self.approvals[i].state = Asked::Done(answer, paths);
        }
        busy
    }

    /// The person's answer to the oldest waiting request.
    fn answer_approval(&mut self, allow: bool) {
        let Some(i) = self.approvals.iter().position(|a| matches!(a.state, Asked::Waiting)) else { return };
        if !allow {
            self.approvals[i].state = Asked::Denied("The person chose Don't Allow in ShapeCraft. Nothing was made or spent.".into());
            return;
        }
        let (plugin, panel, action, values, capture) = {
            let a = &mut self.approvals[i];
            (a.plugin.clone(), a.panel.clone(), a.action.clone(), a.values.clone(), a.capture.take())
        };
        let since_ms = now_ms();
        let ticket = match self.ensure(&plugin) {
            Some(l) if l.busy() => Err("The plugin is still busy with something else; ask again in a moment."),
            Some(l) => Ok(l.run(&action, &values, capture)),
            None => Err("The plugin isn't added or is switched off."),
        };
        self.approvals[i].state = match ticket {
            Ok(t) => {
                let st = self.state(&plugin, &panel);
                st.message = None;
                st.run = Some((action, t));
                Asked::Running { since_ms }
            }
            Err(e) => Asked::Denied(e.into()),
        };
    }

    fn texture(&mut self, ctx: &egui::Context, path: &PathBuf) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(path) {
            return Some(t.clone());
        }
        let pic = shapecraft_plugin_host::images::decode(&std::fs::read(path).ok()?).ok()?;
        let img = egui::ColorImage::from_rgba_unmultiplied([pic.width as usize, pic.height as usize], &pic.pixels);
        let t = ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR);
        if self.textures.len() > 64 {
            self.textures.clear();
        }
        self.textures.insert(path.clone(), t.clone());
        Some(t)
    }
}

// ——— What the rest of the app calls ———

/// Each frame: answers, overlays (the sheet, Add confirmations), and web pages plugins asked to open.
pub(crate) fn frame(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    with(|p| {
        p.poll(&ctx);
        // A waiting Allow card sits over everything, so Esc (Don't Allow) is its, not the sheet's under it.
        if p.approvals.iter().any(|a| matches!(a.state, Asked::Waiting)) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            p.answer_approval(false);
        }
        if p.sheet.is_some() {
            sheet(p, ui);
        }
        if p.offer {
            offer_card(p, ui);
        }
        if p.confirm.is_some() {
            confirm_card(p, ui);
        }
        if p.add_card.is_some() {
            add_card(p, ui);
        }
        if p.handoff.is_some() {
            handoff_card(p, ui);
        }
        if p.approvals.iter().any(|a| matches!(a.state, Asked::Waiting)) {
            approval_card(p, ui);
        }
        for url in std::mem::take(&mut p.urls) {
            // Offscreen runs (snap, tests) set SHAPECRAFT_NO_BROWSER so nothing opens on the person's screen.
            if std::env::var_os("SHAPECRAFT_NO_BROWSER").is_some() {
                log::info!("would open {url}");
            } else {
                crate::landing::open_link(&url);
            }
        }
    });
}

/// The plugin that answers Visualization's Generative Render button, when one is added and on.
pub(crate) fn visualization_plugin() -> Option<(String, String)> {
    with(|p| {
        p.installed.iter().filter(|i| i.enabled).find_map(|i| {
            let c = i.manifest.commands.iter().find(|c| c.place == "visualization" && !c.panel.is_empty())?;
            Some((i.manifest.id.clone(), c.panel.clone()))
        })
    })
}

/// Opens a plugin's panel as a sheet over the window, with the view captured for its buttons.
pub(crate) fn open_sheet(plugin: &str, panel: &str, capture: Option<Capture>) {
    with(|p| {
        if p.ensure(plugin).is_none() {
            let why = p.start_errors.get(plugin).cloned().unwrap_or_else(|| "it's turned off".into());
            p.notice = Some((Tone::Error, format!("The plugin couldn't start: {why}")));
            p.offer = false;
            return;
        }
        p.state(plugin, panel).dirty = true;
        p.offer = false;
        p.sheet = Some(Sheet {
            plugin: plugin.into(),
            panel: panel.into(),
            capture,
            snapshot: None,
            shown: None,
            editing: true,
            history: false,
            pending_delete: None,
            hidden: false,
            offset: egui::Vec2::ZERO,
        });
    });
}

/// The Generative Render button without the plugin: a card offering to add it.
pub(crate) fn offer_generative() {
    with(|p| {
        p.offer = true;
        if p.index.is_none() && p.browse.is_none() {
            p.fetch_index();
        }
    });
}

/// Whether a plugin sheet or card has the keyboard (the workspace leaves keys alone then).
pub(crate) fn takes_keys() -> bool {
    with(|p| {
        p.sheet.as_ref().is_some_and(|s| !s.hidden)
            || p.offer
            || p.confirm.is_some()
            || p.add_card.is_some()
            || p.handoff.is_some()
            || p.approvals.iter().any(|a| matches!(a.state, Asked::Waiting))
    })
}

/// Brings back a sheet put aside for Preferences (its Connections button), once Preferences closes.
pub(crate) fn show_sheet_again() {
    with(|p| {
        let Some(key) = p.sheet.as_mut().filter(|s| s.hidden).map(|s| {
            s.hidden = false;
            (s.plugin.clone(), s.panel.clone())
        }) else {
            return;
        };
        // New connections change the model list.
        if let Some(st) = p.panels.get_mut(&key) {
            st.dirty = true;
        }
    });
}

/// Whether a sheet is waiting for Preferences to close.
pub(crate) fn sheet_waiting() -> bool {
    with(|p| p.sheet.as_ref().is_some_and(|s| s.hidden))
}

/// What the plugin cards asked the rest of the app for since the last frame.
pub(crate) fn take_requests() -> Vec<Request> {
    with(|p| std::mem::take(&mut p.requests))
}

// ——— Preferences ▸ Plugins ———

pub(crate) fn prefs_tab(ui: &mut Ui, r: Rect) {
    with(|p| prefs(p, ui, r));
}

fn prefs(p: &mut Plugins, ui: &mut Ui, r: Rect) {
    let painter = ui.painter().clone();
    let x0 = r.left() + 24.0;
    let x1 = r.right() - 24.0;
    let mut y = r.top() + 92.0;
    let intro = crate::i18n::tr_s("Add features you want. Plugins run sealed off and can only do what they ask for when you add them.");
    let mut job = egui::text::LayoutJob::simple(intro, theme::body(11.0), Tokens::text_dim(), r.width() - 48.0);
    job.halign = egui::Align::Center;
    let g = painter.layout_job(job);
    painter.galley(pos2(r.center().x, y - g.size().y / 2.0), g, Tokens::text_dim());
    y += 26.0;
    // Installed | Browse
    let seg = Rect::from_center_size(pos2(r.center().x, y), vec2(200.0, 24.0));
    let picked = segmented_at(ui, seg, "plugins-seg", &["Installed", "Browse"], usize::from(p.browsing));
    if let Some(k) = picked {
        p.browsing = k == 1;
        if p.browsing && p.index.is_none() && p.browse.is_none() {
            p.fetch_index();
        }
    }
    y += 24.0;
    if let Some((tone, text)) = p.notice.clone() {
        let g = painter.layout(crate::i18n::tr_s(text), theme::body(11.5), tone_color(tone), x1 - x0);
        painter.galley(pos2(x0, y - 6.0), g.clone(), tone_color(tone));
        y += g.size().y + 6.0;
    }
    let body = Rect::from_min_max(pos2(r.left() + 1.0, y), pos2(r.right() - 1.0, r.bottom() - 60.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink2(vec2(23.0, 0.0))));
    child.set_clip_rect(body);
    egui::ScrollArea::vertical().id_salt("plugins-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        if p.browsing {
            browse_list(p, ui);
        } else {
            installed_list(p, ui);
        }
    });
    // Footer: adding from a file is for people writing plugins.
    let fy = r.bottom() - 26.0;
    if !crate::tablet::on_tablet() {
        let rect = link(ui, pos2(r.left() + 16.0, fy), "Add from File…", "plugins-file");
        if ui.interact(rect, ui.scope_id().with("plugins-file-btn"), Sense::click()).clicked() && !p.file_pick.is_open() {
            p.file_pick.start(|| crate::dialogs::FileDialog::new().add_filter("ShapeCraft plugin", &["scplugin"]).pick_file());
        }
    }
}

fn installed_list(p: &mut Plugins, ui: &mut Ui) {
    if p.installed.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No plugins yet").font(theme::body(14.0)).color(Tokens::text_strong()));
            ui.label(egui::RichText::new("Open Browse to see what you can add.").font(theme::body(11.5)).color(Tokens::text_dim()));
        });
        return;
    }
    let list = p.installed.clone();
    for inst in &list {
        ui.push_id(("installed", &inst.manifest.id), |ui| installed_row(p, ui, inst));
    }
}

fn installed_row(p: &mut Plugins, ui: &mut Ui, inst: &Installed) {
    {
        let id = inst.manifest.id.clone();
        let open = p.selected.as_deref() == Some(id.as_str());
        let w = ui.available_width();
        let desc = p.start_errors.get(&id).cloned().unwrap_or_else(|| inst.manifest.description.clone());
        let g = ui.painter().layout(crate::i18n::tr_s(desc), theme::body(10.5), Tokens::text_dim(), w - 150.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, (g.size().y + 40.0).max(58.0)), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(rect, radius(8), if open { Tokens::row_selected() } else { Tokens::chip() });
        painter.rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        plugin_icon(&painter, Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(30.0, 30.0)), &inst.manifest.icon);
        let tx = rect.left() + 52.0;
        let mut title = format!("{}  {}", inst.manifest.name, inst.manifest.version);
        if inst.sideloaded {
            title.push_str("  · added from a file");
        }
        painter.text(
            pos2(tx, rect.top() + 19.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(title),
            theme::strong(13.0),
            Tokens::text_strong(),
        );
        painter.galley(pos2(tx, rect.top() + 30.0), g, Tokens::text_dim());
        let mut on = inst.enabled;
        switch(ui, rect.right() - 12.0, rect.center().y, ("plugin-on", hash(&id)), &mut on);
        if on != inst.enabled {
            if let Some(lib) = &p.lib {
                let _ = lib.set_enabled(&id, on);
            }
            p.reload_list();
        }
        if resp.clicked() {
            p.selected = if open { None } else { Some(id.clone()) };
        }
        if open {
            ui.push_id(("plugin-settings", &id), |ui| settings_of(p, ui, inst));
        }
    }
}

fn settings_of(p: &mut Plugins, ui: &mut Ui, inst: &Installed) {
    let id = inst.manifest.id.clone();
    ui.add_space(2.0);
    if inst.enabled {
        p.state(&id, "settings");
        column(p, ui, &id, "settings");
    } else {
        note(ui, "Turned off. Turn it on to change its settings.", Tone::Dim);
    }
    ui.add_space(4.0);
    note(ui, &format!("It may: {}.", inst.manifest.permission_lines().join("; ").to_lowercase()), Tone::Dim);
    let row = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover()).0;
    let (remove, _) = button(ui, row.right(), row.center().y, "Remove", Style::Plain, true);
    if !inst.manifest.repository.is_empty() {
        let r = link(ui, pos2(row.left(), row.center().y), "Source and help", "plugin-repo");
        if ui.interact(r, ui.scope_id().with("plugin-repo-btn"), Sense::click()).clicked() {
            p.urls.push(inst.manifest.repository.clone());
        }
    }
    if remove {
        if let Some(lib) = &p.lib {
            if let Some(l) = p.loaded.remove(&id) {
                l.cancel();
            }
            match lib.remove(&id) {
                Ok(()) => p.notice = Some((Tone::Dim, format!("{} is removed, with its settings, keys and pictures.", inst.manifest.name))),
                Err(e) => p.notice = Some((Tone::Error, e.to_string())),
            }
        }
        p.panels.retain(|(pid, _), _| *pid != id);
        p.results.remove(&id);
        p.selected = None;
        p.reload_list();
    }
    ui.add_space(8.0);
}

fn browse_list(p: &mut Plugins, ui: &mut Ui) {
    let now = ui.ctx().input(|i| i.time);
    match &p.index {
        None => {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 80.0), Sense::hover());
            spinner(ui.painter(), r.center() - vec2(0.0, 10.0), now);
            ui.painter().text(
                r.center() + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                crate::i18n::tr_s("Getting the list…"),
                theme::body(12.0),
                Tokens::text_dim(),
            );
        }
        Some(Err(e)) => {
            let e = e.clone();
            note(ui, &format!("The plugin list couldn't be loaded: {e}"), Tone::Error);
            let row = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover()).0;
            if button(ui, row.right(), row.center().y, "Try Again", Style::Plain, true).0 {
                p.fetch_index();
            }
        }
        Some(Ok(idx)) => {
            let entries = idx.plugins.clone();
            for tier in ["official", "community"] {
                let these: Vec<&index::Entry> = entries.iter().filter(|e| e.tier == tier).collect();
                if these.is_empty() {
                    continue;
                }
                ui.label(
                    egui::RichText::new(if tier == "official" { "From ShapeCraft" } else { "From the Community (reviewed)" })
                        .font(theme::strong(11.5))
                        .color(Tokens::text_dim()),
                );
                for e in these {
                    ui.push_id(("browse", &e.manifest.id), |ui| browse_row(p, ui, e));
                }
            }
            if entries.is_empty() {
                note(ui, "Nothing is listed for this version yet.", Tone::Dim);
            }
            note(ui, "Anyone can make one: see docs/plugins.md in ShapeCraft's repository.", Tone::Dim);
        }
    }
}

fn browse_row(p: &mut Plugins, ui: &mut Ui, e: &index::Entry) {
    let w = ui.available_width();
    let lines = e.manifest.permission_lines();
    let g = ui.painter().layout(crate::i18n::tr_s(&e.manifest.description), theme::body(10.5), Tokens::text_dim(), w - 160.0);
    let gh = g.size().y;
    let h = (46.0 + gh + 14.0 * lines.len() as f32).max(58.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, radius(8), Tokens::chip());
    painter.rect_stroke(rect, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    plugin_icon(&painter, Rect::from_center_size(pos2(rect.left() + 26.0, rect.top() + 26.0), vec2(30.0, 30.0)), &e.manifest.icon);
    let tx = rect.left() + 52.0;
    painter.text(
        pos2(tx, rect.top() + 18.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr_s(format!("{}  {}", e.manifest.name, e.manifest.version)),
        theme::strong(13.0),
        Tokens::text_strong(),
    );
    painter.galley(pos2(tx, rect.top() + 29.0), g, Tokens::text_dim());
    for (k, l) in lines.iter().enumerate() {
        painter.text(
            pos2(tx, rect.top() + 38.0 + gh + 14.0 * k as f32),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("• {l}")),
            theme::body(10.0),
            Tokens::text_faint(),
        );
    }
    let have = p.installed(&e.manifest.id).map(|i| i.manifest.version.clone());
    let newer = have.as_deref().is_some_and(|v| {
        shapecraft_plugin_host::manifest::parse_version(v) < shapecraft_plugin_host::manifest::parse_version(&e.manifest.version)
    });
    let busy = p.adding.as_ref().is_some_and(|(n, _)| *n == e.manifest.name)
        || matches!(&p.add_card, Some(AddCard::Working { name, .. }) if *name == e.manifest.name);
    let (label, enabled) = match (&have, newer, busy) {
        (_, _, true) => ("Adding…", false),
        (Some(_), true, _) => ("Update", true),
        (Some(_), false, _) => ("Added", false),
        (None, ..) => ("Add", true),
    };
    if button(ui, rect.right() - 12.0, rect.top() + 22.0, label, Style::Primary, enabled && p.adding.is_none()).0 {
        p.start_add(e);
    }
}

fn plugin_icon(p: &egui::Painter, r: Rect, which: &str) {
    p.rect_filled(r, radius(8), if which == "sparkle" { Tokens::accent() } else { theme::c(0x4a4a4e) });
    let icon = match which {
        "sparkle" => crate::icons::Icon::CameraSparkle,
        "image" => crate::icons::Icon::Screenshot,
        _ => crate::icons::Icon::Insert,
    };
    crate::icons::paint(p, r.shrink(6.0), icon, Color32::WHITE);
}

fn hash(s: &str) -> usize {
    s.bytes().fold(7usize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(b)))
}

fn tone_color(t: Tone) -> Color32 {
    match t {
        Tone::Dim => Tokens::text_dim(),
        Tone::Normal => Tokens::text_strong(),
        Tone::Good => theme::fixed(0x5fc46a),
        Tone::Warning => theme::fixed(0xe8a93a),
        Tone::Error => Tokens::danger(),
    }
}

fn note(ui: &mut Ui, text: &str, tone: Tone) {
    if tone == Tone::Error {
        error_bar(ui, text);
        return;
    }
    let w = ui.available_width();
    let g = ui.painter().layout(crate::i18n::tr_s(text), theme::body(11.5), tone_color(tone), w);
    let (r, _) = ui.allocate_exact_size(vec2(w, g.size().y), Sense::hover());
    ui.painter().galley(r.min, g, tone_color(tone));
}

/// A segmented control filling `r`; returns a newly picked segment.
fn segmented_at(ui: &Ui, r: Rect, id: &str, items: &[&str], current: usize) -> Option<usize> {
    let p = ui.painter();
    p.rect_filled(r, radius(7), theme::c(0x2c2c2c));
    p.rect_stroke(r, radius(7), Stroke::new(1.0, theme::c(0x3a3a3c)), StrokeKind::Inside);
    let w = r.width() / items.len().max(1) as f32;
    let mut picked = None;
    for (k, item) in items.iter().enumerate() {
        let s = Rect::from_min_size(pos2(r.left() + w * k as f32, r.top()), vec2(w, r.height()));
        if k == current {
            p.rect_filled(s.shrink(2.0), radius(5), theme::c(0x5c5c5e));
        }
        p.text(s.center(), Align2::CENTER_CENTER, crate::i18n::tr_s(*item), theme::body(12.0), Tokens::text_strong());
        if ui.interact(s, ui.scope_id().with((id, k)), Sense::click()).clicked() && k != current {
            picked = Some(k);
        }
    }
    picked
}

// ——— Drawing a panel's controls ———

/// What the person did in a panel this frame.
enum Act {
    Set(String, Value),
    Press(String, bool),
    SaveSecret(String, String),
    ForgetSecret(String),
    Open(String),
    Cancel,
}

/// A panel as a column (settings in Preferences, plain cards).
fn column(p: &mut Plugins, ui: &mut Ui, plugin: &str, panel_id: &str) {
    let Some(dir) = p.installed(plugin).map(|i| i.dir.clone()) else { return };
    let now = ui.ctx().input(|i| i.time);
    let (panel, values, running, message, progress) = {
        let busy_progress = p.loaded.get(plugin).map(|l| l.activity.progress());
        let st = p.state(plugin, panel_id);
        (st.panel.clone(), st.values.clone(), st.run.as_ref().map(|r| r.0.clone()), st.message.clone(), busy_progress)
    };
    let Some(panel) = panel else {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        spinner(ui.painter(), r.center(), now);
        return;
    };
    let mut acts = Vec::new();
    for (k, c) in panel.controls.iter().enumerate() {
        ui.push_id(("ctl", k), |ui| control(ui, c, &values, &dir, running.as_deref(), &mut acts, p.drafts_mut(plugin, panel_id)));
    }
    if let Some(a) = &running {
        let (_, text) = progress.unwrap_or_default();
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        spinner(ui.painter(), pos2(r.left() + 10.0, r.center().y), now);
        let label = if text.is_empty() { format!("Working ({a})…") } else { text };
        ui.painter().text(
            pos2(r.left() + 26.0, r.center().y),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(label),
            theme::body(11.5),
            Tokens::text_dim(),
        );
        if button(ui, r.right(), r.center().y, "Cancel", Style::Plain, true).0 {
            acts.push(Act::Cancel);
        }
    }
    if let Some((tone, text)) = message.filter(|(_, t)| !t.is_empty()) {
        note(ui, &text, tone);
    }
    apply(p, plugin, panel_id, acts, None, &dir);
}

impl Plugins {
    fn drafts_mut(&mut self, plugin: &str, panel: &str) -> &mut HashMap<String, String> {
        &mut self.state(plugin, panel).drafts
    }
}

fn apply(p: &mut Plugins, plugin: &str, panel_id: &str, acts: Vec<Act>, capture: Option<Capture>, dir: &std::path::Path) {
    for a in acts {
        match a {
            Act::Set(id, v) => p.set_value(plugin, panel_id, &id, v),
            Act::Press(action, wants_capture) => {
                let cap = if wants_capture { capture.clone() } else { None };
                p.press(plugin, panel_id, &action, cap);
            }
            Act::SaveSecret(id, value) => {
                let r = KeyFile::secrets(dir).set(&id, Some(value.trim()));
                // Plugins may answer "secret-saved" (fetch what the new key unlocks); its answer becomes the message.
                if r.is_ok()
                    && let Some(l) = p.ensure(plugin)
                {
                    let t = l.run("secret-saved", &json!({ "id": id }), None);
                    p.state(plugin, panel_id).run = Some(("secret-saved".into(), t));
                }
                let st = p.state(plugin, panel_id);
                st.drafts.remove(&id);
                st.dirty = true;
                st.message = Some(match r {
                    Ok(()) => (Tone::Good, "Saved on this computer.".into()),
                    Err(e) => (Tone::Error, e),
                });
                for ((pp, _), s) in p.panels.iter_mut() {
                    if pp == plugin {
                        s.dirty = true;
                    }
                }
            }
            Act::ForgetSecret(id) => {
                let _ = KeyFile::secrets(dir).set(&id, None);
                for ((pp, _), s) in p.panels.iter_mut() {
                    if pp == plugin {
                        s.dirty = true;
                    }
                }
            }
            Act::Open(url) => p.urls.push(url),
            Act::Cancel => {
                if let Some(l) = p.loaded.get(plugin) {
                    l.cancel();
                }
            }
        }
    }
}

fn label_of<'a>(label: &'a str, id: &'a str) -> &'a str {
    if label.is_empty() { id } else { label }
}

fn small_label(ui: &mut Ui, text: &str) {
    if !text.is_empty() {
        ui.label(egui::RichText::new(text).font(theme::strong(11.0)).color(Tokens::text_dim()));
    }
}

/// One control in a column.
fn control(
    ui: &mut Ui,
    c: &Control,
    values: &Map<String, Value>,
    dir: &std::path::Path,
    running: Option<&str>,
    acts: &mut Vec<Act>,
    drafts: &mut HashMap<String, String>,
) {
    let w = ui.available_width();
    match c {
        Control::Heading { text } => {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(text).font(theme::strong(13.0)).color(Tokens::text_strong()));
        }
        Control::Note { text, tone } => note(ui, text, *tone),
        Control::Separator {} => {
            let (r, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, Tokens::divider()));
        }
        Control::Link { label, url } => {
            let (r, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
            let lr = link(ui, pos2(r.left(), r.center().y), label, "plugin-link");
            if ui.interact(lr, ui.scope_id().with("link"), Sense::click()).clicked() && url.starts_with("https://") {
                acts.push(Act::Open(url.clone()));
            }
        }
        Control::Text { id, label, placeholder, multiline, .. } => {
            small_label(ui, label);
            let mut text = values.get(id).and_then(Value::as_str).unwrap_or("").to_string();
            let before = text.clone();
            text_box(ui, &format!("pl-text-{id}"), &mut text, if *multiline { 76.0 } else { 28.0 }, placeholder, false, !*multiline);
            if text != before {
                acts.push(Act::Set(id.clone(), Value::from(text)));
            }
        }
        Control::Secret { id, label, placeholder } => {
            small_label(ui, label_of(label, id));
            let saved = KeyFile::secrets(dir).get(id).is_some();
            let row = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover()).0;
            if saved {
                ui.painter().text(
                    pos2(row.left(), row.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s("✓ Saved on this computer"),
                    theme::body(12.0),
                    tone_color(Tone::Good),
                );
                if button(ui, row.right(), row.center().y, "Remove", Style::Plain, true).0 {
                    acts.push(Act::ForgetSecret(id.clone()));
                }
            } else {
                let draft = drafts.entry(id.clone()).or_default();
                let field = Rect::from_min_max(row.min, pos2(row.right() - 70.0, row.max.y));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
                child.add(
                    egui::TextEdit::singleline(draft)
                        .id(ui.make_persistent_id(("secret", id)))
                        .password(true)
                        .hint_text(placeholder.as_str())
                        .font(theme::body(12.5))
                        .desired_width(field.width()),
                );
                let ok = !draft.trim().is_empty();
                if button(ui, row.right(), row.center().y, "Save", Style::Primary, ok).0 {
                    acts.push(Act::SaveSecret(id.clone(), draft.clone()));
                }
            }
        }
        Control::Toggle { id, label, .. } => {
            let mut on = values.get(id).and_then(Value::as_bool).unwrap_or(false);
            let (r, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
            ui.painter().text(
                pos2(r.left(), r.center().y),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(label),
                theme::body(12.5),
                Tokens::text_strong(),
            );
            let before = on;
            switch(ui, r.right(), r.center().y, ("pl-toggle", hash(id)), &mut on);
            if on != before {
                acts.push(Act::Set(id.clone(), Value::from(on)));
            }
        }
        Control::Segmented { id, label, options, .. } => {
            small_label(ui, label);
            let cur = values.get(id).and_then(Value::as_str).unwrap_or("");
            let k = options.iter().position(|o| o == cur).unwrap_or(0);
            let (r, _) = ui.allocate_exact_size(vec2(w.min(90.0 * options.len() as f32), 24.0), Sense::hover());
            let refs: Vec<&str> = options.iter().map(String::as_str).collect();
            if let Some(n) = segmented_at(ui, r, &format!("pl-seg-{id}"), &refs, k) {
                acts.push(Act::Set(id.clone(), Value::from(options[n].clone())));
            }
        }
        Control::Menu { id, label, options, empty, .. } => {
            small_label(ui, label);
            let cur = values.get(id).and_then(Value::as_str).unwrap_or("").to_string();
            if let Some(v) = menu(ui, id, options, &cur, empty, w) {
                acts.push(Act::Set(id.clone(), Value::from(v)));
            }
        }
        Control::Button { label, action, primary, capture, enabled, .. } => {
            let row = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover()).0;
            let g = ui.painter().layout_no_wrap(crate::i18n::tr_s(label), theme::body(13.0), Color32::WHITE);
            let right = row.left() + g.size().x + 26.0;
            let ok = enabled.unwrap_or(true) && running.is_none();
            if button(ui, right, row.center().y, label, if *primary { Style::Primary } else { Style::Plain }, ok).0 {
                acts.push(Act::Press(action.clone(), *capture));
            }
        }
        Control::Results {} | Control::Stage {} => {}
    }
}

/// A pop-up menu of grouped choices; the newly picked value.
fn menu(ui: &mut Ui, id: &str, options: &[Choice], cur: &str, empty: &str, w: f32) -> Option<String> {
    // A value the list doesn't have (yet: an older pick, a list not refreshed) still shows by its name.
    let shown = options
        .iter()
        .find(|c| c.value == cur)
        .map(|c| c.shown().to_string())
        .or_else(|| (!cur.is_empty()).then(|| cur.split_once(':').map_or(cur, |(_, m)| m).to_string()));
    let text = shown.unwrap_or_else(|| if options.is_empty() { empty.to_string() } else { "Choose…".to_string() });
    let mut picked = None;
    egui::ComboBox::from_id_salt(("pl-menu", id))
        .width(w.min(380.0))
        .selected_text(egui::RichText::new(text).font(theme::body(12.5)).color(Tokens::text_strong()))
        .show_ui(ui, |ui| {
            let mut last_group = None;
            for c in options {
                if last_group != Some(&c.group) && !c.group.is_empty() {
                    ui.label(egui::RichText::new(&c.group).font(theme::strong(11.0)).color(Tokens::text_dim()));
                }
                last_group = Some(&c.group);
                let label = if c.detail.is_empty() { c.shown().to_string() } else { format!("{}    {}", c.shown(), c.detail) };
                if ui.selectable_label(c.value == cur, label).clicked() {
                    picked = Some(c.value.clone());
                }
            }
        });
    picked
}

// ——— The sheet (Generative Render) ———

/// A card in the middle of the window that the person can drag by its top strip, like the sheet; where it was left
/// is remembered while the app runs (kept on screen).
fn dim_and_card(ui: &mut Ui, id: egui::Id, w: f32, h: f32, f: impl FnOnce(&mut Ui, Rect)) {
    let key = id.with("offset");
    let offset = ui.ctx().data(|d| d.get_temp::<egui::Vec2>(key)).unwrap_or_default();
    let mut dragged = egui::Vec2::ZERO;
    dim_and_card_at(ui, id, w, h, offset, |ui, card| {
        // Registered first, so the card's own buttons (drawn after) stay on top of it.
        let grip = Rect::from_min_max(card.min, pos2(card.right(), card.top() + 56.0));
        let drag = ui.interact(grip, id.with("grip"), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(if drag.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        dragged = drag.drag_delta();
        f(ui, card);
    });
    if dragged != egui::Vec2::ZERO {
        // Kept within what dim_and_card_at allows, so a long drag past the edge doesn't pile up.
        let full = card_area(ui.ctx());
        let size = vec2(w.min(full.width() - 24.0), h.min(full.height() - 24.0));
        let room = ((full.size() - size) / 2.0 - vec2(4.0, 4.0)).max(egui::Vec2::ZERO);
        ui.ctx().data_mut(|d| d.insert_temp(key, (offset + dragged).clamp(-room, room)));
    }
}

/// Where cards and the sheet may sit: the safe area, below a sideways iPhone's status row and clear of a reserved
/// status corner (the iPhone Duo's clock), so nothing in a card's header sits under the clock.
fn card_area(ctx: &egui::Context) -> Rect {
    let mut r = ctx.content_rect();
    r.min.y += crate::tablet::status_row();
    if let Some(c) = crate::tablet::reserved_corner()
        && c.top() <= r.top() + 1.0
        && c.bottom() > r.top()
    {
        r.min.y = c.bottom();
    }
    r
}

/// A card moved by `offset` from the middle (kept on screen).
fn dim_and_card_at(ui: &mut Ui, id: egui::Id, w: f32, h: f32, offset: egui::Vec2, f: impl FnOnce(&mut Ui, Rect)) {
    let ctx = ui.ctx().clone();
    let screen = ctx.content_rect();
    let full = card_area(&ctx);
    let card = Rect::from_center_size(full.center(), vec2(w.min(full.width() - 24.0), h.min(full.height() - 24.0)));
    let room = ((full.size() - card.size()) / 2.0 - vec2(4.0, 4.0)).max(egui::Vec2::ZERO);
    let card = card.translate(offset.clamp(-room, room));
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(screen.min).show(&ctx, |ui| {
        let _ = ui.allocate_rect(screen, Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(screen, 0.0, Color32::from_black_alpha(110));
        p.rect_filled(card.translate(vec2(0.0, 8.0)), radius(14), Color32::from_black_alpha(90));
        p.rect_filled(card, radius(12), Tokens::panel());
        p.rect_stroke(card, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
        let _ = ui.interact(card, id.with("card"), Sense::click());
        f(ui, card);
    });
    ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
}

/// A small rounded label over a picture: what made it.
fn pill(p: &egui::Painter, left_top: egui::Pos2, text: &str, size: f32, accent: bool) -> f32 {
    let g = p.layout_no_wrap(crate::i18n::tr_s(text), theme::body(size), Color32::WHITE);
    let r = Rect::from_min_size(left_top, vec2(g.size().x + size * 1.3, g.size().y + size * 0.55));
    p.rect_filled(
        r,
        radius(((r.height() / 2.0) as u8).max(4)),
        if accent { Tokens::accent().gamma_multiply(0.92) } else { Color32::from_black_alpha(165) },
    );
    p.galley(pos2(r.left() + size * 0.65, r.top() + size * 0.27), g, Color32::WHITE);
    r.width()
}

/// The facts a plugin gave a result (`details`, "A · B · C").
fn tags(e: &results::Entry) -> Vec<String> {
    let mut v: Vec<String> = e.details.split(" · ").map(str::trim).filter(|t| !t.is_empty()).map(pretty_tag).collect();
    // Pictures from before the service came first: put the mode last.
    if v.len() > 1 && ["Surroundings", "Whole Picture", "Selected Faces"].contains(&v[0].as_str()) {
        let mode = v.remove(0);
        v.push(mode);
    }
    v
}

/// "gemini-3.1-flash-image" → "Gemini 3.1 Flash Image"; "model kept exact" → "Model kept exact".
fn pretty_tag(t: &str) -> String {
    let id_like = !t.contains(' ') && t.contains('-') && t.chars().all(|c| !c.is_uppercase());
    if id_like {
        return t
            .split('-')
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
            .replace("Gpt", "GPT");
    }
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// "Today 14:05", "Nov 18, 09:12".
fn when(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(secs);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (h, m) = (rem / 3600, rem % 3600 / 60);
    if days == now.div_euclid(86_400) {
        return format!("Today {h:02}:{m:02}");
    }
    // Civil date from days since 1970 (UTC).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {d}, {h:02}:{m:02}", names[(month - 1).clamp(0, 11) as usize])
}

/// A square icon button with a hover fill; true when pressed.
fn icon_button(ui: &Ui, r: Rect, id: &str, tip: &str, glyph: impl FnOnce(&egui::Painter, egui::Pos2, Color32)) -> bool {
    let resp = ui.interact(r, ui.scope_id().with(id), Sense::click()).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(r, radius(6), Tokens::button_hover());
    }
    glyph(ui.painter(), r.center(), Tokens::text_strong());
    resp.clicked()
}

fn gear_glyph(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    for k in 0..8 {
        let a = k as f32 / 8.0 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 5.0, c + d * 8.0], Stroke::new(2.4, col));
    }
    p.circle_stroke(c, 5.2, Stroke::new(1.8, col));
}

fn clock_glyph(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    p.circle_stroke(c, 7.5, Stroke::new(1.6, col));
    p.line_segment([c, c + vec2(0.0, -4.5)], Stroke::new(1.6, col));
    p.line_segment([c, c + vec2(3.5, 1.5)], Stroke::new(1.6, col));
}

fn back_glyph(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    p.line_segment([c + vec2(3.0, -6.0), c + vec2(-3.0, 0.0)], Stroke::new(2.0, col));
    p.line_segment([c + vec2(-3.0, 0.0), c + vec2(3.0, 6.0)], Stroke::new(2.0, col));
}

/// What happened in the sheet this frame beyond panel values.
#[derive(Default)]
struct SheetOut {
    close: bool,
    show: Option<results::Entry>,
    save: Option<PathBuf>,
    copy: Option<PathBuf>,
    delete: Option<results::Entry>,
    reuse: Option<String>,
    edit: bool,
    history: Option<bool>,
    connect: bool,
}

fn sheet(p: &mut Plugins, ui: &mut Ui) {
    let Some((plugin, panel_id)) = p.sheet.as_ref().filter(|s| !s.hidden).map(|s| (s.plugin.clone(), s.panel.clone())) else { return };
    let Some(dir) = p.installed(&plugin).map(|i| i.dir.clone()) else {
        p.sheet = None;
        return;
    };
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let (panel, values, running, message) = {
        let st = p.state(&plugin, &panel_id);
        (st.panel.clone(), st.values.clone(), st.run.as_ref().map(|r| r.0.clone()), st.message.clone())
    };
    let progress = p.loaded.get(&plugin).map(|l| l.activity.progress()).unwrap_or_default();
    if let Some(s) = p.sheet.as_mut()
        && s.snapshot.is_none()
        && let Some(c) = &s.capture
    {
        let img = egui::ColorImage::from_rgba_unmultiplied([c.view.width as usize, c.view.height as usize], &c.view.pixels);
        s.snapshot = Some(ctx.load_texture("plugin-sheet-snapshot", img, egui::TextureOptions::LINEAR));
    }
    let (snapshot, capture, shown_entry, editing, history, pending_delete) = match p.sheet.as_ref() {
        Some(s) => (s.snapshot.clone(), s.capture.clone(), s.shown.clone(), s.editing, s.history, s.pending_delete.clone()),
        None => return,
    };
    let shown_tex = shown_entry.as_ref().and_then(|e| p.texture(&ctx, &e.path));
    let all: Vec<results::Entry> = p.results_of(&plugin).clone();
    let full = card_area(&ctx);
    let touch = crate::tablet::touch_ui();
    let max_thumbs = if history { 60 } else { 10 };
    let thumbs: Vec<Option<TextureHandle>> = all.iter().take(max_thumbs).map(|e| p.texture(&ctx, &e.path)).collect();

    // The panel's parts: the switch at the top, the prompt, the blue button, and rows of the rest (a separator
    // starts a row; a heading names the tiles after it).
    let controls = panel.as_ref().map(|p| p.controls.clone()).unwrap_or_default();
    let prompt = controls.iter().find(|c| matches!(c, Control::Text { .. })).cloned();
    let go = controls.iter().find(|c| matches!(c, Control::Button { primary: true, .. })).cloned();
    let top = controls.iter().find(|c| matches!(c, Control::Segmented { place, .. } if place == "header")).cloned();
    let mut rows: Vec<Vec<Control>> = vec![Vec::new()];
    let mut notes: Vec<(String, Tone)> = Vec::new();
    for c in &controls {
        match c {
            Control::Separator {} => rows.push(Vec::new()),
            // A note straight after a row's heading is what that zone says (nothing to pick there now).
            Control::Note { text, tone } => match rows.last_mut() {
                Some(r) if matches!(r.last(), Some(Control::Heading { .. })) => r.push(c.clone()),
                _ => notes.push((text.clone(), *tone)),
            },
            Control::Stage {} | Control::Results {} | Control::Link { .. } => {}
            Control::Text { .. } if prompt.as_ref() == Some(c) => {}
            Control::Button { .. } if go.as_ref() == Some(c) => {}
            Control::Segmented { .. } if top.as_ref() == Some(c) => {}
            other => {
                if let Some(r) = rows.last_mut() {
                    r.push(other.clone());
                }
            }
        }
    }
    rows.retain(|r| r.iter().any(|c| !matches!(c, Control::Heading { .. })));
    let connected = !matches!(&go, Some(Control::Button { enabled: Some(false), .. }));
    let prompt_text = match &prompt {
        Some(Control::Text { id, placeholder, .. }) => {
            (values.get(id).and_then(Value::as_str).unwrap_or("").to_string(), placeholder.clone())
        }
        _ => (String::new(), String::new()),
    };
    // A result on show folds the words into one line with Try Again (only for a button that makes a picture from the
    // view; a button that hands the view elsewhere, like a free chat, stays as it is).
    let makes = matches!(&go, Some(Control::Button { capture: true, .. }));
    let folded = makes && !editing && running.is_none() && shown_entry.is_some();

    // Sizes. A computer or tablet: one column with the picture on top, the prompt under it, then the options as
    // tiles (the body scrolls when the window is short). A phone held sideways: the picture on the left and the
    // rest scrolling on the right.
    let mp = ui.painter().clone();
    let msg = message.clone().filter(|(t, s)| !s.is_empty() && *t != Tone::Good);
    let short = full.height() < 560.0;
    let header_h = if short { 46.0 } else { 54.0 };
    let footer_h = if short { 46.0 } else { 54.0 };
    let max_h = full.height() - 24.0;
    let body_avail = max_h - header_h - footer_h;
    let gap = 16.0;
    let switch_w = top.as_ref().map_or(0.0, |t| switch_width(&mp, t));
    let landscape = full.width() >= 1.3 * full.height() && full.width() >= 600.0;
    let measure = |split: bool| {
        let w = if split { (full.width() - 24.0).min(1100.0) } else { (full.width() - 24.0).min(if touch { 850.0 } else { 800.0 }) };
        let pad = if w < 560.0 { 12.0 } else { 20.0 };
        let switch_in_header = top.is_some() && w - 2.0 * pad >= switch_w + 2.0 * 190.0;
        let (stage_w, col_w) = if split {
            let s = ((w - 3.0 * pad) * 0.52).floor();
            (s, w - 3.0 * pad - s)
        } else {
            (w - 2.0 * pad, w - 2.0 * pad)
        };
        let switch_h = if top.is_some() && !switch_in_header { switch_line_h(touch) + gap } else { 0.0 };
        let composer_h = composer_height(&mp, col_w, &prompt_text.0, &prompt_text.1, folded, connected, touch);
        let (placed, options_h) = lay_out(&mp, &rows, col_w, touch);
        let notes_h: f32 =
            notes.iter().map(|(t, _)| mp.layout(crate::i18n::tr_s(t), theme::body(12.0), Color32::WHITE, col_w).size().y + 8.0).sum();
        let msg_h = msg
            .as_ref()
            .map_or(0.0, |(_, s)| mp.layout(crate::i18n::tr_s(s), theme::body(12.0), Color32::WHITE, col_w - 20.0).size().y + 22.0);
        let strip_h = if all.is_empty() { 0.0 } else { 50.0 + gap };
        let rest = switch_h + composer_h + gap + options_h + if notes_h > 0.0 { 6.0 + notes_h } else { 0.0 } + msg_h + gap + strip_h;
        let ideal = (stage_w * 0.56).round();
        let (stage_h, h) = if split {
            (body_avail - 14.0, max_h)
        } else {
            let s = ideal.min(body_avail - 14.0 - rest - gap).max(ideal.min(if short { 160.0 } else { 220.0 }));
            (s, header_h + 14.0 + (s + gap + rest).min(body_avail - 14.0) + footer_h)
        };
        (w, pad, switch_in_header, stage_w, col_w, composer_h, placed, options_h, notes_h, rest, stage_h, h, ideal)
    };
    // Sideways and too short for the picture to keep most of its height on top: picture left, the rest right. Decided
    // by the window alone (a typical body under the picture), so the layout doesn't jump as the words grow or fold.
    let single = measure(false);
    let typical = if touch { 400.0 } else { 360.0 };
    let split = landscape && (short || body_avail - 14.0 - typical - gap < 0.6 * single.12);
    let (w, pad, switch_in_header, stage_w, col_w, composer_h, placed, options_h, notes_h, rest, stage_h, h, _) =
        if split { measure(true) } else { single };
    let content_h = if split { rest } else { stage_h + gap + rest };
    if running.is_some() {
        ctx.request_repaint();
    }
    let mut acts = Vec::new();
    let mut out =
        SheetOut { close: ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) && running.is_none(), ..Default::default() };
    let offset = p.sheet.as_ref().map_or(egui::Vec2::ZERO, |s| s.offset);
    let mut dragged = egui::Vec2::ZERO;
    dim_and_card_at(ui, egui::Id::NULL.with("plugin-sheet"), w, h, offset, |ui, card| {
        let painter = ui.painter().clone();
        // The header moves the sheet (the buttons in it are registered after, so they stay pressable).
        let grip = Rect::from_min_max(card.min, pos2(card.right(), card.top() + header_h));
        let drag = ui.interact(grip, ui.scope_id().with("sheet-grip"), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(if drag.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        dragged = drag.drag_delta();
        let x0 = card.left() + pad;
        let x1 = card.right() - pad;
        // Header: the title (or History with a way back), the switch in the middle, History and settings.
        let hy = card.top() + header_h / 2.0 + 1.0;
        if history {
            if icon_button(ui, Rect::from_center_size(pos2(x0 + 12.0, hy), vec2(28.0, 28.0)), "sheet-back", "Back", back_glyph) {
                out.history = Some(false);
            }
            painter.text(
                pos2(x0 + 34.0, hy),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s("History"),
                theme::strong(15.0),
                Tokens::text_strong(),
            );
            if w >= 480.0 {
                painter.text(
                    pos2(x0 + 104.0, hy),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(format!("{} pictures, kept on this computer", all.len())),
                    theme::body(11.5),
                    Tokens::text_dim(),
                );
            }
        } else {
            let title = panel.as_ref().map(|p| p.title.clone()).unwrap_or_default();
            painter.text(pos2(x0, hy), Align2::LEFT_CENTER, crate::i18n::tr_s(title), theme::strong(15.0), Tokens::text_strong());
            if switch_in_header && let Some(t) = &top {
                let r = Rect::from_center_size(pos2(card.center().x, hy), vec2(switch_w, switch_line_h(touch)));
                header_switch(ui, r, t, &values, &mut acts);
            }
            if !all.is_empty()
                && icon_button(ui, Rect::from_center_size(pos2(x1 - 50.0, hy), vec2(30.0, 30.0)), "sheet-history", "History", clock_glyph)
            {
                out.history = Some(true);
            }
        }
        if icon_button(
            ui,
            Rect::from_center_size(pos2(x1 - 14.0, hy), vec2(30.0, 30.0)),
            "sheet-gear",
            "Connections and settings",
            gear_glyph,
        ) {
            out.connect = true;
        }
        painter.hline(card.x_range(), card.top() + header_h, Stroke::new(1.0, Tokens::divider()));
        let body_top = card.top() + header_h + 14.0;
        let body_bottom = card.bottom() - footer_h;
        if history {
            let area = Rect::from_min_max(pos2(x0, body_top), pos2(x1, body_bottom - 6.0));
            history_grid(ui, area, &all, &thumbs, pending_delete.as_ref(), touch, &mut out);
        } else {
            let stage_pic = shown_tex.as_ref().or(snapshot.as_ref());
            // Sideways phone: the picture holds the left; everything else scrolls on its right.
            let col_x = if split {
                let stage = Rect::from_min_size(pos2(x0, body_top), vec2(stage_w, stage_h));
                draw_stage(&painter, stage, stage_pic, shown_entry.as_ref(), running.is_some(), &progress.1, now);
                stage.right() + pad
            } else {
                x0
            };
            let view = Rect::from_min_max(pos2(col_x, body_top), pos2(card.right() - 4.0, body_bottom));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(view));
            child.set_clip_rect(Rect::from_min_max(pos2(view.left() - 4.0, view.top() - 6.0), view.max));
            egui::ScrollArea::vertical().id_salt("sheet-body").auto_shrink([false, false]).show(&mut child, |ui| {
                let (block, _) = ui.allocate_exact_size(vec2(col_w, content_h), Sense::hover());
                let painter = ui.painter().clone();
                let (bx0, bx1) = (block.left(), block.right());
                let mut y = block.top();
                if !switch_in_header && let Some(t) = &top {
                    let r = Rect::from_min_size(pos2(bx0, y), vec2(col_w, switch_line_h(touch)));
                    header_switch(ui, r, t, &values, &mut acts);
                    y = r.bottom() + gap;
                }
                if !split {
                    let stage = Rect::from_min_size(pos2(bx0, y), vec2(stage_w, stage_h));
                    draw_stage(&painter, stage, stage_pic, shown_entry.as_ref(), running.is_some(), &progress.1, now);
                    y = stage.bottom() + gap;
                }
                // The words, under the picture (never over it).
                let bar = Rect::from_min_size(pos2(bx0, y), vec2(col_w, composer_h));
                composer(
                    ui,
                    bar,
                    &prompt,
                    &go,
                    &values,
                    running.is_some(),
                    folded,
                    connected,
                    shown_entry.as_ref(),
                    touch,
                    &mut acts,
                    &mut out,
                );
                y = bar.bottom() + gap;
                // How and who: tiles with a picture of what each does.
                draw_options(ui, &placed, pos2(bx0, y), &values, &mut acts);
                y += options_h;
                if notes_h > 0.0 {
                    y += 6.0;
                }
                for (text, tone) in &notes {
                    let g = painter.layout(crate::i18n::tr_s(text), theme::body(12.0), tone_color(*tone), col_w);
                    let gh = g.size().y;
                    painter.galley(pos2(bx0, y), g, tone_color(*tone));
                    y += gh + 8.0;
                }
                if let Some((tone, text)) = &msg {
                    let g = painter.layout(crate::i18n::tr_s(text), theme::body(12.0), Tokens::text_strong(), col_w - 20.0);
                    let r = Rect::from_min_size(pos2(bx0, y), vec2(col_w, g.size().y + 14.0));
                    painter.rect_filled(r, radius(7), if *tone == Tone::Error { theme::c(0x58302f) } else { theme::c(0x2c2c2e) });
                    painter.galley(pos2(bx0 + 10.0, y + 7.0), g, Tokens::text_strong());
                    y = r.bottom() + 8.0;
                }
                y += gap;
                if !all.is_empty() {
                    recent_strip(ui, bx0, bx1, y, &all, &thumbs, shown_entry.as_ref(), &mut out);
                }
            });
        }
        // Footer.
        let fy = card.bottom() - footer_h / 2.0;
        painter.hline(card.x_range(), card.bottom() - footer_h, Stroke::new(1.0, Tokens::divider()));
        if w >= 600.0 {
            painter.text(
                pos2(x0, fy),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s("Made by the service you picked, on your own account or computer."),
                theme::body(11.0),
                Tokens::text_faint(),
            );
        }
        let close_label = if running.is_some() { "Cancel" } else { "Close" };
        if button(ui, x1, fy, close_label, Style::Plain, true).0 {
            if running.is_some() {
                acts.push(Act::Cancel);
            } else {
                out.close = true;
            }
        }
    });
    apply(p, &plugin, &panel_id, acts, capture, &dir);
    if let Some(s) = p.sheet.as_mut() {
        s.offset += dragged;
    }
    let pressed_generate =
        p.panels.get(&(plugin.clone(), panel_id.clone())).is_some_and(|s| s.run.as_ref().is_some_and(|r| r.0 == "generate"));
    if let Some(s) = p.sheet.as_mut() {
        if let Some(e) = out.show {
            s.shown = Some(e);
            s.editing = false;
            s.history = false;
        }
        if out.edit {
            s.editing = true;
        }
        if let Some(hv) = out.history {
            s.history = hv;
            s.pending_delete = None;
        }
        if pressed_generate {
            s.editing = false;
        }
    }
    if let Some(prompt_text) = out.reuse {
        if let Some(Control::Text { id, .. }) = &prompt {
            p.set_value(&plugin, &panel_id, id, Value::from(prompt_text));
        }
        if let Some(s) = p.sheet.as_mut() {
            s.history = false;
            s.editing = true;
        }
    }
    if let Some(e) = out.delete {
        let confirmed = p.sheet.as_ref().and_then(|s| s.pending_delete.clone()).is_some_and(|d| d == e.path);
        if confirmed {
            results::remove(&e);
            p.results_of(&plugin).retain(|r| r.path != e.path);
            p.textures.remove(&e.path);
            if let Some(s) = p.sheet.as_mut() {
                s.pending_delete = None;
                if s.shown.as_ref().is_some_and(|x| x.path == e.path) {
                    s.shown = None;
                }
            }
        } else if let Some(s) = p.sheet.as_mut() {
            s.pending_delete = Some(e.path.clone());
        }
    }
    if let Some(path) = out.copy
        && let Ok(bytes) = std::fs::read(&path)
        && let Ok(pic) = shapecraft_plugin_host::images::decode(&bytes)
    {
        ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([pic.width as usize, pic.height as usize], &pic.pixels));
        p.notice = Some((Tone::Good, "Copied.".into()));
    }
    if let Some(src) = out.save {
        let mut pick = crate::dialogs::Pick::default();
        pick.start(|| crate::dialogs::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("Generative Render.png").save_file());
        p.save_pick = Some((src, pick));
    }
    if out.connect {
        p.selected = Some(plugin.clone());
        p.browsing = false;
        if let Some(s) = p.sheet.as_mut() {
            s.hidden = true;
        }
        p.requests.push(Request::OpenPlugins);
    }
    if out.close {
        p.sheet = None;
    }
}

/// The picture: the view, or the result on show, with what made it on pills; dimmed with a spinner while working.
fn draw_stage(
    painter: &egui::Painter,
    stage: Rect,
    pic: Option<&TextureHandle>,
    shown: Option<&results::Entry>,
    running: bool,
    progress: &str,
    now: f64,
) {
    painter.rect_filled(stage, radius(10), theme::c(0x141414));
    if let Some(t) = pic {
        let size = t.size_vec2();
        let k = (stage.width() / size.x).min(stage.height() / size.y);
        let r = Rect::from_center_size(stage.center(), size * k);
        painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    painter.rect_stroke(stage, radius(10), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    // What this picture is: pills in the top corner (they never cover the model's middle).
    let mut px = stage.left() + 10.0;
    match shown {
        Some(e) => {
            for (k, t) in tags(e).iter().enumerate() {
                if px > stage.right() - 60.0 {
                    break;
                }
                px += pill(&painter.with_clip_rect(stage), pos2(px, stage.top() + 10.0), t, 11.5, k == 0) + 6.0;
            }
        }
        None => {
            pill(painter, pos2(px, stage.top() + 10.0), "Your view", 11.5, false);
        }
    }
    if running {
        painter.rect_filled(stage, radius(10), Color32::from_black_alpha(150));
        let pulse = (0.5 + 0.5 * (now * 2.4).sin()) as f32;
        painter.rect_stroke(stage, radius(10), Stroke::new(3.0, Tokens::accent().gamma_multiply(0.35 + 0.45 * pulse)), StrokeKind::Inside);
        spinner(painter, stage.center() - vec2(0.0, 26.0), now);
        let text = if progress.is_empty() { "Generating scene…" } else { progress };
        painter.text(stage.center() + vec2(0.0, 2.0), Align2::CENTER_CENTER, crate::i18n::tr_s(text), theme::strong(13.5), Color32::WHITE);
    }
}

/// The prompt bar's shape for a width: the field's width and height, and whether the buttons go under the words.
fn composer_geometry(
    p: &egui::Painter,
    w: f32,
    text: &str,
    placeholder: &str,
    folded: bool,
    connected: bool,
    touch: bool,
) -> (f32, f32, bool) {
    let btn_h = if touch { 34.0 } else { 28.0 };
    let buttons_w = if !connected {
        110.0
    } else if folded {
        g_w(p, "Edit") + g_w(p, "Try Again") + 18.0
    } else {
        150.0
    };
    let stacked = w < 420.0;
    let field_w = if stacked { w - 16.0 } else { w - 16.0 - buttons_w - 34.0 - 16.0 };
    let lines = if folded || !connected {
        1.0
    } else {
        let g = p.layout(
            crate::i18n::tr_s(if text.is_empty() { placeholder.to_string() } else { text.to_string() }),
            theme::body(13.0),
            Color32::WHITE,
            field_w - 18.0,
        );
        (g.rows.len() as f32).clamp(2.0, 5.0)
    };
    let field_h = (lines * 17.0 + 14.0).max(btn_h);
    (field_w, field_h, stacked)
}

fn composer_height(p: &egui::Painter, w: f32, text: &str, placeholder: &str, folded: bool, connected: bool, touch: bool) -> f32 {
    let (_, field_h, stacked) = composer_geometry(p, w, text, placeholder, folded, connected, touch);
    let btn_h = if touch { 34.0 } else { 28.0 };
    if stacked { field_h + 8.0 + btn_h + 16.0 } else { field_h + 16.0 }
}

/// The prompt bar under the picture. Writing: the words (up to five lines) with Generate and Save beside them.
/// With a result on show: the words in one line, Edit, Try Again and Save. Nothing connected: Connect….
#[allow(clippy::too_many_arguments)]
fn composer(
    ui: &mut Ui,
    bar: Rect,
    prompt: &Option<Control>,
    go: &Option<Control>,
    values: &Map<String, Value>,
    running: bool,
    folded: bool,
    connected: bool,
    shown: Option<&results::Entry>,
    touch: bool,
    acts: &mut Vec<Act>,
    out: &mut SheetOut,
) {
    let painter = ui.painter().clone();
    let (prompt_id, placeholder) = match prompt {
        Some(Control::Text { id, placeholder, .. }) => (id.clone(), placeholder.clone()),
        _ => (String::new(), String::new()),
    };
    let text = values.get(&prompt_id).and_then(Value::as_str).unwrap_or("").to_string();
    let (go_label, go_action, wants) = match go {
        Some(Control::Button { label, action, capture, .. }) => (label.clone(), action.clone(), *capture),
        _ => ("Generate".to_string(), String::new(), false),
    };
    let (field_w, field_h, stacked) = composer_geometry(&painter, bar.width(), &text, &placeholder, folded, connected, touch);
    let btn_h = if touch { 34.0 } else { 28.0 };
    painter.rect_filled(bar, radius(12), Tokens::chip());
    painter.rect_stroke(bar, radius(12), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    let field = Rect::from_min_size(pos2(bar.left() + 8.0, bar.top() + 8.0), vec2(field_w, field_h));
    let by = if stacked { bar.bottom() - 8.0 - btn_h / 2.0 } else { field.center().y };
    // Save: the picture on show (dim until there is one).
    let save_r = Rect::from_center_size(pos2(bar.right() - 8.0 - 15.0, by), vec2(30.0, btn_h));
    let can_save = shown.is_some() && !running;
    let resp = ui.interact(save_r, ui.scope_id().with("sheet-save"), if can_save { Sense::click() } else { Sense::hover() });
    if can_save && resp.hovered() {
        painter.rect_filled(save_r, radius(6), Tokens::button_hover());
    }
    save_glyph(&painter, save_r.center(), if can_save { Tokens::text_strong() } else { Tokens::text_ghost() });
    if can_save && resp.on_hover_text("Save Image…").clicked() {
        out.save = shown.map(|e| e.path.clone());
    }
    let mut right = save_r.left() - 8.0;
    if !connected {
        painter.text(
            pos2(field.left() + 6.0, field.center().y),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("Connect an AI to start"),
            theme::body(13.0),
            Tokens::text_dim(),
        );
        if button(ui, right, by, "Connect…", Style::Primary, true).0 {
            out.connect = true;
        }
        return;
    }
    if folded {
        let one = text.lines().next().unwrap_or("").to_string();
        let g = painter.layout_no_wrap(
            crate::i18n::tr_s(if one.is_empty() { "(no description)".into() } else { one }),
            theme::body(13.0),
            Tokens::text_dim(),
        );
        let clip = Rect::from_min_max(field.min, pos2(field.right() - 4.0, field.max.y));
        painter.with_clip_rect(clip).galley(pos2(field.left() + 8.0, field.center().y - g.size().y / 2.0), g, Tokens::text_dim());
        if ui.interact(field, ui.scope_id().with("sheet-fold"), Sense::click()).on_hover_text("Edit the description").clicked() {
            out.edit = true;
        }
        let (again, r) = button(ui, right, by, "Try Again", Style::Primary, true);
        if again {
            acts.push(Act::Press(go_action.clone(), wants));
        }
        right = r.left() - 8.0;
        if button(ui, right, by, "Edit", Style::Plain, true).0 {
            out.edit = true;
        }
        return;
    }
    let mut edited = text.clone();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
    child.set_clip_rect(field.expand(1.0).intersect(ui.clip_rect()));
    text_box(&mut child, &format!("sheet-{prompt_id}"), &mut edited, field.height(), &placeholder, false, false);
    if edited != text {
        acts.push(Act::Set(prompt_id.clone(), Value::from(edited)));
    }
    let ok = !running && matches!(go, Some(Control::Button { enabled, .. }) if enabled.unwrap_or(true));
    let enter = ok && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
    if button(ui, right, by, &go_label, Style::Primary, ok).0 || enter {
        acts.push(Act::Press(go_action, wants));
    }
}

/// The recent pictures under the options: what made each one on a small pill, and History.
#[allow(clippy::too_many_arguments)]
fn recent_strip(
    ui: &mut Ui,
    x0: f32,
    x1: f32,
    y: f32,
    all: &[results::Entry],
    thumbs: &[Option<TextureHandle>],
    shown: Option<&results::Entry>,
    out: &mut SheetOut,
) {
    let painter = ui.painter().clone();
    let th = 50.0;
    let mut x = x0;
    for (e, t) in all.iter().zip(thumbs) {
        let Some(t) = t else { continue };
        let size = t.size_vec2();
        let tw = (th * size.x / size.y.max(1.0)).min(110.0);
        if x + tw > x1 - 70.0 {
            break;
        }
        let r = Rect::from_min_size(pos2(x, y), vec2(tw, th));
        let resp = ui.interact(r, ui.scope_id().with(("thumb", &e.path)), Sense::click());
        painter.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        let on = shown.is_some_and(|s| s.path == e.path);
        let edge = if on {
            Tokens::accent()
        } else if resp.hovered() {
            Tokens::text_dim()
        } else {
            Tokens::chip_border()
        };
        painter.rect_stroke(r, radius(5), Stroke::new(if on { 2.0 } else { 1.0 }, edge), StrokeKind::Inside);
        if let Some(first) = tags(e).first() {
            pill(&painter.with_clip_rect(r), pos2(r.left() + 3.0, r.bottom() - 17.0), first, 9.5, false);
        }
        if resp.on_hover_text(format!("{}\n{}", e.details, when(e.time))).clicked() {
            out.show = Some(e.clone());
        }
        x += tw + 8.0;
    }
    let more = link(ui, pos2(x1 - 56.0, y + th / 2.0), "History", "sheet-more");
    if ui.interact(more, ui.scope_id().with("sheet-more-btn"), Sense::click()).clicked() {
        out.history = Some(true);
    }
}

// ——— The sheet's switch and tiles ———

fn switch_line_h(touch: bool) -> f32 {
    if touch { 34.0 } else { 30.0 }
}

fn switch_parts(c: &Control) -> (Vec<String>, Vec<String>, &str) {
    match c {
        Control::Segmented { id, options, icons, .. } => (options.clone(), icons.clone(), id.as_str()),
        _ => (Vec::new(), Vec::new(), ""),
    }
}

fn switch_seg_w(p: &egui::Painter, label: &str, icon: bool) -> f32 {
    p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.5), Color32::WHITE).size().x + 30.0 + if icon { 22.0 } else { 0.0 }
}

fn switch_width(p: &egui::Painter, c: &Control) -> f32 {
    let (options, icons, _) = switch_parts(c);
    options.iter().enumerate().map(|(k, o)| switch_seg_w(p, o, icons.get(k).is_some_and(|i| known_icon(i)))).sum::<f32>() + 4.0
}

/// The sheet's top switch between two ways of working: a capsule, the side picked lit, each side with its pictogram.
fn header_switch(ui: &Ui, r: Rect, c: &Control, values: &Map<String, Value>, acts: &mut Vec<Act>) {
    let (options, icons, id) = switch_parts(c);
    if options.is_empty() {
        return;
    }
    let p = ui.painter();
    let cur = values.get(id).and_then(Value::as_str).unwrap_or("");
    let k_cur = options.iter().position(|o| o == cur).unwrap_or(0);
    let round = radius(((r.height() / 2.0) as u8).max(4));
    p.rect_filled(r, round, theme::c(0x1e1e20));
    p.rect_stroke(r, round, Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    let natural: Vec<f32> =
        options.iter().enumerate().map(|(k, o)| switch_seg_w(p, o, icons.get(k).is_some_and(|i| known_icon(i)))).collect();
    let k_fit = (r.width() - 4.0) / natural.iter().sum::<f32>().max(1.0);
    let mut x = r.left() + 2.0;
    for (k, o) in options.iter().enumerate() {
        let sw = natural[k] * k_fit;
        let s = Rect::from_min_size(pos2(x, r.top() + 2.0), vec2(sw, r.height() - 4.0));
        x += sw;
        let resp = ui.interact(s, ui.scope_id().with(("sheet-switch", k)), Sense::click());
        let on = k == k_cur;
        let inner = radius(((s.height() / 2.0) as u8).max(4));
        if on {
            p.rect_filled(s, inner, Tokens::accent());
        } else if resp.hovered() {
            p.rect_filled(s, inner, Tokens::button_hover());
        }
        let g = p.layout_no_wrap(crate::i18n::tr_s(o), theme::body(12.5), Color32::PLACEHOLDER);
        let icon = icons.get(k).filter(|i| known_icon(i));
        let total = g.size().x + if icon.is_some() { 22.0 } else { 0.0 };
        let mut tx = s.center().x - total / 2.0;
        let col = if on { Color32::WHITE } else { Tokens::text_dim() };
        if let Some(i) = icon {
            glyph(p, Rect::from_center_size(pos2(tx + 8.0, s.center().y), vec2(16.0, 16.0)), i, col);
            tx += 22.0;
        }
        p.galley(pos2(tx, s.center().y - g.size().y / 2.0), g, col);
        let resp = match icon.map(|i| icon_tip(i)).filter(|t| !t.is_empty()) {
            Some(t) => resp.on_hover_text(t),
            None => resp,
        };
        if resp.clicked() && !on {
            acts.push(Act::Set(id.to_string(), Value::from(o.clone())));
        }
    }
}

/// Where one of a sheet's options goes: a zone (one question, its own shaded panel with a caption), an option in
/// it, or a zone's words.
enum Placed {
    Zone(Rect, String),
    Item(Rect, Control),
    Text(Rect, String),
}

fn tile_h(touch: bool) -> f32 {
    if touch { 88.0 } else { 80.0 }
}

fn tiled(c: &Control) -> bool {
    match c {
        Control::Segmented { icons, options, .. } => !icons.is_empty() && icons.len() >= options.len(),
        Control::Toggle { icon, .. } => known_icon(icon),
        _ => false,
    }
}

fn tile_w(p: &egui::Painter, label: &str, icon: &str) -> f32 {
    let lw = p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.0), Color32::WHITE).size().x;
    (lw + 24.0).max(pict_size(icon).x + 28.0).max(if pict_size(icon).x < 50.0 { 58.0 } else { 104.0 })
}

/// How many tiles a control shows (0 for anything else).
fn tile_count(c: &Control) -> usize {
    match c {
        Control::Segmented { options, .. } if tiled(c) => options.len(),
        Control::Toggle { .. } if tiled(c) => 1,
        _ => 0,
    }
}

/// The natural width of something that isn't a tile.
fn item_w(p: &egui::Painter, c: &Control, touch: bool) -> f32 {
    match c {
        Control::Segmented { options, .. } => options.iter().map(|o| g_w(p, o).max(40.0) - 4.0).sum(),
        Control::Toggle { label, .. } => p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.5), Color32::WHITE).size().x + 54.0,
        Control::Menu { .. } => {
            if touch {
                260.0
            } else {
                220.0
            }
        }
        Control::Button { label, .. } => p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.0), Color32::WHITE).size().x + 20.0,
        _ => 0.0,
    }
}

/// One question in the sheet: its caption and what answers it.
struct ZoneSpec {
    caption: String,
    items: Vec<Control>,
    text: Option<String>,
}

impl ZoneSpec {
    fn min_w(&self, p: &egui::Painter, touch: bool) -> f32 {
        let tiles: f32 = self
            .items
            .iter()
            .map(|c| match c {
                Control::Segmented { options, icons, .. } if tiled(c) => {
                    options.iter().zip(icons).map(|(o, i)| tile_w(p, o, i) + 8.0).sum()
                }
                Control::Toggle { label, icon, .. } if tiled(c) => tile_w(p, label, icon) + 8.0,
                other => item_w(p, other, touch) + 8.0,
            })
            .sum();
        tiles.max(if self.text.is_some() { 180.0 } else { 0.0 }) + 2.0 * ZONE_PAD
    }
}

const ZONE_PAD: f32 = 12.0;
const ZONE_GAP: f32 = 10.0;

/// A row's zones: a heading starts one, and so does a segmented or menu with its own label.
fn zones_of(row: &[Control]) -> Vec<ZoneSpec> {
    let mut zones: Vec<ZoneSpec> = Vec::new();
    let mut pending: Option<String> = None;
    for c in row {
        match c {
            Control::Heading { text } => {
                pending = Some(text.clone());
                zones.push(ZoneSpec { caption: text.clone(), items: Vec::new(), text: None });
            }
            Control::Note { text, .. } => {
                if let Some(z) = zones.last_mut() {
                    z.text = Some(text.clone());
                }
            }
            other => {
                let own = match other {
                    Control::Segmented { label, .. } | Control::Menu { label, .. } if !label.is_empty() => Some(label.clone()),
                    _ => None,
                };
                let starts = pending.take().is_none() && (own.is_some() || zones.is_empty());
                if starts {
                    zones.push(ZoneSpec { caption: own.unwrap_or_default(), items: Vec::new(), text: None });
                }
                if let Some(z) = zones.last_mut() {
                    z.items.push(other.clone());
                }
            }
        }
    }
    zones.retain(|z| !z.items.is_empty() || z.text.is_some());
    zones
}

/// Lays the rows out in `width` as zones: each question on its own shaded panel, the zones of a row sharing it evenly
/// (stacked when they don't fit side by side), every zone in a row the same height, its answers centred in it.
fn lay_out(p: &egui::Painter, rows: &[Vec<Control>], width: f32, touch: bool) -> (Vec<Placed>, f32) {
    let cap_h = 22.0;
    let row_h = if touch { 36.0 } else { 30.0 };
    let mut placed = Vec::new();
    let mut y = 0.0;
    // Side by side only when every row's zones fit (so the zones line up as a grid from row to row).
    let all: Vec<Vec<ZoneSpec>> = rows.iter().map(|r| zones_of(r)).filter(|z| !z.is_empty()).collect();
    let side = all.iter().all(|zs| {
        let n = zs.len() as f32;
        let share = (width - ZONE_GAP * (n - 1.0)) / n;
        zs.iter().all(|z| z.min_w(p, touch) <= share)
    });
    let lines: Vec<Vec<&ZoneSpec>> =
        if side { all.iter().map(|zs| zs.iter().collect()).collect() } else { all.iter().flatten().map(|z| vec![z]).collect() };
    for (li, line) in lines.iter().enumerate() {
        if li > 0 {
            y += ZONE_GAP;
        }
        let n = line.len() as f32;
        let zw = (width - ZONE_GAP * (n - 1.0)) / n;
        let inner = zw - 2.0 * ZONE_PAD;
        let content_h = |z: &ZoneSpec| -> f32 {
            if z.items.iter().any(tiled) {
                tile_h(touch)
            } else if let Some(t) = &z.text {
                p.layout(crate::i18n::tr_s(t), theme::body(12.0), Color32::WHITE, inner).size().y.max(row_h)
            } else {
                row_h
            }
        };
        let band = line.iter().map(|z| content_h(z)).fold(0.0, f32::max);
        let zh = ZONE_PAD * 0.75 + cap_h + band + ZONE_PAD;
        for (k, z) in line.iter().enumerate() {
            let zr = Rect::from_min_size(pos2(k as f32 * (zw + ZONE_GAP), y), vec2(zw, zh));
            placed.push(Placed::Zone(zr, z.caption.clone()));
            let top = zr.top() + ZONE_PAD * 0.75 + cap_h;
            let tiles: usize = z.items.iter().map(tile_count).sum();
            if tiles > 0 {
                // Even tiles across the zone (up to a comfortable size), the group centred.
                let tw = ((inner - 8.0 * (tiles as f32 - 1.0)) / tiles as f32).min(150.0);
                let group = tw * tiles as f32 + 8.0 * (tiles as f32 - 1.0);
                let mut x = zr.left() + ZONE_PAD + (inner - group) / 2.0;
                let ty = top + (band - tile_h(touch)) / 2.0;
                for c in &z.items {
                    let t = tile_count(c);
                    if t == 0 {
                        continue;
                    }
                    let w = tw * t as f32 + 8.0 * (t as f32 - 1.0);
                    placed.push(Placed::Item(Rect::from_min_size(pos2(x, ty), vec2(w, tile_h(touch))), c.clone()));
                    x += w + 8.0;
                }
            } else if let Some(t) = &z.text {
                let th = p.layout(crate::i18n::tr_s(t), theme::body(12.0), Color32::WHITE, inner).size().y;
                placed.push(Placed::Text(
                    Rect::from_min_size(pos2(zr.left() + ZONE_PAD, top + (band - th) / 2.0), vec2(inner, th)),
                    t.clone(),
                ));
            } else {
                let mut x = zr.left() + ZONE_PAD;
                for c in &z.items {
                    let w = if matches!(c, Control::Menu { .. }) { inner } else { item_w(p, c, touch).min(inner) };
                    placed.push(Placed::Item(Rect::from_min_size(pos2(x, top + (band - row_h) / 2.0), vec2(w, row_h)), c.clone()));
                    x += w + 8.0;
                }
            }
        }
        y += zh;
    }
    (placed, y)
}

/// Draws what [`lay_out`] placed, offset to `origin`.
fn draw_options(ui: &mut Ui, placed: &[Placed], origin: egui::Pos2, values: &Map<String, Value>, acts: &mut Vec<Act>) {
    let o = origin.to_vec2();
    for (n, item) in placed.iter().enumerate() {
        match item {
            Placed::Zone(r, caption) => {
                let r = r.translate(o);
                let p = ui.painter();
                p.rect_filled(r, radius(12), theme::c(0x1e1e1f));
                p.rect_stroke(r, radius(12), Stroke::new(1.0, theme::c(0x313133)), StrokeKind::Inside);
                p.text(
                    pos2(r.left() + ZONE_PAD, r.top() + ZONE_PAD * 0.75 + 9.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(caption),
                    theme::strong(11.5),
                    Tokens::text_dim(),
                );
            }
            Placed::Text(r, text) => {
                let r = r.translate(o);
                let g = ui.painter().layout(crate::i18n::tr_s(text), theme::body(12.0), Tokens::text_dim(), r.width());
                // One line sits in the middle of its zone, longer text from the left.
                let x = if g.rows.len() == 1 { r.center().x - g.size().x / 2.0 } else { r.left() };
                ui.painter().galley(pos2(x, r.top()), g, Tokens::text_dim());
            }
            Placed::Item(r, c) => {
                let r = r.translate(o);
                match c {
                    Control::Segmented { id, options, icons, .. } if tiled(c) => {
                        let cur = values.get(id).and_then(Value::as_str).unwrap_or("");
                        let k_cur = options.iter().position(|x| x == cur).unwrap_or(0);
                        let k_n = options.len().max(1) as f32;
                        let tw = (r.width() - 8.0 * (k_n - 1.0)) / k_n;
                        for (k, opt) in options.iter().enumerate() {
                            let tr = Rect::from_min_size(pos2(r.left() + k as f32 * (tw + 8.0), r.top()), vec2(tw, r.height()));
                            let icon = icons.get(k).map(String::as_str).unwrap_or("");
                            if tile(ui, tr, ui.scope_id().with(("sheet-tile", n, k)), opt, icon, k == k_cur, false) && k != k_cur {
                                acts.push(Act::Set(id.clone(), Value::from(opt.clone())));
                            }
                        }
                    }
                    Control::Toggle { id, label, icon, .. } if tiled(c) => {
                        let on = values.get(id).and_then(Value::as_bool).unwrap_or(false);
                        if tile(ui, r, ui.scope_id().with(("sheet-tile", n)), label, icon, on, true) {
                            acts.push(Act::Set(id.clone(), Value::from(!on)));
                        }
                    }
                    _ => {
                        let mut row_ui =
                            ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        row_ui.push_id(("sheet-item", n), |ui| inline_control(ui, c, values, r.height(), acts));
                    }
                }
            }
        }
    }
}

/// One option as a tile: its pictogram over its name, lit when picked (a toggle also shows a tick).
fn tile(ui: &Ui, r: Rect, id: egui::Id, label: &str, icon: &str, on: bool, check: bool) -> bool {
    let resp = ui.interact(r, id, Sense::click());
    let p = ui.painter();
    let fill = if on {
        Tokens::accent().gamma_multiply(0.16)
    } else if resp.hovered() {
        Tokens::chip_hover()
    } else {
        Tokens::chip()
    };
    p.rect_filled(r, radius(10), fill);
    p.rect_stroke(
        r,
        radius(10),
        Stroke::new(if on { 1.6 } else { 1.0 }, if on { Tokens::accent() } else { Tokens::chip_border() }),
        StrokeKind::Inside,
    );
    let ps = pict_size(icon);
    let pict = Rect::from_center_size(pos2(r.center().x, r.top() + 10.0 + 20.0), ps);
    pictogram(p, pict, icon, on);
    let g = p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.0), Color32::PLACEHOLDER);
    let col = if on { Tokens::text_strong() } else { Tokens::text_dim() };
    let lr = Rect::from_min_max(pos2(r.left() + 4.0, r.top()), pos2(r.right() - 4.0, r.bottom()));
    p.with_clip_rect(lr).galley(pos2(r.center().x - g.size().x / 2.0, r.bottom() - 10.0 - g.size().y), g, col);
    if check {
        let c = pos2(r.right() - 10.0, r.top() + 10.0);
        if on {
            tick_badge(p, c, 6.5, Tokens::accent());
        } else {
            p.circle_stroke(c, 6.0, Stroke::new(1.2, Tokens::text_faint()));
        }
    }
    let tip = icon_tip(icon);
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    resp.clicked()
}

fn known_icon(name: &str) -> bool {
    shapecraft_plugin_host::panel::ICONS.contains(&name)
}

/// What each pictogram's option does, said once more on hover.
fn icon_tip(name: &str) -> &'static str {
    match name {
        "surroundings" => "A new scene around your model. The model itself stays as it is.",
        "whole-picture" => "The whole picture restyled, your model included.",
        "keep-exact" => "Your model's own pixels go back on top: same shape, place and size.",
        "move-resize" => "The model may be moved, turned and resized to suit the scene.",
        "pictures-1" => "One picture.",
        "pictures-2" => "Two pictures to pick from.",
        "pictures-4" => "Four pictures to pick from.",
        "service" => "An AI service on your own account, or one running on this computer.",
        "chat" => "A free chat website: your view goes on the clipboard and the site opens.",
        _ => "",
    }
}

fn pict_size(name: &str) -> egui::Vec2 {
    match name {
        "pictures-1" | "pictures-2" | "pictures-4" => vec2(36.0, 28.0),
        "chat" | "service" | "sparkle" => vec2(34.0, 28.0),
        _ => vec2(62.0, 38.0),
    }
}

/// A cube's silhouette (the model in the pictograms): three faces in `base`'s shades.
fn cube(p: &egui::Painter, c: egui::Pos2, s: f32, base: Color32) {
    let k = 0.866 * s;
    let top = c + vec2(0.0, -s);
    let ur = c + vec2(k, -0.5 * s);
    let lr = c + vec2(k, 0.5 * s);
    let bot = c + vec2(0.0, s);
    let ll = c + vec2(-k, 0.5 * s);
    let ul = c + vec2(-k, -0.5 * s);
    let shade = |f: f32| {
        Color32::from_rgba_unmultiplied(
            (base.r() as f32 * f).min(255.0) as u8,
            (base.g() as f32 * f).min(255.0) as u8,
            (base.b() as f32 * f).min(255.0) as u8,
            base.a(),
        )
    };
    p.add(egui::Shape::convex_polygon(vec![top, ur, c, ul], shade(1.0), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![ul, c, bot, ll], shade(0.78), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![c, ur, lr, bot], shade(0.6), Stroke::NONE));
}

/// A four-pointed star.
fn star(p: &egui::Painter, c: egui::Pos2, s: f32, col: Color32) {
    let t = s * 0.26;
    p.add(egui::Shape::convex_polygon(vec![c + vec2(0.0, -s), c + vec2(t, 0.0), c + vec2(0.0, s), c + vec2(-t, 0.0)], col, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![c + vec2(-s, 0.0), c + vec2(0.0, -t), c + vec2(s, 0.0), c + vec2(0.0, t)], col, Stroke::NONE));
}

/// A little landscape (sky, two hills, a sun) filling `r`.
fn scene(p: &egui::Painter, r: Rect, accent: Color32) {
    let clip = p.with_clip_rect(r.intersect(p.clip_rect()));
    clip.rect_filled(r, radius(4), accent.gamma_multiply(0.22));
    clip.circle_filled(pos2(r.left() + r.width() * 0.22, r.bottom() + r.height() * 0.55), r.height() * 0.9, accent.gamma_multiply(0.55));
    clip.circle_filled(pos2(r.left() + r.width() * 0.8, r.bottom() + r.height() * 0.6), r.height() * 0.85, accent.gamma_multiply(0.75));
    clip.circle_filled(pos2(r.right() - r.width() * 0.2, r.top() + r.height() * 0.26), r.height() * 0.12, accent);
}

/// The pictures the sheet's tiles carry: what an option does, the part it changes in the accent color.
fn pictogram(p: &egui::Painter, r: Rect, name: &str, on: bool) {
    let accent = if on { Tokens::icon_blue() } else { Tokens::icon_blue().gamma_multiply(0.6) };
    let model = if on { theme::c(0xe4e4e6) } else { theme::c(0xa8a8ac) };
    let frame = Stroke::new(1.2, if on { Tokens::text_strong() } else { Tokens::text_faint() });
    let s = r.height() * 0.27;
    let mid = pos2(r.center().x, r.center().y + r.height() * 0.06);
    match name {
        "surroundings" => {
            scene(p, r, accent);
            p.rect_stroke(r, radius(4), frame, StrokeKind::Inside);
            cube(p, mid, s, model);
        }
        "whole-picture" => {
            scene(p, r, accent);
            p.rect_stroke(r, radius(4), frame, StrokeKind::Inside);
            cube(p, mid, s, Tokens::icon_blue().gamma_multiply(if on { 1.0 } else { 0.75 }));
            star(p, pos2(r.left() + r.width() * 0.2, r.top() + r.height() * 0.3), r.height() * 0.16, Color32::WHITE.gamma_multiply(0.9));
            star(p, mid + vec2(s * 1.35, -s * 0.9), r.height() * 0.1, Color32::WHITE.gamma_multiply(0.9));
        }
        "keep-exact" => {
            p.rect_stroke(r, radius(4), frame, StrokeKind::Inside);
            let b = Rect::from_center_size(mid, vec2(s * 2.6, s * 2.8));
            let dash = Stroke::new(1.2, accent);
            let pts = [b.left_top(), b.right_top(), b.right_bottom(), b.left_bottom(), b.left_top()];
            p.extend(egui::Shape::dashed_line(&pts, dash, 3.0, 2.5));
            cube(p, mid, s, model);
            // A small lock: pinned in place.
            let lc = pos2(b.right() + 6.0, b.bottom() - 3.0);
            let body = Rect::from_center_size(lc + vec2(0.0, 1.5), vec2(8.0, 6.0));
            p.rect_filled(body, radius(1), accent);
            let arc: Vec<egui::Pos2> = (0..=8)
                .map(|i| {
                    lc + vec2(
                        -2.6 * (std::f32::consts::PI * i as f32 / 8.0).cos(),
                        -1.5 - 3.0 * (std::f32::consts::PI * i as f32 / 8.0).sin(),
                    )
                })
                .collect();
            p.add(egui::Shape::line(arc, Stroke::new(1.4, accent)));
        }
        "move-resize" => {
            p.rect_stroke(r, radius(4), frame, StrokeKind::Inside);
            // Where it was (a dashed outline), where it may go (smaller, higher), and the way between.
            let from = mid + vec2(-r.width() * 0.2, s * 0.2);
            let k = 0.866 * s;
            let hex = [
                from + vec2(0.0, -s),
                from + vec2(k, -0.5 * s),
                from + vec2(k, 0.5 * s),
                from + vec2(0.0, s),
                from + vec2(-k, 0.5 * s),
                from + vec2(-k, -0.5 * s),
                from + vec2(0.0, -s),
            ];
            p.extend(egui::Shape::dashed_line(&hex, Stroke::new(1.1, Tokens::text_faint()), 2.5, 2.0));
            let to = mid + vec2(r.width() * 0.24, -s * 0.45);
            cube(p, to, s * 0.75, model);
            let a = from + vec2(k + 3.0, -2.0);
            let b = to + vec2(-0.866 * s * 0.75 - 4.0, 2.0);
            let st = Stroke::new(1.5, accent);
            p.line_segment([a, b], st);
            let dir = (b - a).normalized();
            let side = vec2(-dir.y, dir.x);
            p.line_segment([b, b - dir * 5.0 + side * 3.5], st);
            p.line_segment([b, b - dir * 5.0 - side * 3.5], st);
        }
        "pictures-1" | "pictures-2" | "pictures-4" => {
            let n = match name {
                "pictures-1" => 1,
                "pictures-2" => 2,
                _ => 4,
            };
            let cells: Vec<Rect> = match n {
                1 => vec![r],
                2 => {
                    let w = (r.width() - 4.0) / 2.0;
                    vec![
                        Rect::from_min_size(r.min, vec2(w, r.height())),
                        Rect::from_min_size(r.min + vec2(w + 4.0, 0.0), vec2(w, r.height())),
                    ]
                }
                _ => {
                    let (w, h) = ((r.width() - 3.0) / 2.0, (r.height() - 3.0) / 2.0);
                    (0..4)
                        .map(|i| Rect::from_min_size(r.min + vec2((i % 2) as f32 * (w + 3.0), (i / 2) as f32 * (h + 3.0)), vec2(w, h)))
                        .collect()
                }
            };
            for c in cells {
                p.rect_filled(c, radius(3), accent.gamma_multiply(0.3));
                p.rect_stroke(c, radius(3), frame, StrokeKind::Inside);
                cube(p, c.center() + vec2(0.0, 1.0), c.height().min(c.width()) * 0.24, model);
            }
        }
        _ => glyph(p, r, name, if on { Tokens::icon_blue() } else { Tokens::text_dim() }),
    }
}

/// The small one-color glyphs (the switch, chat tiles): an AI service (sparkles), a chat (a speech bubble).
fn glyph(p: &egui::Painter, r: Rect, name: &str, col: Color32) {
    let c = r.center();
    let s = r.height().min(r.width()) / 2.0;
    match name {
        "service" | "sparkle" => {
            star(p, c + vec2(-s * 0.15, s * 0.12), s * 0.85, col);
            star(p, c + vec2(s * 0.68, -s * 0.6), s * 0.38, col);
        }
        "chat" => {
            let b = Rect::from_center_size(c + vec2(0.0, -s * 0.12), vec2(s * 1.9, s * 1.35));
            p.rect_stroke(b, radius(((s * 0.4) as u8).max(2)), Stroke::new(1.5, col), StrokeKind::Middle);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pos2(b.left() + s * 0.35, b.bottom() - 0.5),
                    pos2(b.left() + s * 0.85, b.bottom() - 0.5),
                    pos2(b.left() + s * 0.3, b.bottom() + s * 0.5),
                ],
                col,
                Stroke::NONE,
            ));
            for k in -1..=1 {
                p.circle_filled(b.center() + vec2(k as f32 * s * 0.48, 0.0), (s * 0.13).max(1.2), col);
            }
        }
        _ => {}
    }
}

/// A push button's width for `label`.
fn g_w(p: &egui::Painter, label: &str) -> f32 {
    p.layout_no_wrap(crate::i18n::tr_s(label), theme::body(13.0), Color32::WHITE).size().x + 26.0
}

/// Every picture kept, newest first: the picture with what made it, when, the words, and Save / Copy / Use Prompt /
/// Delete.
fn history_grid(
    ui: &mut Ui,
    area: Rect,
    all: &[results::Entry],
    thumbs: &[Option<TextureHandle>],
    pending_delete: Option<&PathBuf>,
    touch: bool,
    out: &mut SheetOut,
) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.set_clip_rect(area);
    let cols = if area.width() < 420.0 {
        1
    } else if area.width() < 520.0 {
        2
    } else {
        3
    };
    let gap = 14.0;
    let cw = (area.width() - gap * (cols as f32 - 1.0) - 10.0) / cols as f32;
    let img_h = cw * 0.62;
    let card_h = img_h + if touch { 92.0 } else { 84.0 };
    egui::ScrollArea::vertical().id_salt("gr-history").auto_shrink([false, false]).show(&mut child, |ui| {
        let rows = all.len().div_ceil(cols);
        let (block, _) = ui.allocate_exact_size(vec2(area.width() - 10.0, rows as f32 * (card_h + gap)), Sense::hover());
        let p = ui.painter().clone();
        for (i, e) in all.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            let cell =
                Rect::from_min_size(pos2(block.left() + c as f32 * (cw + gap), block.top() + r as f32 * (card_h + gap)), vec2(cw, card_h));
            if !ui.is_rect_visible(cell) {
                continue;
            }
            let img = Rect::from_min_size(cell.min, vec2(cw, img_h));
            p.rect_filled(img, radius(7), theme::c(0x141414));
            if let Some(Some(t)) = thumbs.get(i) {
                let size = t.size_vec2();
                let k = (img.width() / size.x).max(img.height() / size.y);
                // Fill the tile, cropping the middle.
                let uv_w = (img.width() / (size.x * k)).min(1.0);
                let uv_h = (img.height() / (size.y * k)).min(1.0);
                let uv = Rect::from_center_size(pos2(0.5, 0.5), vec2(uv_w, uv_h));
                p.image(t.id(), img, uv, Color32::WHITE);
            }
            let resp = ui.interact(img, ui.scope_id().with(("hist", i)), Sense::click());
            if resp.hovered() {
                p.rect_stroke(img, radius(7), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
            }
            if resp.on_hover_text("Show it in the sheet").clicked() {
                out.show = Some(e.clone());
            }
            let mut px = img.left() + 6.0;
            for (k, t) in tags(e).iter().take(3).enumerate() {
                if px > img.right() - 40.0 {
                    break;
                }
                px += pill(&p.with_clip_rect(img), pos2(px, img.top() + 6.0), t, 9.5, k == 0) + 4.0;
            }
            p.text(
                pos2(cell.left(), img.bottom() + 12.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(when(e.time)),
                theme::strong(11.0),
                Tokens::text_dim(),
            );
            let g = p.layout(crate::i18n::tr_s(&e.title), theme::body(11.5), Tokens::text_strong(), cw);
            let clip = Rect::from_min_size(pos2(cell.left(), img.bottom() + 20.0), vec2(cw, 30.0));
            p.with_clip_rect(clip).galley(clip.min, g, Tokens::text_strong());
            let ly = cell.bottom() - 10.0;
            let mut lx = cell.left();
            let deleting = pending_delete.is_some_and(|d| *d == e.path);
            let labels: [(&str, u8); 4] =
                [("Save", 0), ("Copy", 1), ("Use Prompt", 2), (if deleting { "Really Delete" } else { "Delete" }, 3)];
            for (label, what) in labels {
                let lr = link(ui, pos2(lx, ly), label, "hist-link");
                if ui.interact(lr, ui.scope_id().with(("hist-act", i, what)), Sense::click()).clicked() {
                    match what {
                        0 => out.save = Some(e.path.clone()),
                        1 => out.copy = Some(e.path.clone()),
                        2 => out.reuse = Some(e.title.clone()),
                        _ => out.delete = Some(e.clone()),
                    }
                }
                lx = lr.right() + 12.0;
            }
        }
    });
}

/// A control in one of the sheet's rows (its label is the caption over it).
fn inline_control(ui: &mut Ui, c: &Control, values: &Map<String, Value>, h: f32, acts: &mut Vec<Act>) {
    match c {
        Control::Segmented { id, options, .. } => {
            let cur = values.get(id).and_then(Value::as_str).unwrap_or("");
            let k = options.iter().position(|o| o == cur).unwrap_or(0);
            let w: f32 = options.iter().map(|o| g_w(ui.painter(), o).max(40.0) - 4.0).sum();
            let (r, _) = ui.allocate_exact_size(vec2(w, h - 4.0), Sense::hover());
            let refs: Vec<&str> = options.iter().map(String::as_str).collect();
            if let Some(n) = segmented_at(ui, r, &format!("sh-seg-{id}"), &refs, k) {
                acts.push(Act::Set(id.clone(), Value::from(options[n].clone())));
            }
        }
        Control::Toggle { id, label, .. } => {
            let mut on = values.get(id).and_then(Value::as_bool).unwrap_or(false);
            let g = ui.painter().layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.5), Tokens::text_strong());
            let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 54.0, h), Sense::hover());
            ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, Tokens::text_strong());
            let before = on;
            switch(ui, r.right(), r.center().y, ("sh-toggle", hash(id)), &mut on);
            if on != before {
                acts.push(Act::Set(id.clone(), Value::from(on)));
            }
        }
        Control::Menu { id, options, empty, .. } => {
            let cur = values.get(id).and_then(Value::as_str).unwrap_or("").to_string();
            let w = ui.available_width();
            if let Some(v) = menu(ui, id, options, &cur, empty, w) {
                acts.push(Act::Set(id.clone(), Value::from(v)));
            }
        }
        Control::Heading { text } => {
            ui.label(egui::RichText::new(text).font(theme::body(12.0)).color(Tokens::text_dim()));
        }
        Control::Button { label, action, capture, .. } => {
            // A small pill (the free chats).
            let g = ui.painter().layout_no_wrap(crate::i18n::tr_s(label), theme::body(12.0), Tokens::text_strong());
            let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, h - 6.0), Sense::click());
            let fill = if resp.hovered() { Tokens::button_hover() } else { Tokens::chip() };
            ui.painter().rect_filled(r, radius(((r.height() / 2.0) as u8).max(4)), fill);
            ui.painter().rect_stroke(
                r,
                radius(((r.height() / 2.0) as u8).max(4)),
                Stroke::new(1.0, Tokens::chip_border()),
                StrokeKind::Inside,
            );
            ui.painter().galley(pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, Tokens::text_strong());
            if resp.clicked() {
                acts.push(Act::Press(action.clone(), *capture));
            }
        }
        _ => {}
    }
}

/// The free-chat card: the view is on the clipboard and the site is open; paste the picture, then the words.
fn handoff_card(p: &mut Plugins, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let touch = crate::tablet::touch_ui();
    let view = p.sheet.as_ref().and_then(|s| s.capture.as_ref()).map(|c| c.view.clone());
    let Some(h) = p.handoff.as_mut() else { return };
    if !h.started {
        h.started = true;
        if !touch && let Some(v) = &view {
            ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([v.width as usize, v.height as usize], &v.pixels));
            h.copied = Some("picture");
        }
        p.urls.push(h.url.clone());
    }
    let Some(h) = p.handoff.as_ref() else { return };
    let (site, text, url, copied) = (h.site.clone(), h.text.clone(), h.url.clone(), h.copied);
    let mut out: Option<&'static str> = None;
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        out = Some("close");
    }
    let paste = if cfg!(target_os = "macos") { "⌘V" } else { "Ctrl+V" };
    dim_and_card(ui, egui::Id::NULL.with("plugin-handoff"), 480.0, 330.0, |ui, card| {
        let painter = ui.painter().clone();
        let pad = 26.0;
        let x0 = card.left() + pad;
        painter.text(
            pos2(x0, card.top() + 34.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("Make it in {site}, free")),
            theme::strong(17.0),
            Tokens::text_strong(),
        );
        painter.text(
            pos2(x0, card.top() + 56.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("{site} is opening in your browser. Sign in there if it asks.")),
            theme::body(12.0),
            Tokens::text_dim(),
        );
        let steps: Vec<(String, String)> = if touch {
            vec![
                ("Save the view".into(), format!("Then attach it in {site} (the + or paperclip by the message box).")),
                ("Copy the description".into(), "Paste it under the picture and send.".into()),
                ("Bring the result back".into(), "Save the picture it makes, then Add It to History here.".into()),
            ]
        } else {
            vec![
                ("Paste the view".into(), format!("It's on the clipboard: click the message box and press {paste}.")),
                ("Copy the description".into(), format!("Then press {paste} again, under the picture, and send.")),
                ("Bring the result back".into(), "Save the picture it makes, then Add It to History here.".into()),
            ]
        };
        let mut y = card.top() + 88.0;
        for (k, (t, d)) in steps.iter().enumerate() {
            let c = pos2(x0 + 11.0, y + 8.0);
            painter.circle_filled(c, 11.0, Tokens::accent());
            painter.text(c, Align2::CENTER_CENTER, crate::i18n::tr_s(format!("{}", k + 1)), theme::strong(12.0), Color32::WHITE);
            painter.text(pos2(x0 + 32.0, y), Align2::LEFT_CENTER, crate::i18n::tr_s(t), theme::strong(13.0), Tokens::text_strong());
            painter.text(pos2(x0 + 32.0, y + 17.0), Align2::LEFT_CENTER, crate::i18n::tr_s(d), theme::body(11.5), Tokens::text_dim());
            y += 46.0;
        }
        let note = match copied {
            Some("picture") => "The view is on the clipboard.",
            Some("words") => "The description is on the clipboard.",
            _ => "",
        };
        painter.text(pos2(x0, y + 2.0), Align2::LEFT_CENTER, crate::i18n::tr_s(note), theme::body(11.5), tone_color(Tone::Good));
        let by = card.bottom() - 30.0;
        painter.hline(card.x_range(), by - 24.0, Stroke::new(1.0, Tokens::divider()));
        let (copy, r1) = button(ui, card.right() - pad + 8.0, by, "Copy Description", Style::Primary, true);
        if copy {
            out = Some("words");
        }
        let (first, r2) =
            button(ui, r1.left() - 8.0, by, if touch { "Save the View…" } else { "Copy the View" }, Style::Plain, view.is_some());
        if first {
            out = Some(if touch { "save" } else { "picture" });
        }
        let (add, _) = button(ui, r2.left() - 8.0, by, "Add It to History…", Style::Plain, true);
        if add {
            out = Some("add");
        }
        let lr = link(ui, pos2(x0, by), "Done", "handoff-done");
        if ui.interact(lr, ui.scope_id().with("handoff-done-btn"), Sense::click()).clicked() {
            out = Some("close");
        }
        let _ = url;
    });
    match out {
        Some("words") => {
            ctx.copy_text(text);
            if let Some(h) = p.handoff.as_mut() {
                h.copied = Some("words");
            }
        }
        Some("picture") => {
            if let Some(v) = &view {
                ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([v.width as usize, v.height as usize], &v.pixels));
            }
            if let Some(h) = p.handoff.as_mut() {
                h.copied = Some("picture");
            }
        }
        Some("save") => {
            if let Some(v) = &view
                && let Ok(png) = shapecraft_plugin_host::images::encode_png(v)
            {
                let tmp = std::env::temp_dir().join("ShapeCraft view.png");
                if std::fs::write(&tmp, png).is_ok() {
                    let mut pick = crate::dialogs::Pick::default();
                    pick.start(|| {
                        crate::dialogs::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("ShapeCraft view.png").save_file()
                    });
                    p.save_pick = Some((tmp, pick));
                }
            }
        }
        Some("add") => {
            if !p.add_pick.is_open() {
                p.add_pick.start(|| crate::dialogs::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp"]).pick_file());
            }
        }
        Some(_) => p.handoff = None,
        None => {}
    }
}

/// A tray with a down arrow (save).
fn save_glyph(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.line_segment([c + vec2(0.0, -7.0), c + vec2(0.0, 3.0)], s);
    p.line_segment([c + vec2(-4.0, -1.0), c + vec2(0.0, 3.0)], s);
    p.line_segment([c + vec2(4.0, -1.0), c + vec2(0.0, 3.0)], s);
    p.line_segment([c + vec2(-7.0, 2.0), c + vec2(-7.0, 7.0)], s);
    p.line_segment([c + vec2(-7.0, 7.0), c + vec2(7.0, 7.0)], s);
    p.line_segment([c + vec2(7.0, 7.0), c + vec2(7.0, 2.0)], s);
}

/// A round badge with a tick (permissions, "ready").
fn tick_badge(p: &egui::Painter, c: egui::Pos2, r: f32, fill: Color32) {
    p.circle_filled(c, r, fill);
    let s = Stroke::new((r * 0.22).max(1.4), Color32::WHITE);
    p.line_segment([c + vec2(-r * 0.42, r * 0.02), c + vec2(-r * 0.1, r * 0.34)], s);
    p.line_segment([c + vec2(-r * 0.1, r * 0.34), c + vec2(r * 0.45, -r * 0.3)], s);
}

/// The plugin's tile, bigger and with a soft sheen (the cards).
fn hero_icon(p: &egui::Painter, r: Rect, which: &str) {
    let base = if which == "sparkle" { Tokens::accent() } else { theme::c(0x4a4a4e) };
    p.rect_filled(r.translate(vec2(0.0, 3.0)), radius(14), Color32::from_black_alpha(60));
    p.rect_filled(r, radius(14), base);
    let top = Rect::from_min_max(r.min, pos2(r.max.x, r.center().y));
    p.rect_filled(top, egui::CornerRadius { nw: 14, ne: 14, sw: 0, se: 0 }, Color32::from_white_alpha(18));
    let icon = match which {
        "sparkle" => crate::icons::Icon::CameraSparkle,
        "image" => crate::icons::Icon::Screenshot,
        _ => crate::icons::Icon::Insert,
    };
    crate::icons::paint(p, r.shrink(r.width() * 0.22), icon, Color32::WHITE);
}

/// Centred text wrapped to `w`; returns its height.
fn centred(p: &egui::Painter, top: egui::Pos2, w: f32, text: &str, font: egui::FontId, color: Color32) -> f32 {
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, color, w);
    job.halign = egui::Align::Center;
    let g = p.layout_job(job);
    let h = g.size().y;
    // A centred layout is laid out around x = 0: paint it from the middle.
    p.galley(pos2(top.x + w / 2.0, top.y), g, color);
    h
}

/// Rows in a rounded group (as the Export dialog's switches): a badge, a bold line and a dim one under it, each
/// wrapped to the width. Draws at `top`, `w` wide; returns the height.
fn feature_rows(p: &egui::Painter, top: egui::Pos2, w: f32, rows: &[(&str, &str)], badge: Color32) -> f32 {
    let text_w = w - 46.0 - 14.0;
    let laid: Vec<(std::sync::Arc<egui::Galley>, Option<std::sync::Arc<egui::Galley>>)> = rows
        .iter()
        .map(|(title, detail)| {
            let font = if detail.is_empty() { theme::body(13.0) } else { theme::strong(13.0) };
            let t = p.layout(crate::i18n::tr_s(title), font, Tokens::text_strong(), text_w);
            let d = (!detail.is_empty()).then(|| p.layout(crate::i18n::tr_s(detail), theme::body(11.5), Tokens::text_dim(), text_w));
            (t, d)
        })
        .collect();
    let heights: Vec<f32> =
        laid.iter().map(|(t, d)| (t.size().y + d.as_ref().map_or(0.0, |d| d.size().y + 2.0) + 22.0).max(42.0)).collect();
    let total: f32 = heights.iter().sum();
    let rect = Rect::from_min_size(top, vec2(w, total));
    p.rect_filled(rect, radius(10), Tokens::chip());
    p.rect_stroke(rect, radius(10), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
    let mut y = top.y;
    for (k, ((t, d), h)) in laid.into_iter().zip(&heights).enumerate() {
        let r = Rect::from_min_size(pos2(rect.left(), y), vec2(w, *h));
        if k > 0 {
            p.hline((r.left() + 46.0)..=(r.right() - 14.0), r.top(), Stroke::new(1.0, Tokens::chip_border()));
        }
        tick_badge(p, pos2(r.left() + 24.0, r.center().y), 9.0, badge);
        let block = t.size().y + d.as_ref().map_or(0.0, |d| d.size().y + 2.0);
        let ty = r.center().y - block / 2.0;
        let th = t.size().y;
        p.galley(pos2(r.left() + 46.0, ty), t, Tokens::text_strong());
        if let Some(d) = d {
            p.galley(pos2(r.left() + 46.0, ty + th + 2.0), d, Tokens::text_dim());
        }
        y += h;
    }
    total
}

/// The card the Generative Render button shows before the plugin is added.
fn offer_card(p: &mut Plugins, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let mut close = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    let entry =
        p.index.as_ref().and_then(|r| r.as_ref().ok()).and_then(|i| i.plugins.iter().find(|e| e.manifest.id == GENERATIVE).cloned());
    let failed = p.index.as_ref().and_then(|r| r.as_ref().err()).cloned();
    let mut add = false;
    let mut retry = false;
    dim_and_card(ui, egui::Id::NULL.with("plugin-offer"), 480.0, 446.0, |ui, card| {
        let painter = ui.painter().clone();
        let pad = 28.0;
        let mut y = card.top() + 32.0;
        hero_icon(&painter, Rect::from_center_size(pos2(card.center().x, y + 28.0), vec2(56.0, 56.0)), "sparkle");
        y += 56.0 + 20.0;
        painter.text(
            pos2(card.center().x, y + 11.0),
            Align2::CENTER_CENTER,
            crate::i18n::tr_s("Generative Render"),
            theme::strong(19.0),
            Tokens::text_strong(),
        );
        y += 32.0;
        y += centred(
            &painter,
            pos2(card.left() + pad, y),
            card.width() - 2.0 * pad,
            "Describe a scene and get a picture of your model in it.",
            theme::body(13.5),
            Tokens::text_dim(),
        );
        y += 20.0;
        let rows = [
            ("Your own AI", "OpenRouter, OpenAI or Google Gemini, or free on your computer"),
            ("Your model stays exact", "Only the scene around it is made up"),
            ("No limits of ours", "Make as many as your provider allows"),
        ];
        y += feature_rows(&painter, pos2(card.left() + pad, y), card.width() - 2.0 * pad, &rows, Tokens::accent()) + 16.0;
        centred(
            &painter,
            pos2(card.left() + pad, y),
            card.width() - 2.0 * pad,
            "It's a plugin, so ShapeCraft stays lean for everyone who doesn't use it.",
            theme::body(11.5),
            Tokens::text_faint(),
        );
        let by = card.bottom() - 34.0;
        painter.hline(card.x_range(), by - 26.0, Stroke::new(1.0, Tokens::divider()));
        match (&entry, &failed) {
            (Some(_), _) => {
                if button(ui, card.right() - pad + 8.0, by, "Add Plugin…", Style::Primary, p.adding.is_none()).0 {
                    add = true;
                }
            }
            (None, Some(e)) => {
                painter.text(
                    pos2(card.left() + pad, by),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(short(e, 46)),
                    theme::body(11.5),
                    Tokens::danger(),
                );
                if button(ui, card.right() - pad + 8.0, by, "Try Again", Style::Primary, true).0 {
                    retry = true;
                }
            }
            (None, None) => {
                spinner(&painter, pos2(card.left() + pad + 8.0, by), now);
                painter.text(
                    pos2(card.left() + pad + 24.0, by),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s("Looking it up…"),
                    theme::body(12.0),
                    Tokens::text_dim(),
                );
            }
        }
        if button(ui, card.right() - pad - 112.0, by, "Not Now", Style::Plain, true).0 {
            close = true;
        }
    });
    if retry {
        p.fetch_index();
    }
    if add && let Some(e) = entry {
        p.start_add(&e);
        p.offer = false;
    }
    if close {
        p.offer = false;
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// "Add <plugin>?" with what it asks for.
fn confirm_card(p: &mut Plugins, ui: &mut Ui) {
    let Some(c) = &p.confirm else { return };
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let m = c.manifest.clone();
    let from_file = matches!(c.source, Source::File(_));
    let lines = m.permission_lines();
    let mut answer = None;
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        answer = Some(false);
    }
    // Hosts read better as a list under the line than as one long sentence.
    let split: Vec<(String, String)> = lines
        .iter()
        .map(|l| match l.strip_prefix("Talk to ").filter(|rest| rest.contains('.') && !rest.contains(" on ")) {
            Some(hosts) => ("Talk to these services".to_string(), hosts.to_string()),
            None => (l.clone(), String::new()),
        })
        .collect();
    let rows_h: f32 = split.iter().map(|(_, d)| if d.is_empty() { 42.0 } else { 58.0 }).sum();
    let h = 250.0 + rows_h + if from_file { 52.0 } else { 0.0 };
    dim_and_card(ui, egui::Id::NULL.with("plugin-confirm"), 480.0, h, |ui, card| {
        let painter = ui.painter().clone();
        let pad = 28.0;
        let x0 = card.left() + pad;
        hero_icon(&painter, Rect::from_min_size(pos2(x0, card.top() + 28.0), vec2(48.0, 48.0)), &m.icon);
        painter.text(
            pos2(x0 + 64.0, card.top() + 42.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("Add {}?", m.name)),
            theme::strong(17.0),
            Tokens::text_strong(),
        );
        let by = if m.authors.is_empty() { String::new() } else { format!("By {} · ", m.authors.join(", ")) };
        painter.text(
            pos2(x0 + 64.0, card.top() + 63.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("{by}version {}", m.version)),
            theme::body(12.0),
            Tokens::text_dim(),
        );
        let mut y = card.top() + 100.0;
        painter.text(pos2(x0, y), Align2::LEFT_CENTER, crate::i18n::tr_s("It will be able to"), theme::strong(12.0), Tokens::text_dim());
        y += 14.0;
        let rows: Vec<(&str, &str)> = split.iter().map(|(t, d)| (t.as_str(), d.as_str())).collect();
        y += feature_rows(&painter, pos2(x0, y), card.width() - 2.0 * pad, &rows, theme::fixed(0x3d9a50)) + 14.0;
        let g = painter.layout(
            crate::i18n::tr_s(
                "Nothing else: not your files, not other plugins. You can switch it off or remove it any time in Preferences ▸ Plugins.",
            ),
            theme::body(11.5),
            Tokens::text_dim(),
            card.width() - 2.0 * pad,
        );
        let gh = g.size().y;
        painter.galley(pos2(x0, y), g, Tokens::text_dim());
        y += gh + 12.0;
        if from_file {
            let g = painter.layout(
                crate::i18n::tr_s(
                    "This one comes from a file, not ShapeCraft's reviewed list. Only add it if you trust where it came from.",
                ),
                theme::body(11.5),
                tone_color(Tone::Warning),
                card.width() - 2.0 * pad,
            );
            painter.galley(pos2(x0, y), g, tone_color(Tone::Warning));
        }
        let by = card.bottom() - 34.0;
        painter.hline(card.x_range(), by - 26.0, Stroke::new(1.0, Tokens::divider()));
        if button(ui, card.right() - pad + 8.0, by, "Add Plugin", Style::Primary, true).0 {
            answer = Some(true);
        }
        if button(ui, card.right() - pad - 108.0, by, "Cancel", Style::Plain, true).0 {
            answer = Some(false);
        }
    });
    match answer {
        Some(true) => p.accept(now),
        Some(false) => p.confirm = None,
        None => {}
    }
}

/// "Allow <service> to charge your account?": an assistant asked to press a paid button. Never answered by a key
/// press except Esc (Don't Allow), so a stray Return can't spend money.
fn approval_card(p: &mut Plugins, ui: &mut Ui) {
    let Some(a) = p.approvals.iter().find(|a| matches!(a.state, Asked::Waiting)) else { return };
    let ctx = ui.ctx().clone();
    let (spend, plugin_name) = (a.spend.clone(), a.plugin_name.clone());
    let more = p.approvals.iter().filter(|a| matches!(a.state, Asked::Waiting)).count() - 1;
    let mut answer = None;
    let w = 460.0;
    let pad = 28.0;
    let tw = w - 2.0 * pad;
    let who = if spend.with.is_empty() { "the service".to_string() } else { spend.with.clone() };
    let body = format!(
        "An assistant connected to ShapeCraft wants {plugin_name} to make {}. Your view and description go to {who}, which \
         charges your account for it{}.",
        if spend.what.is_empty() { "something".to_string() } else { spend.what.clone() },
        if spend.estimate.is_empty() { String::new() } else { format!(" ({})", spend.estimate) },
    );
    let measure = |text: &str, font: egui::FontId| ctx.fonts_mut(|f| f.layout(crate::i18n::tr_s(text), font, Color32::WHITE, tw).size().y);
    let body_h = measure(&body, theme::body(13.0));
    let detail_h = if spend.detail.is_empty() { 0.0 } else { measure(&spend.detail, theme::body(12.0)) + 22.0 };
    let h = 132.0 + body_h + detail_h + if more > 0 { 22.0 } else { 0.0 };
    dim_and_card(ui, egui::Id::NULL.with("plugin-approval"), w, h, |ui, card| {
        let painter = ui.painter().clone();
        let x0 = card.left() + pad;
        let mut y = card.top() + 34.0;
        painter.text(
            pos2(x0, y),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s(format!("Allow {who} to charge your account?")),
            theme::strong(17.0),
            Tokens::text_strong(),
        );
        y += 24.0;
        let g = painter.layout(crate::i18n::tr_s(body), theme::body(13.0), Tokens::text(), tw);
        y += {
            let gh = g.size().y;
            painter.galley(pos2(x0, y), g, Tokens::text());
            gh
        } + 12.0;
        if !spend.detail.is_empty() {
            let g = painter.layout(crate::i18n::tr_s(format!("“{}”", spend.detail)), theme::body(12.0), Tokens::text_dim(), tw);
            let gh = g.size().y;
            painter.galley(pos2(x0, y), g, Tokens::text_dim());
            y += gh + 10.0;
        }
        if more > 0 {
            painter.text(
                pos2(x0, y + 6.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(format!("{more} more waiting after this one.")),
                theme::body(11.5),
                Tokens::text_dim(),
            );
        }
        let by = card.bottom() - 34.0;
        painter.hline(card.x_range(), by - 26.0, Stroke::new(1.0, Tokens::divider()));
        if button(ui, card.right() - pad + 8.0, by, "Allow", Style::Primary, true).0 {
            answer = Some(true);
        }
        if button(ui, card.right() - pad - 80.0, by, "Don't Allow", Style::Plain, true).0 {
            answer = Some(false);
        }
    });
    if let Some(allow) = answer {
        p.answer_approval(allow);
    }
}

/// Adding a plugin, start to finish: working (with what it's doing), then ready (with what to do next) or what went
/// wrong (with Try Again).
fn add_card(p: &mut Plugins, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    // Started without a clock (the control channel): it starts now.
    if let Some(AddCard::Working { since, .. }) = &mut p.add_card
        && *since < 0.0
    {
        *since = now;
    }
    let Some(card_state) = &p.add_card else { return };
    let mut out: Option<&'static str> = None;
    let (name, icon) = match card_state {
        AddCard::Working { name, icon, .. } | AddCard::Done { name, icon, .. } | AddCard::Failed { name, icon, .. } => {
            (name.clone(), icon.clone())
        }
    };
    if matches!(card_state, AddCard::Working { .. }) {
        ctx.request_repaint();
    } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        out = Some("close");
    }
    let state = match card_state {
        AddCard::Working { size, since, .. } => (0u8, String::new(), *size, now - *since),
        AddCard::Done { .. } => (1u8, String::new(), 0, 0.0),
        AddCard::Failed { error, .. } => (2u8, error.clone(), 0, 0.0),
    };
    let card_h = if state.0 == 0 { 250.0 } else { 270.0 };
    dim_and_card(ui, egui::Id::NULL.with("plugin-adding"), 440.0, card_h, |ui, card| {
        let painter = ui.painter().clone();
        let pad = 28.0;
        let c = card.center().x;
        let icon_r = Rect::from_center_size(pos2(c, card.top() + 62.0), vec2(56.0, 56.0));
        hero_icon(&painter, icon_r, &icon);
        let by = card.bottom() - 34.0;
        match state.0 {
            0 => {
                // Working: a bar that fills as it goes (downloads are small; the checking takes a moment).
                let t = ((state.3 / 1.6) as f32).clamp(0.0, 0.92);
                let step =
                    if t < 0.45 { format!("Downloading {}…", size_text(state.2)) } else { "Checking and installing…".to_string() };
                painter.text(
                    pos2(c, card.top() + 118.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s(format!("Adding {name}")),
                    theme::strong(17.0),
                    Tokens::text_strong(),
                );
                painter.text(
                    pos2(c, card.top() + 144.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s(step),
                    theme::body(12.5),
                    Tokens::text_dim(),
                );
                let bar = Rect::from_center_size(pos2(c, card.top() + 178.0), vec2(card.width() - 2.0 * pad - 40.0, 6.0));
                painter.rect_filled(bar, radius(3), theme::c(0x3a3a3c));
                let fill = Rect::from_min_size(bar.min, vec2(bar.width() * (0.08 + 0.92 * t), bar.height()));
                painter.rect_filled(fill, radius(3), Tokens::accent());
                painter.text(
                    pos2(c, by),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s("It runs sealed off, with only the permissions you just saw."),
                    theme::body(11.5),
                    Tokens::text_faint(),
                );
            }
            1 => {
                tick_badge(&painter, icon_r.right_bottom() - vec2(4.0, 4.0), 13.0, theme::fixed(0x3d9a50));
                painter.text(
                    pos2(c, card.top() + 118.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s(format!("{name} is ready")),
                    theme::strong(17.0),
                    Tokens::text_strong(),
                );
                centred(
                    &painter,
                    pos2(card.left() + pad, card.top() + 136.0),
                    card.width() - 2.0 * pad,
                    "Connect the AI you'd like to use, then press Generative Render in Visualization. You'll find it any time in Preferences ▸ Plugins.",
                    theme::body(12.5),
                    Tokens::text_dim(),
                );
                painter.hline(card.x_range(), by - 26.0, Stroke::new(1.0, Tokens::divider()));
                if button(ui, card.right() - pad + 8.0, by, "Set Up Connections…", Style::Primary, true).0 {
                    out = Some("setup");
                }
                if button(ui, card.right() - pad - 172.0, by, "Later", Style::Plain, true).0 {
                    out = Some("close");
                }
            }
            _ => {
                painter.text(
                    pos2(c, card.top() + 118.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr_s(format!("{name} couldn't be added")),
                    theme::strong(17.0),
                    Tokens::text_strong(),
                );
                centred(
                    &painter,
                    pos2(card.left() + pad, card.top() + 136.0),
                    card.width() - 2.0 * pad,
                    &short(&state.1, 180),
                    theme::body(12.0),
                    Tokens::danger(),
                );
                painter.hline(card.x_range(), by - 26.0, Stroke::new(1.0, Tokens::divider()));
                if button(ui, card.right() - pad + 8.0, by, "Try Again", Style::Primary, true).0 {
                    out = Some("retry");
                }
                if button(ui, card.right() - pad - 104.0, by, "Close", Style::Plain, true).0 {
                    out = Some("close");
                }
            }
        }
    });
    match out {
        Some("setup") => {
            if let Some(AddCard::Done { id, .. }) = &p.add_card {
                p.selected = Some(id.clone());
            }
            p.browsing = false;
            p.requests.push(Request::OpenPlugins);
            p.add_card = None;
        }
        Some("retry") => {
            let retry = match p.add_card.take() {
                Some(AddCard::Failed { retry, .. }) => retry,
                _ => None,
            };
            if let Some(e) = retry {
                p.start_add(&e);
            }
        }
        Some(_) => p.add_card = None,
        None => {}
    }
}

fn size_text(bytes: u64) -> String {
    if bytes >= 1 << 20 { format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64) } else { format!("{} KB", (bytes / 1024).max(1)) }
}

// ——— Control channel ———

/// `plugins.*` on the control channel (and MCP through it): list, add from a file or the list, open a sheet, read a
/// panel, set values, press a button, wait for it.
pub(crate) fn control_call(method: &str, params: &Value, capture: impl FnOnce() -> Option<Capture>) -> Result<Value, String> {
    let s = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match method {
        "plugins.list" => with(|p| {
            p.reload_list();
            Ok(json!(
                p.installed
                    .iter()
                    .map(|i| json!({
                        "id": i.manifest.id, "name": i.manifest.name, "version": i.manifest.version,
                        "enabled": i.enabled, "sideloaded": i.sideloaded,
                        "permissions": i.manifest.permission_lines(),
                    }))
                    .collect::<Vec<_>>()
            ))
        }),
        "plugins.install" => with(|p| {
            let pkg = shapecraft_plugin_host::library::read_package(std::path::Path::new(&s("path"))).map_err(|e| e.to_string())?;
            let lib = p.lib.as_ref().ok_or("no plugins folder")?;
            let i = lib.install(&pkg, params.get("sideloaded").and_then(Value::as_bool).unwrap_or(true)).map_err(|e| e.to_string())?;
            let id = i.manifest.id.clone();
            p.added(i);
            Ok(json!({ "id": id }))
        }),
        "plugins.remove" => with(|p| {
            let id = s("id");
            if let Some(l) = p.loaded.remove(&id) {
                l.cancel();
            }
            p.lib.as_ref().ok_or("no plugins folder")?.remove(&id).map_err(|e| e.to_string())?;
            p.panels.retain(|(pid, _), _| *pid != id);
            p.reload_list();
            Ok(Value::Null)
        }),
        "plugins.panel" => with(|p| {
            let (id, panel) = (s("id"), if s("panel").is_empty() { "main".into() } else { s("panel") });
            if let Some(Value::Object(vals)) = params.get("values") {
                for (k, v) in vals {
                    p.set_value(&id, &panel, k, v.clone());
                }
            }
            let values = Value::Object(p.state(&id, &panel).values.clone());
            let loaded = p.ensure(&id).ok_or("the plugin isn't added or is off")?;
            let text = loaded.panel(&panel, &values).wait().map_err(|e| e.to_string())?;
            Ok(json!({ "panel": serde_json::from_str::<Value>(&text).unwrap_or(Value::Null), "values": values }))
        }),
        "plugins.secret" => with(|p| {
            let dir = p.installed(&s("id")).map(|i| i.dir.clone()).ok_or("the plugin isn't added")?;
            let v = params.get("value").and_then(Value::as_str);
            KeyFile::secrets(&dir).set(&s("name"), v)?;
            Ok(Value::Null)
        }),
        "plugins.run" => {
            // The panel says whether the button looks at the view and whether it costs money. A paid press waits for
            // the person's Allow in the app: the answer is `{"needs_approval": {"request": n, …}}`, then
            // `plugins.approval {request}` tells how it went.
            let (id, panel) = (s("id"), if s("panel").is_empty() { "main".to_string() } else { s("panel") });
            let action = s("action");
            let button = with(|p| -> Result<Option<(bool, Option<Spend>)>, String> {
                let values = Value::Object(p.state(&id, &panel).values.clone());
                let loaded = p.ensure(&id).ok_or("the plugin isn't added or is off")?;
                let text = loaded.panel(&panel, &values).wait().map_err(|e| e.to_string())?;
                Ok(Panel::parse(&text)?.button_for(&action))
            })?;
            let (wants_view, spend) = button.unwrap_or((false, None));
            let wants_view = params.get("capture").and_then(Value::as_bool).unwrap_or(wants_view);
            let cap = if wants_view { capture() } else { None };
            if let Some(spend) = spend {
                return with(|p| {
                    let values = Value::Object(p.state(&id, &panel).values.clone());
                    let plugin_name = p.installed(&id).map(|i| i.manifest.name.clone()).unwrap_or_else(|| id.clone());
                    let request = p.next_request;
                    p.next_request += 1;
                    let answer = json!({ "needs_approval": {
                        "request": request, "with": spend.with, "what": spend.what, "estimate": spend.estimate,
                        "note": "ShapeCraft is asking the person to Allow or Don't Allow this on screen. Poll plugins.approval." } });
                    if p.approvals.len() >= 32 {
                        p.approvals.retain(|a| matches!(a.state, Asked::Waiting | Asked::Running { .. }));
                    }
                    if p.approvals.iter().filter(|a| matches!(a.state, Asked::Waiting)).count() >= 4 {
                        return Err("Four requests are already waiting for the person's answer in ShapeCraft.".into());
                    }
                    p.approvals.push(Approval {
                        request,
                        plugin: id.clone(),
                        plugin_name,
                        panel: panel.clone(),
                        action: action.clone(),
                        values,
                        capture: cap,
                        spend,
                        asked: -1.0,
                        state: Asked::Waiting,
                    });
                    Ok(answer)
                });
            }
            with(|p| {
                let values = Value::Object(p.state(&id, &panel).values.clone());
                let loaded = p.ensure(&id).ok_or("the plugin isn't added or is off")?;
                let t = loaded.run(&action, &values, cap);
                if params.get("wait").and_then(Value::as_bool).unwrap_or(true) {
                    let out = t.wait().map_err(|e| e.to_string())?;
                    let new = loaded.activity.take_results();
                    let paths: Vec<String> = new.iter().map(|e| e.path.to_string_lossy().into_owned()).collect();
                    if !new.is_empty() {
                        let list = p.results_of(&id);
                        for r in new.iter().rev() {
                            list.insert(0, r.clone());
                        }
                    }
                    Ok(json!({ "answer": serde_json::from_str::<Value>(&out).unwrap_or(Value::Null), "results": paths }))
                } else {
                    p.state(&id, &panel).run = Some((action.clone(), t));
                    Ok(Value::Null)
                }
            })
        }
        // How a paid request is going: waiting (for the person), denied, running (with progress), done (answer +
        // pictures). `withdraw: true` takes back one still waiting.
        "plugins.approval" => with(|p| {
            let n = params.get("request").and_then(Value::as_u64).ok_or("'request' is required")?;
            let withdraw = params.get("withdraw").and_then(Value::as_bool).unwrap_or(false);
            let i = p.approvals.iter().position(|a| a.request == n).ok_or("no such request")?;
            if withdraw && matches!(p.approvals[i].state, Asked::Waiting) {
                p.approvals[i].state = Asked::Denied("Withdrawn before the person answered. Nothing was made or spent.".into());
            }
            let plugin = p.approvals[i].plugin.clone();
            let progress = p.loaded.get(&plugin).map(|l| l.activity.progress()).unwrap_or_default();
            let a = &p.approvals[i];
            Ok(match &a.state {
                Asked::Waiting => json!({ "state": "waiting" }),
                Asked::Denied(why) => json!({ "state": "denied", "reason": why }),
                Asked::Running { .. } => json!({ "state": "running", "progress": progress.0, "message": progress.1 }),
                Asked::Done(Ok(answer), results) => json!({ "state": "done", "answer": answer, "results": results }),
                Asked::Done(Err(e), results) => json!({ "state": "failed", "error": e, "results": results }),
            })
        }),
        "plugins.select" => with(|p| {
            p.selected = Some(s("id")).filter(|i| !i.is_empty());
            p.browsing = false;
            Ok(Value::Null)
        }),
        // The open sheet's state, to look at it offscreen: `show` (a History index), `history`, `editing`.
        "plugins.sheet" => with(|p| {
            let Some(plugin) = p.sheet.as_ref().map(|s| s.plugin.clone()) else { return Err("no sheet open".into()) };
            let entry = params.get("show").and_then(Value::as_u64).and_then(|i| p.results_of(&plugin).get(i as usize).cloned());
            let s = p.sheet.as_mut().ok_or("no sheet open")?;
            if let Some(e) = entry {
                s.shown = Some(e);
                s.editing = false;
            }
            if let Some(h) = params.get("history").and_then(Value::as_bool) {
                s.history = h;
            }
            if let Some(e) = params.get("editing").and_then(Value::as_bool) {
                s.editing = e;
            }
            Ok(Value::Null)
        }),
        // Presses the confirmation card's Add Plugin (to look at the add cards offscreen).
        "plugins.accept" => with(|p| {
            p.accept(-1.0);
            Ok(Value::Null)
        }),
        "plugins.add" => with(|p| {
            let e =
                p.index.as_ref().and_then(|r| r.as_ref().ok()).and_then(|i| i.plugins.iter().find(|e| e.manifest.id == s("id")).cloned());
            let e = e.ok_or("not in the list (plugins.browse first)")?;
            p.start_add(&e);
            Ok(Value::Null)
        }),
        "plugins.close" => with(|p| {
            p.sheet = None;
            p.offer = false;
            p.confirm = None;
            p.add_card = None;
            Ok(Value::Null)
        }),
        "plugins.browse" => with(|p| {
            if params.get("refresh").and_then(Value::as_bool).unwrap_or(false) || p.index.is_none() && p.browse.is_none() {
                p.fetch_index();
            }
            p.browsing = true;
            Ok(Value::Null)
        }),
        "plugins.offer" => with(|p| {
            p.offer = true;
            if p.index.is_none() && p.browse.is_none() {
                p.fetch_index();
            }
            Ok(Value::Null)
        }),
        _ => Err(format!("unknown method {method}")),
    }
}
