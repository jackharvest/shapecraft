//! ShapeCraft for Android: the same app as the desktop, in a `NativeActivity` (no Java code). Projects and
//! preferences live in the app's own storage. Built and packed into an APK by `cargo xtask android-apk`.

#[cfg(target_os = "android")]
mod android {
    use std::sync::Arc;

    use shapecraft_kernel_occt::OcctKernel;
    use shapecraft_ui::settings::Settings;
    use shapecraft_ui::tablet::NativeTouch;
    use shapecraft_ui::{Options, ShapeCraftApp, crash_log, graphics};
    use winit::platform::android::activity::AndroidApp;

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
        // Android has no Documents folder for the app: projects go in its own storage (…/files/ShapeCraft).
        Options::set_documents(data.clone());
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
                Ok(Box::new(Phone { inner, touch: NativeTouch::default(), android, keyboard: false }))
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
    }

    impl eframe::App for Phone {
        fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
            self.touch.input(raw, self.inner.workspace());
        }

        fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            if let Some(zoom) = NativeTouch::fit_zoom(ctx)
                && (zoom - ctx.zoom_factor()).abs() > 1e-3
            {
                ctx.set_zoom_factor(zoom);
            }
            self.inner.logic(ctx, frame);
        }

        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.inner.ui(ui, frame);
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
}
