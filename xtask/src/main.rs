//! ShapeCraft's build and maintenance jobs, in Rust like the app (the Craft apps' `cargo xtask` convention).
//!
//!   cargo xtask bundle-mac          ShapeCraft.app + DMG from a release build (macOS)
//!   cargo xtask bundle-linux        self-contained folder + .tar.gz (Linux; BUNDLE_CXX_RUNTIME=1 adds libstdc++)
//!   cargo xtask appimage            AppImage from bundle-linux's folder (APPIMAGETOOL=path)
//!   cargo xtask bundle-windows      portable zip + Inno Setup installer (Windows, in a VS developer shell)
//!   cargo xtask android-occt [ABI]  OpenCASCADE for Android with the NDK (ANDROID_NDK_HOME, OCCT_SRC)
//!   cargo xtask android-cli [ABI]   shapecraft-cli for Android (arm64 or x86_64), checked for 16 KB pages
//!   cargo xtask android-smoke [ABI] kernel smoke test on the adb device
//!   cargo xtask android-apk [ABI]   the app as an APK (ANDROID_HOME with build-tools and a platform)
//!   cargo xtask android-run [ABI]   installs the APK on the adb device, starts it, takes a screenshot
//!   cargo xtask android-release-key [--upload]  makes the APK release key once (--upload: as GitHub secrets)
//!   cargo xtask bench-tasks         writes bench/tasks/*.jsonl, the task benchmark
//!   cargo xtask icons OUT_DIR       the app icon: SVG variants and PNG renders
//!   cargo xtask home-art SCRATCH    the home screen's pictures (assets/home/*.png), from our own models
//!   cargo xtask site-scenes OUT_DIR the website's recording scenarios (snap .jsonl)
//!   cargo xtask site-media SCRATCH  records the website's GIFs, videos and stills (needs gifski, ffmpeg)
//!   cargo xtask site-shot URL OUT W H STEPS.json   drives headless Chromium to look at a page
//!   cargo xtask assets              every file asset has its ATTRIBUTION.md row
//!   cargo xtask layers              the crates depend on each other only downwards

mod android;
mod bench;
mod checks;
mod icons;
mod linux;
mod mac;
mod site;
mod util;
mod windows;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let result = match args.first().map(String::as_str) {
        Some("bundle-mac") => mac::bundle(),
        Some("bundle-linux") => linux::bundle(),
        Some("appimage") => linux::appimage(),
        Some("bundle-windows") => windows::bundle(),
        Some("android-occt") => android::occt(rest),
        Some("android-cli") => android::cli(rest),
        Some("android-smoke") => android::smoke(rest),
        Some("android-apk") => android::apk(rest),
        Some("android-run") => android::apk_run(rest),
        Some("android-release-key") => android::release_key(rest),
        Some("bench-tasks") => bench::write(rest),
        Some("icons") => icons::write(rest),
        Some("home-art") => site::home_art(rest),
        Some("site-scenes") => site::scenes(rest),
        Some("site-media") => site::media(rest),
        Some("site-shot") => site::shot(rest),
        Some("assets") => checks::assets(),
        Some("layers") => checks::layers(),
        _ => {
            eprintln!(
                "{}",
                include_str!("main.rs")
                    .lines()
                    .take_while(|l| l.starts_with("//!"))
                    .map(|l| l.trim_start_matches("//!"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return std::process::ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
