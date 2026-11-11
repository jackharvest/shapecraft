//! ShapeCraft for Android: the same app as the desktop, in a `NativeActivity`. Projects and preferences live in the
//! app's own storage; files come and go through the system's picker (a little Java beside the activity, reached
//! through a mailbox of files, see `Mailbox`). Built and packed into an APK by `cargo xtask android-apk`.

#[cfg(target_os = "android")]
mod android {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use shapecraft_kernel_occt::OcctKernel;
    use shapecraft_ui::settings::Settings;
    use shapecraft_ui::tablet::{NativeTouch, Pointer, PointerPhase, PointerTool};
    use shapecraft_ui::{Options, ShapeCraftApp, TabletRequest, crash_log, graphics};
    use winit::platform::android::activity::AndroidApp;
    use winit::platform::android::pointers::{self, Phase, Record, Tool};

    /// The entry point `android-activity` calls on its own thread once the activity is created.
    // The NDK's glue looks the function up by this exact name.
    #[unsafe(no_mangle)]
    fn android_main(app: AndroidApp) {
        android_logger::init_once(android_logger::Config::default().with_tag("ShapeCraft").with_max_level(log::LevelFilter::Info));
        // stderr goes nowhere on Android, so panics are logged too (crash_log has no config folder here).
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("panic: {info}");
            previous(info);
        }));
        crash_log::install();
        let data = app.internal_data_path().unwrap_or_else(std::env::temp_dir);
        // Exports land in Android/data/<package>/files, which a computer can read over USB.
        shapecraft_ui::dialogs::set_android_folder(app.external_data_path().unwrap_or_else(|| data.join("Exports")));
        shapecraft_ui::tablet::set_touch_screen(true);
        // Phones (under 600 dp across) keep their view clear, the same line the manifest's orientation uses.
        shapecraft_ui::tablet::set_small_screen(app.config().smallest_screen_width_dp().is_some_and(|w| w < 600));
        // Android has no Documents folder for the app: projects go in its own storage (…/files/ShapeCraft).
        Options::set_documents(data.clone());
        // Saves waiting for the picker in a run that ended are gone with it.
        let _ = std::fs::remove_dir_all(data.join(".outbox"));
        let mailbox = Mailbox::new(&data);
        let opts = Options {
            projects_root: Options::default_root(),
            control_port: None,
            open: None,
            new_project: false,
            native_menu: false,
            settings_file: Some(data.join("settings.json")),
            hosted: false,
        };
        let saved = opts.settings_file.as_deref().map(Settings::load).unwrap_or_default();
        // One try with every path in it (Vulkan, then GLES): Android can't make a second window to fall back with.
        let pass = graphics::single_pass(&saved.graphics);
        let android = app.clone();
        let native = eframe::NativeOptions {
            android_app: Some(app),
            wgpu_options: graphics::configuration(&saved.graphics, &saved.gpu, &pass),
            ..Default::default()
        };
        let result = eframe::run_native(
            "ShapeCraft",
            native,
            Box::new(move |cc| {
                // The touch model makes long presses itself (a right-click), so a tap is a click however long its
                // frames take: a slow GPU, or a heavy model, must not turn taps into nothing.
                cc.egui_ctx.options_mut(|o| o.input_options.max_click_duration = 10.0);
                let inner = ShapeCraftApp::new(Arc::new(OcctKernel::new()), opts, &cc.egui_ctx);
                Ok(Box::new(Phone { inner, touch: NativeTouch::default(), android, keyboard: false, pen_button: 0, mailbox }))
            }),
        );
        match result {
            Ok(()) => log::info!("ShapeCraft closed"),
            Err(e) => graphics::report_failure(&e.to_string()),
        }
    }

    /// The desktop app with what a touch screen needs around it: fingers through the tablet's touch model, the
    /// interface scaled to fit, and the on-screen keyboard while a text field has the focus.
    struct Phone {
        inner: ShapeCraftApp,
        touch: NativeTouch,
        android: AndroidApp,
        keyboard: bool,
        pen_button: u8,
        mailbox: Mailbox,
    }

    /// The way to the system's file picker (#7). The app writes a request file into files/.mailbox/to-android;
    /// ShapeCraftActivity (packaging/android/java) shows the picker and answers in files/.mailbox/to-app. Files
    /// rather than calls into Java keep this side free of JNI.
    struct Mailbox {
        to_android: PathBuf,
        to_app: PathBuf,
        seq: u64,
        /// Requests still waiting for their answer: the app keeps looking for it.
        waiting: usize,
    }

    /// An answer from the picker.
    enum Answer {
        Picked(u64, PathBuf),
        Saved(String),
        Failed(String),
        Cancelled,
    }

    impl Mailbox {
        fn new(files: &Path) -> Self {
            let (to_android, to_app) = (files.join(".mailbox/to-android"), files.join(".mailbox/to-app"));
            // What an earlier run asked or was told is stale now.
            for d in [&to_android, &to_app] {
                let _ = std::fs::remove_dir_all(d);
                let _ = std::fs::create_dir_all(d);
            }
            Self { to_android, to_app, seq: 0, waiting: 0 }
        }

        /// One request, written whole before the activity can see it (a temporary name, then renamed).
        fn send(&mut self, lines: &[&str]) {
            self.seq += 1;
            let stem = format!("{:020}", self.seq);
            let tmp = self.to_android.join(format!("{stem}.tmp"));
            let ok = std::fs::write(&tmp, lines.join("\n")).is_ok()
                && std::fs::rename(&tmp, self.to_android.join(format!("{stem}.req"))).is_ok();
            if ok {
                self.waiting += 1;
            } else {
                log::error!("couldn't ask for the file picker in {}", self.to_android.display());
            }
        }

        /// The answers that have come in, oldest first.
        fn receive(&mut self) -> Vec<Answer> {
            let Ok(dir) = std::fs::read_dir(&self.to_app) else { return Vec::new() };
            let mut files: Vec<PathBuf> = dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "res")).collect();
            files.sort();
            let mut out = Vec::new();
            for f in files {
                let text = std::fs::read_to_string(&f).unwrap_or_default();
                let _ = std::fs::remove_file(&f);
                self.waiting = self.waiting.saturating_sub(1);
                let lines: Vec<&str> = text.split('\n').collect();
                out.push(match lines.as_slice() {
                    ["picked", id, path, ..] => match id.parse() {
                        Ok(id) => Answer::Picked(id, PathBuf::from(path)),
                        Err(_) => Answer::Cancelled,
                    },
                    ["saved", name, ..] => Answer::Saved((*name).to_string()),
                    ["failed", why, ..] => Answer::Failed((*why).to_string()),
                    _ => Answer::Cancelled,
                });
            }
            out
        }

        /// What the app asked of the file picker this frame: a file to open, or files it wrote to save somewhere.
        fn ask(&mut self, requests: Vec<TabletRequest>) {
            for r in requests {
                match r {
                    TabletRequest::Pick { id, label, .. } => self.send(&["open", &id.to_string(), &label]),
                    TabletRequest::Saved(dir) => {
                        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                            .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
                            .unwrap_or_default();
                        files.sort();
                        for path in files {
                            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            self.send(&["save", &path.to_string_lossy(), &name]);
                        }
                    }
                }
            }
        }
    }

    impl eframe::App for Phone {
        fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
            let ppp = ctx.pixels_per_point();
            let mut list: Vec<Pointer> = pointers::take().into_iter().filter_map(|r| pointer(r, ppp)).collect();
            // A pen lifts with the button it pressed with, even if the barrel button was let go first.
            for p in list.iter_mut().filter(|p| p.tool == PointerTool::Pen) {
                match p.phase {
                    PointerPhase::Down => self.pen_button = p.button,
                    PointerPhase::Up | PointerPhase::Cancel => p.button = self.pen_button,
                    _ => {}
                }
            }
            let pointers = list;
            self.touch.input(raw, Some(pointers), self.inner.workspace());
        }

        fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            // Shortcuts show while a hardware keyboard is attached.
            let qwerty = self.android.config().keyboard() == winit::platform::android::activity::ndk::configuration::Keyboard::Qwerty;
            shapecraft_ui::tablet::set_keyboard(qwerty);
            if let Some(zoom) = NativeTouch::fit_zoom(ctx)
                && (zoom - ctx.zoom_factor()).abs() > 1e-3
            {
                ctx.set_zoom_factor(zoom);
            }
            for answer in self.mailbox.receive() {
                match answer {
                    Answer::Picked(id, path) => self.inner.tablet_file_arrived(id, path),
                    Answer::Saved(name) => self.inner.toast(format!("Saved {name}")),
                    Answer::Failed(why) => self.inner.toast(why),
                    Answer::Cancelled => {}
                }
            }
            if self.mailbox.waiting > 0 {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            self.inner.logic(ctx, frame);
        }

        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.inner.ui(ui, frame);
            // Files written this frame are on disk now: hand them (and any pick) to the picker.
            let requests = self.inner.take_tablet_requests();
            self.mailbox.ask(requests);
            let over_view = self.inner.workspace().is_some_and(|ws| ws.viewport_hot);
            self.touch.after_frame(ui.ctx(), over_view);
            let wants = ui.ctx().egui_wants_keyboard_input();
            if wants != self.keyboard {
                self.keyboard = wants;
                if wants {
                    self.android.show_soft_input(true);
                } else {
                    self.android.hide_soft_input(false);
                }
            }
        }

        fn on_exit(&mut self) {
            self.inner.on_exit();
        }
    }

    /// One of winit's records as the touch model's pointer, in points. A mouse's buttons arrive as their own
    /// press/release records (its Down/Up repeat them); a pen's barrel button held makes its touch a right-click.
    fn pointer(r: Record, ppp: f32) -> Option<Pointer> {
        let pos = egui::pos2(r.x / ppp, r.y / ppp);
        let tool = match r.tool {
            Tool::Stylus | Tool::Eraser => PointerTool::Pen,
            Tool::Mouse => PointerTool::Mouse,
            Tool::Finger | Tool::Unknown => PointerTool::Finger,
        };
        let mouse = tool == PointerTool::Mouse;
        let bits = |b: u32| -> u8 {
            if b & (pointers::SECONDARY | pointers::STYLUS_PRIMARY) != 0 {
                2
            } else if b & (pointers::TERTIARY | pointers::STYLUS_SECONDARY) != 0 {
                1
            } else {
                0
            }
        };
        let (phase, button) = match r.phase {
            Phase::Down | Phase::Up if mouse => return None,
            Phase::Down => (PointerPhase::Down, bits(r.buttons)),
            Phase::Up => (PointerPhase::Up, bits(r.buttons)),
            Phase::Move => (PointerPhase::Move, 0),
            Phase::Cancel => (PointerPhase::Cancel, 0),
            Phase::Hover if tool == PointerTool::Finger => return None,
            Phase::Hover => (PointerPhase::Hover, 0),
            Phase::HoverExit => (PointerPhase::Leave, 0),
            Phase::ButtonPress if mouse => (PointerPhase::Down, bits(r.button)),
            Phase::ButtonRelease if mouse => (PointerPhase::Up, bits(r.button)),
            Phase::ButtonPress | Phase::ButtonRelease => return None,
            Phase::Scroll => (PointerPhase::Scroll(egui::vec2(r.scroll.0, r.scroll.1)), 0),
        };
        Some(Pointer { tool, id: r.id as i64, phase, pos, button })
    }
}
