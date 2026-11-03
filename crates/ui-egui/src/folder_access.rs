//! The first touch of the projects folder, off the UI thread.
//!
//! On macOS the projects folder is in Documents, which is privacy-protected: the first read by a copy of the app
//! macOS hasn't seen before (every update of an unsigned app counts as one) waits until the person answers
//! "would like to access files in your Documents folder". Done on the UI thread that wait froze the launch with no
//! window at all. So the window opens first, the home screen says what it's waiting for, and nothing on the UI
//! thread reads or writes the folder until this check has finished.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// After this long, the home screen adds the hint about macOS asking for access.
pub const HINT_AFTER: Duration = Duration::from_millis(1500);

#[derive(Clone, Debug, PartialEq)]
pub enum Access {
    /// Still checking; since when.
    Pending(Instant),
    /// The folder is there and readable (it may differ from the one asked for: the old Shayper3D folder in place).
    Ready(PathBuf),
    /// It can't be read or made: why.
    Denied(String),
}

pub struct FolderAccess {
    result: Arc<Mutex<Option<Result<PathBuf, String>>>>,
    started: Instant,
}

impl FolderAccess {
    /// Starts the check: moves the old folder (`legacy`) to `root` once if `root` isn't there yet, makes `root`,
    /// and reads it once. `wake` runs when the answer is in (the app asks for a repaint).
    pub fn start(root: PathBuf, legacy: Option<PathBuf>, wake: impl Fn() + Send + 'static) -> Self {
        let result = Arc::new(Mutex::new(None));
        let out = result.clone();
        std::thread::Builder::new()
            .name("projects-folder".into())
            .spawn(move || {
                let answer = settle(root, legacy);
                if let Ok(mut slot) = out.lock() {
                    *slot = Some(answer);
                }
                wake();
            })
            .ok();
        Self { result, started: Instant::now() }
    }

    /// Already settled (snapshots and tests, which point at a scratch folder).
    pub fn ready(root: PathBuf) -> Self {
        Self { result: Arc::new(Mutex::new(Some(Ok(root)))), started: Instant::now() }
    }

    pub fn state(&self) -> Access {
        match self.result.lock().ok().and_then(|r| r.clone()) {
            None => Access::Pending(self.started),
            Some(Ok(root)) => Access::Ready(root),
            Some(Err(e)) => Access::Denied(e),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state(), Access::Ready(_))
    }
}

fn settle(root: PathBuf, legacy: Option<PathBuf>) -> Result<PathBuf, String> {
    let mut root = root;
    // Move the old projects folder once; if it can't be moved, keep using it where it is.
    if let Some(old) = legacy
        && !root.exists()
        && old.is_dir()
        && std::fs::rename(&old, &root).is_err()
    {
        root = old;
    }
    std::fs::create_dir_all(&root).map_err(|e| describe(&root, &e))?;
    std::fs::read_dir(&root).map_err(|e| describe(&root, &e))?;
    Ok(root)
}

fn describe(root: &std::path::Path, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!("ShapeCraft isn't allowed to use {}", root.display())
    } else {
        format!("Couldn't open {}: {e}", root.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(a: &FolderAccess) -> Access {
        let t = Instant::now();
        while matches!(a.state(), Access::Pending(_)) && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(5));
        }
        a.state()
    }

    #[test]
    fn makes_the_folder_moves_the_old_one_and_reports_failures() {
        let tmp = std::env::temp_dir().join(format!("sc-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // Made when missing.
        let root = tmp.join("ShapeCraft");
        assert_eq!(wait(&FolderAccess::start(root.clone(), None, || {})), Access::Ready(root.clone()));
        assert!(root.is_dir());
        // The old folder moves over when the new one isn't there yet.
        let (root2, old) = (tmp.join("New"), tmp.join("Old"));
        std::fs::create_dir_all(old.join("a project")).unwrap();
        assert_eq!(wait(&FolderAccess::start(root2.clone(), Some(old.clone()), || {})), Access::Ready(root2.clone()));
        assert!(root2.join("a project").is_dir() && !old.exists());
        // A file where the folder should be: denied with a reason, not a hang or a panic.
        let file = tmp.join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(matches!(wait(&FolderAccess::start(file.join("inside"), None, || {})), Access::Denied(_)));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
