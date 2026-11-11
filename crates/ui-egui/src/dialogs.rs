//! The system's file and message dialogs. Desktops get `rfd`. Android has no dialog `rfd` can open from a native
//! activity: opening, importing and exporting go through the system's picker instead, asked for by the Android app
//! (see `TabletRequest`); the folder picks left here (Work Folder, backups) go to one shared folder
//! (`set_android_folder`, the app's folder under Android/data, reachable from a computer over USB), and messages go
//! to the system log (`adb logcat -s ShapeCraft`).

#[cfg(not(target_os = "android"))]
pub use rfd::{FileDialog, MessageDialog, MessageLevel};

#[cfg(target_os = "android")]
pub use android::{FileDialog, MessageDialog, MessageLevel, set_android_folder};

#[cfg(target_os = "android")]
mod android {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    static FOLDER: OnceLock<PathBuf> = OnceLock::new();

    /// Where saves and exports go. Set once at start; later calls are ignored.
    pub fn set_android_folder(dir: PathBuf) {
        let _ = FOLDER.set(dir);
    }

    fn folder() -> Option<PathBuf> {
        let dir = FOLDER.get()?.clone();
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    /// The same builder as `crate::dialogs::FileDialog`, for the calls ShapeCraft makes.
    #[derive(Default)]
    pub struct FileDialog {
        name: Option<String>,
    }

    impl FileDialog {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> Self {
            self
        }
        pub fn set_title(self, _title: impl Into<String>) -> Self {
            self
        }
        pub fn set_directory(self, _dir: impl AsRef<Path>) -> Self {
            self
        }
        pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
            self.name = Some(name.into());
            self
        }
        /// No picker yet: nothing is chosen.
        pub fn pick_file(self) -> Option<PathBuf> {
            None
        }
        pub fn pick_folder(self) -> Option<PathBuf> {
            folder()
        }
        /// The shared folder, under the suggested name (only its last part, so it can't point anywhere else).
        pub fn save_file(self) -> Option<PathBuf> {
            let name = self.name.as_deref().map(Path::new).and_then(Path::file_name)?.to_owned();
            Some(folder()?.join(name))
        }
    }

    pub enum MessageLevel {
        Error,
    }

    /// Messages go to the system log, the only place a failed start can still show them.
    #[derive(Default)]
    pub struct MessageDialog {
        text: String,
    }

    impl MessageDialog {
        pub fn new() -> Self {
            Self::default()
        }
        pub fn set_level(self, _level: MessageLevel) -> Self {
            self
        }
        pub fn set_title(mut self, title: impl Into<String>) -> Self {
            self.text = format!("{}: {}", title.into(), self.text);
            self
        }
        pub fn set_description(mut self, text: impl Into<String>) -> Self {
            self.text.push_str(&text.into());
            self
        }
        /// Always "seen", like a dialog with one OK button.
        pub fn show(self) -> bool {
            log::error!("{}", self.text);
            true
        }
    }
}
