//! A record of what went wrong, for the reports of people who hit it. A Windows release has no console, so without this a
//! crash would leave nothing behind. Commands that fail are caught and undone, and land here as well.

use std::io::Write;
use std::path::PathBuf;

/// Past this the log starts over: it's for the last few failures.
const MAX_BYTES: u64 = 1 << 20;

/// `crash.log`, next to the settings file.
pub fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("ShapeCraft").join("crash.log"))
}

fn entry(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_string());
    let place = info.location().map_or_else(|| "unknown place".to_string(), |l| format!("{}:{}", l.file(), l.line()));
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let thread = std::thread::current();
    format!(
        "--- unix time {seconds}, ShapeCraft {} on {} {}, thread {}\n{message}\nat {place}\n{}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        thread.name().unwrap_or("unnamed"),
        std::backtrace::Backtrace::force_capture(),
    )
}

/// Adds to the panic handling already in place. Nothing here may fail loudly: it runs while something else already has.
pub fn install() {
    if let Some(path) = path() {
        install_at(path);
    }
}

fn install_at(path: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
            let _ = std::fs::remove_file(&path);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(entry(info).as_bytes());
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_panic_leaves_a_record_and_a_full_log_starts_over() {
        let dir = std::env::temp_dir().join(format!("shapecraft-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("logs").join("crash.log");
        install_at(log.clone());
        let boom = |text: &'static str| {
            let _ = std::thread::Builder::new().name("boom".into()).spawn(move || std::panic::panic_any(text)).unwrap().join();
        };
        boom("first failure");
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("first failure") && text.contains("thread boom") && text.contains("crash_log.rs"), "{text}");
        std::fs::write(&log, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        boom("after the limit");
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("after the limit") && text.len() < MAX_BYTES as usize, "{}", text.len());
        let _ = std::panic::take_hook();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
