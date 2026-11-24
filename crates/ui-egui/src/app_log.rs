//! ShapeCraft's own log: what it says while it runs, kept in `shapecraft.log` next to the settings (and `crash.log`)
//! so nobody has to start it from a terminal to see it, and shown by Help ▸ Show Log. ShapeCraft's own messages are
//! kept from Info up; the libraries under it (graphics, windowing) only from Warn, or the file fills with their chatter.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Past this the log moves to `shapecraft.old.log` and starts over, so two runs' worth stay around.
const MAX_BYTES: u64 = 2 << 20;
/// Lines kept for the Show Log window.
const KEEP_LINES: usize = 4000;

/// `shapecraft.log`, next to the settings file.
pub fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ShapeCraft").join("shapecraft.log"))
}

struct Inner {
    lines: VecDeque<String>,
    file: Option<std::fs::File>,
    written: u64,
    /// The last message and how many times it came again since, so a repeating warning is one line.
    last: String,
    repeats: u32,
}

struct Logger {
    inner: Mutex<Inner>,
    echo: bool,
    /// The file it writes, if any.
    file: Option<PathBuf>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// ShapeCraft's own crates (and its apps) log from Info; everything else from Warn.
fn wanted(target: &str, level: log::Level) -> bool {
    if target.starts_with("shapecraft") { level <= log::Level::Info } else { level <= log::Level::Warn }
}

fn clock() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// The crate a target belongs to, without its module path: `shapecraft_engine::cmd::model` → `engine`.
fn short_target(target: &str) -> &str {
    let krate = target.split("::").next().unwrap_or(target);
    krate.strip_prefix("shapecraft_").unwrap_or(krate)
}

impl Inner {
    fn push(&mut self, line: String) {
        if let Some(file) = self.file.as_mut() {
            let bytes = line.len() as u64 + 1;
            if file.write_all(line.as_bytes()).and_then(|()| file.write_all(b"\n")).is_ok() {
                self.written += bytes;
            }
        }
        if self.lines.len() >= KEEP_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn flush_repeats(&mut self) {
        if self.repeats > 0 {
            let n = std::mem::take(&mut self.repeats);
            self.push(format!("{} (the line above came {n} more time{})", clock(), if n == 1 { "" } else { "s" }));
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        wanted(metadata.target(), metadata.level())
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        if self.echo && record.level() <= log::Level::Warn {
            eprintln!("{message}");
        }
        let Ok(mut inner) = self.inner.lock() else { return };
        if message == inner.last {
            inner.repeats += 1;
            return;
        }
        inner.flush_repeats();
        let level = match record.level() {
            log::Level::Error => "error",
            log::Level::Warn => "warning",
            _ => "",
        };
        let target = short_target(record.target());
        let line =
            if level.is_empty() { format!("{} {target}: {message}", clock()) } else { format!("{} {target} {level}: {message}", clock()) };
        inner.push(line);
        inner.last = message;
        if inner.written > MAX_BYTES
            && let Some(p) = self.file.as_deref()
        {
            inner.file = None;
            let _ = std::fs::rename(p, p.with_file_name("shapecraft.old.log"));
            inner.file = std::fs::File::create(p).ok();
            inner.written = 0;
        }
    }

    fn flush(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.flush_repeats();
            if let Some(f) = inner.file.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

/// Starts the log for this run (once; a second call, or another logger already in place, changes nothing).
pub fn install() {
    install_at(path(), true);
}

/// The log without a file, for offscreen runs (snapshots, the benchmark) that mustn't touch the real one.
pub fn install_in_memory() {
    install_at(None, false);
}

fn install_at(file: Option<PathBuf>, echo: bool) {
    let at = file.clone();
    let file = file.and_then(|p| {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Each run starts its own file; the run before stays in shapecraft.old.log.
        if p.exists() {
            let _ = std::fs::rename(&p, p.with_file_name("shapecraft.old.log"));
        }
        std::fs::File::create(&p).ok()
    });
    let file_ok = file.is_some();
    let logger = LOGGER.get_or_init(|| Logger {
        inner: Mutex::new(Inner { lines: VecDeque::new(), file, written: 0, last: String::new(), repeats: 0 }),
        echo,
        file: at.filter(|_| file_ok),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
        log::info!("ShapeCraft {} on {} {}, started", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    }
}

/// The file this run's log goes to (none offscreen, or where another logger took over).
pub fn file() -> Option<PathBuf> {
    LOGGER.get().and_then(|l| l.file.clone())
}

/// This run's lines, oldest first (empty where another logger took over, as on Android).
pub fn lines() -> Vec<String> {
    let Some(logger) = LOGGER.get() else { return Vec::new() };
    let Ok(mut inner) = logger.inner.lock() else { return Vec::new() };
    inner.flush_repeats();
    if let Some(f) = inner.file.as_mut() {
        let _ = f.flush();
    }
    inner.lines.iter().cloned().collect()
}

/// Help ▸ Show Log: this run's log in a card that scrolls, with Copy and the folder it's kept in. `open` turns false
/// when it's closed.
pub fn window(ui: &mut egui::Ui, open: &mut bool) {
    let ctx = ui.ctx().clone();
    let id = ui.scope_id().with("show-log");
    use crate::theme::{self, Tokens, radius};
    use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
    let full = ctx.content_rect();
    let size = vec2((full.width() - 32.0).min(820.0), (full.height() - 80.0).min(560.0));
    let card = Rect::from_center_size(full.center(), size);
    let lines = lines();
    let close_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let mut close = close_key;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(full.min).show(&ctx, |ui| {
        // The rest of the window takes no clicks while it's up, and a click out there closes the card.
        let behind = ui.allocate_rect(full, Sense::click());
        if behind.clicked() && !ui.input(|i| i.pointer.interact_pos().is_some_and(|p| card.contains(p))) {
            close = true;
        }
        let p = ui.painter();
        p.rect_filled(card.translate(vec2(0.0, 6.0)), radius(12), Color32::from_black_alpha(90));
        p.rect_filled(card, radius(12), Tokens::panel());
        p.rect_stroke(card, radius(12), Stroke::new(1.0, Tokens::panel_border()), StrokeKind::Inside);
        let _ = ui.interact(card, id.with("card"), Sense::click());
        p.text(
            pos2(card.left() + 18.0, card.top() + 22.0),
            Align2::LEFT_CENTER,
            crate::i18n::tr_s("Log"),
            theme::strong(15.0),
            Tokens::text_strong(),
        );
        let where_ = file().map(|p| p.display().to_string()).unwrap_or_else(|| "Kept only while ShapeCraft runs.".into());
        p.text(pos2(card.left() + 18.0, card.top() + 42.0), Align2::LEFT_CENTER, &where_, theme::body(11.5), Tokens::text_dim());
        let body = Rect::from_min_max(pos2(card.left() + 12.0, card.top() + 58.0), pos2(card.right() - 12.0, card.bottom() - 54.0));
        p.rect_filled(body, radius(8), Tokens::field());
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(8.0)));
        egui::ScrollArea::both().auto_shrink([false, false]).stick_to_bottom(true).show(&mut inner, |ui| {
            if lines.is_empty() {
                ui.label(egui::RichText::new("Nothing logged yet.").color(Tokens::text_dim()));
            }
            for line in &lines {
                let color = if line.contains(" error: ") {
                    theme::c(0xff6b6b)
                } else if line.contains(" warning: ") {
                    theme::c(0xf2c14e)
                } else {
                    Tokens::text()
                };
                ui.add(egui::Label::new(egui::RichText::new(line).monospace().size(11.5).color(color)).extend());
            }
        });
        // Buttons at the foot: Copy, Open Folder, Close.
        let button = |ui: &mut egui::Ui, right: f32, label: &str, primary: bool| {
            let g = ui.painter().layout_no_wrap(label.to_string(), theme::strong(13.0), Color32::WHITE);
            let r = Rect::from_min_max(pos2(right - g.size().x - 28.0, card.bottom() - 42.0), pos2(right, card.bottom() - 12.0));
            let resp = ui.interact(r, id.with(label), Sense::click());
            let fill = if primary { Tokens::accent() } else { theme::c(0x3a3a3e) };
            ui.painter().rect_filled(r, radius(8), if resp.hovered() { fill.gamma_multiply(1.15) } else { fill });
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, theme::strong(13.0), Color32::WHITE);
            (resp.clicked(), r.left() - 10.0)
        };
        let (done, x) = button(ui, card.right() - 12.0, "Close", true);
        let (copy, x) = button(ui, x, "Copy All", false);
        let can_open = file().is_some() && cfg!(not(any(target_os = "android", target_os = "ios")));
        let folder = can_open && button(ui, x, "Show in Folder", false).0;
        if copy {
            ctx.copy_text(lines.join("\n"));
        }
        if folder && let Some(dir) = file().as_deref().and_then(std::path::Path::parent) {
            crate::landing::reveal(dir);
        }
        close |= done;
    });
    // Over the workspace's own floating cards, whichever of them was touched last.
    ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, id));
    if close {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn keeps_our_info_and_only_others_warnings() {
        assert!(wanted("shapecraft_engine::cmd", log::Level::Info));
        assert!(!wanted("shapecraft_engine::cmd", log::Level::Debug));
        assert!(!wanted("wgpu_core::device", log::Level::Info));
        assert!(wanted("wgpu_core::device", log::Level::Warn));
        assert_eq!(short_target("shapecraft_engine::cmd::model"), "engine");
        assert_eq!(short_target("wgpu_hal::vulkan"), "wgpu_hal");
    }

    #[test]
    fn writes_the_file_and_folds_repeats() {
        let dir = std::env::temp_dir().join(format!("shapecraft-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("shapecraft.log");
        install_at(Some(file.clone()), false);
        log::info!("extrude: done in 12 ms");
        for _ in 0..3 {
            log::warn!("the same warning");
        }
        log::info!("after the warnings");
        log::debug!("too fine to keep");
        let got = lines();
        let text = std::fs::read_to_string(&file).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        for l in [&got.join("\n"), &text] {
            assert!(l.contains("ShapeCraft ") && l.contains("started"), "{l}");
            assert!(l.contains("ui: extrude: done in 12 ms"), "{l}");
            assert_eq!(l.matches("the same warning").count(), 1, "{l}");
            assert!(l.contains("came 2 more times"), "{l}");
            assert!(l.contains("after the warnings") && !l.contains("too fine"), "{l}");
        }
    }
}
