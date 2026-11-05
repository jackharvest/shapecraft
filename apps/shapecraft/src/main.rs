//! ShapeCraft desktop app.
//!
//! Flags: `--control PORT` (JSON-lines control channel on 127.0.0.1), `--open DIR`, `--new`,
//! `--projects DIR` (where projects live; default ~/Documents/ShapeCraft, or $SHAPECRAFT_PROJECTS),
//! `--graphics auto|vulkan|metal|dx12|cpu|opengl` (this run only, over Preferences ▸ ShapeCraft ▸ Graphics),
//! `--serve [ADDR:]PORT` (no window: serve ShapeCraft to browsers on the network, for a server, VM or container; the
//! pairing key is $SHAPECRAFT_KEY, else the one in Preferences ▸ Tablet, else a new one, printed with the address).

// No console window beside the app on Windows; debug builds keep it for their logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use shapecraft_kernel_occt::OcctKernel;
use shapecraft_ui::settings::Settings;
use shapecraft_ui::{Options, ShapeCraftApp, crash_log, graphics, system_theme, window_chrome};

fn main() -> anyhow::Result<()> {
    crash_log::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let opts = Options {
        projects_root: value("--projects").map(PathBuf::from).unwrap_or_else(Options::default_root),
        control_port: value("--control").and_then(|p| p.parse().ok()),
        open: value("--open").map(PathBuf::from),
        new_project: args.iter().any(|a| a == "--new"),
        native_menu: true,
        settings_file: Settings::default_path(),
        hosted: false,
    };
    let saved = opts.settings_file.as_deref().map(Settings::load).unwrap_or_default();
    if let Some(i) = args.iter().position(|a| a == "--serve") {
        let spec = args.get(i + 1).filter(|a| !a.starts_with("--")).cloned().unwrap_or_default();
        return serve(&spec, opts, &saved);
    }
    let choice = value("--graphics").and_then(|g| graphics::parse(&g).map(str::to_string)).unwrap_or(saved.graphics);
    let viewport = egui::ViewportBuilder::default()
        .with_title("ShapeCraft")
        .with_inner_size([1600.0, 1000.0])
        .with_min_inner_size([1000.0, 640.0])
        // Our own title bar, drawn under the window buttons, like the reference app.
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        // Linux desktops match a window to its launcher and icon by this.
        .with_app_id("shapecraft")
        .with_decorations(!window_chrome::OWN_FRAME);
    let viewport = match icon() {
        Some(icon) => viewport.with_icon(icon),
        None => viewport,
    };
    let run = |pass: &[&'static str], opts: Options| {
        let wgpu_options = graphics::configuration(&choice, &saved.gpu, pass);
        let native = eframe::NativeOptions { viewport: viewport.clone(), wgpu_options, ..Default::default() };
        eframe::run_native(
            "ShapeCraft",
            native,
            Box::new(move |cc| {
                system_theme::watch(&cc.egui_ctx);
                Ok(Box::new(ShapeCraftApp::new(Arc::new(OcctKernel::new()), opts, &cc.egui_ctx)))
            }),
        )
    };
    // A path that can't start must not lock anyone out of the app that could change it.
    let mut first_failure = None;
    for pass in graphics::passes(&choice) {
        match run(&pass, opts.clone()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Without a window system no pass can run, and a second try would only report that it was the second.
                let no_windows = matches!(e, eframe::Error::WinitEventLoop(_));
                first_failure.get_or_insert(e);
                if no_windows {
                    break;
                }
            }
        }
    }
    let reason = first_failure.map_or_else(|| "no graphics path to try".to_string(), |e| e.to_string());
    graphics::report_failure(&reason);
    Err(anyhow::anyhow!(reason))
}

/// Runs only the tablet host, with no window, until the process is stopped.
fn serve(spec: &str, opts: Options, saved: &Settings) -> anyhow::Result<()> {
    use shapecraft_ui::tablet;
    let port = if saved.tablet_port == 0 { tablet::DEFAULT_PORT } else { saved.tablet_port };
    let addr: std::net::SocketAddr = if spec.is_empty() {
        ([0, 0, 0, 0], port).into()
    } else if let Ok(p) = spec.parse::<u16>() {
        ([0, 0, 0, 0], p).into()
    } else {
        spec.parse().map_err(|_| anyhow::anyhow!("--serve takes a port (8780) or an address and port (0.0.0.0:8780)"))?
    };
    let key = std::env::var("SHAPECRAFT_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .or_else(|| (!saved.tablet_key.is_empty()).then(|| saved.tablet_key.clone()))
        .unwrap_or_else(tablet::new_token);
    let host = tablet::Host::start(addr, key, Arc::new(OcctKernel::new()), opts)?;
    println!("ShapeCraft is serving on port {}. Open this in a browser on the same network:", host.port);
    for url in tablet::status().map(|s| s.urls).unwrap_or_default() {
        println!("  {url}");
    }
    host.wait();
    Ok(())
}

/// The app icon (assets/icon). macOS gets the variant drawn on Apple's icon grid, so it sits right in the Dock.
fn icon() -> Option<egui::IconData> {
    #[cfg(target_os = "macos")]
    let bytes: &[u8] = include_bytes!("../../../assets/icon/png/shapecraft-mac-512.png");
    #[cfg(not(target_os = "macos"))]
    let bytes: &[u8] = include_bytes!("../../../assets/icon/png/shapecraft-256.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    rgba.truncate(info.buffer_size());
    Some(egui::IconData { rgba, width: info.width, height: info.height })
}
