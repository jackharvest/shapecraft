//! Android builds: OpenCASCADE cross-compiled with the NDK and linked in statically, the command-line tools with a
//! kernel smoke test run through adb, and the app itself as an APK (a NativeActivity around the desktop interface).
//!
//!   ANDROID_NDK_HOME=… OCCT_SRC=… cargo xtask android-occt [arm64|x86_64]   → target/android/occt-<abi>
//!   ANDROID_NDK_HOME=… cargo xtask android-cli [arm64|x86_64]                → target/<triple>/release/shapecraft-cli
//!   cargo xtask android-smoke [arm64|x86_64]                                 (adb on PATH, one device attached)
//!   ANDROID_HOME=… cargo xtask android-apk [arm64|x86_64]                    → target/android/ShapeCraft-<version>-<abi>.apk
//!   cargo xtask android-run [arm64|x86_64]                                   installs, starts, screenshots (adb)
//!
//! Linux or macOS host; NDK r28 or newer (16 KB pages by default, which Android 15+ devices need). See
//! packaging/android/README.md.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util::{clear, copy, mb, output, root, run, s, target};

/// The oldest Android release the tools run on (8.0). Rust's own floor is lower; OCCT needs nothing newer.
const API: u32 = 26;

/// One Android ABI: the NDK's name for it and Rust's target triple.
struct Abi {
    ndk: &'static str,
    triple: &'static str,
}

fn abi(rest: &[String]) -> Result<Abi> {
    match rest.first().map(String::as_str) {
        None | Some("arm64") => Ok(Abi { ndk: "arm64-v8a", triple: "aarch64-linux-android" }),
        Some("x86_64") => Ok(Abi { ndk: "x86_64", triple: "x86_64-linux-android" }),
        Some(other) => bail!("unknown ABI {other}: arm64 or x86_64"),
    }
}

fn ndk() -> Result<PathBuf> {
    let ndk = std::env::var_os("ANDROID_NDK_HOME").or_else(|| std::env::var_os("NDK")).context("set ANDROID_NDK_HOME to the NDK")?;
    let ndk = PathBuf::from(ndk);
    if !ndk.join("build/cmake/android.toolchain.cmake").exists() {
        bail!("{} is not an Android NDK (no build/cmake/android.toolchain.cmake)", ndk.display());
    }
    Ok(ndk)
}

/// The NDK's clang and binutils folder for this host.
fn toolchain(ndk: &Path) -> Result<PathBuf> {
    let host = match std::env::consts::OS {
        "linux" => "linux-x86_64",
        "macos" => "darwin-x86_64", // a universal build despite the name
        os => bail!("Android builds run on Linux or macOS hosts, not {os}"),
    };
    Ok(ndk.join("toolchains/llvm/prebuilt").join(host).join("bin"))
}

fn occt_dir(abi: &Abi) -> PathBuf {
    target().join("android").join(format!("occt-{}", abi.ndk))
}

/// Configures, builds and installs OpenCASCADE for Android: static libraries, only the toolkits our shim links (a
/// fifth of OCCT's build; the rest is persistence, mesh formats and viewers), and none of the optional third-party
/// products (FreeType, OpenGL, Tcl/Tk, …).
pub fn occt(rest: &[String]) -> Result<()> {
    let abi = abi(rest)?;
    let ndk = ndk()?;
    let src = PathBuf::from(std::env::var_os("OCCT_SRC").context("set OCCT_SRC to an OCCT 7.9 source tree")?);
    let build = target().join("android").join(format!("occt-build-{}", abi.ndk));
    let install = occt_dir(&abi);
    let toolchain_file = ndk.join("build/cmake/android.toolchain.cmake");
    let mut args = vec![
        "-S".to_string(),
        s(&src).to_string(),
        "-B".to_string(),
        s(&build).to_string(),
        "-G".to_string(),
        "Ninja".to_string(),
        format!("-DCMAKE_TOOLCHAIN_FILE={}", s(&toolchain_file)),
        format!("-DANDROID_ABI={}", abi.ndk),
        format!("-DANDROID_PLATFORM=android-{API}"),
        // One self-contained executable: libc++ goes inside, so there is no libc++_shared.so to ship beside it.
        "-DANDROID_STL=c++_static".to_string(),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
        "-DBUILD_LIBRARY_TYPE=Static".to_string(),
        "-DBUILD_DOC_Overview=OFF".to_string(),
        "-DBUILD_SAMPLES_QT=OFF".to_string(),
        "-DBUILD_MODULE_Draw=OFF".to_string(),
    ];
    for off in ["FREETYPE", "OPENGL", "GLES2", "TK", "FREEIMAGE", "RAPIDJSON", "TBB", "VTK", "DRACO", "OPENVR", "FFMPEG"] {
        args.push(format!("-DUSE_{off}=OFF"));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run("cmake", &args)?;
    let toolkits = linked_toolkits()?;
    let mut build_args = vec!["--build", s(&build), "--target"];
    build_args.extend(toolkits.iter().map(String::as_str));
    run("cmake", &build_args)?;
    // `install` would build every toolkit, so copy the archives and the headers the configure step laid out.
    clear(&install)?;
    let lib = install.join("lib");
    for tk in &toolkits {
        let name = format!("lib{tk}.a");
        let found = find(&build, &name).with_context(|| format!("{name} wasn't built"))?;
        copy(&found, &lib.join(&name))?;
    }
    let headers = build.join("include/opencascade");
    let into = install.join("include/opencascade");
    std::fs::create_dir_all(&into)?;
    for entry in std::fs::read_dir(&headers).with_context(|| format!("no headers in {}", headers.display()))? {
        let path = entry?.path();
        if let Some(name) = path.file_name() {
            std::fs::copy(&path, into.join(name)).with_context(|| format!("copying {}", path.display()))?;
        }
    }
    println!("OCCT for {} ({} toolkits) → {}", abi.ndk, toolkits.len(), install.display());
    Ok(())
}

/// The OCCT toolkits kernel-occt links, read from its build script so the list lives in one place.
fn linked_toolkits() -> Result<Vec<String>> {
    let script = std::fs::read_to_string(root().join("crates/kernel-occt/build.rs"))?;
    let start = script.find("const LIBS").context("no LIBS list in kernel-occt/build.rs")?;
    let list = &script[start..start + script[start..].find("];").context("LIBS list not closed")?];
    let names: Vec<String> = list.split('"').skip(1).step_by(2).map(str::to_string).collect();
    if names.is_empty() {
        bail!("empty LIBS list in kernel-occt/build.rs");
    }
    Ok(names)
}

/// The first file called `name` under `dir`.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

/// Cross-compiles shapecraft-cli against the OCCT that `android-occt` installed, then checks the result is an
/// executable whose segments are aligned for 16 KB pages.
pub fn cli(rest: &[String]) -> Result<()> {
    let abi = abi(rest)?;
    let bin = build(&abi, "shapecraft-cli")?;
    let exe = target().join(abi.triple).join("release/shapecraft-cli");
    let loads = check_pages(&bin, &exe)?;
    println!("{} ({}, {loads} LOAD segments, 16 KB aligned)", exe.display(), mb(&exe));
    Ok(())
}

/// `cargo build --release -p package` for an ABI with the NDK's clang and the OCCT `android-occt` installed. Returns
/// the NDK's tool folder.
fn build(abi: &Abi, package: &str) -> Result<PathBuf> {
    let ndk = ndk()?;
    let bin = toolchain(&ndk)?;
    let occt = occt_dir(abi);
    if !occt.join("lib/libTKernel.a").exists() {
        bail!("no OCCT at {}: run cargo xtask android-occt first", occt.display());
    }
    let clang = bin.join(format!("{}{API}-clang", abi.triple));
    let under = abi.triple.replace('-', "_");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root())
        .args(["build", "--release", "-p", package, "--target", abi.triple])
        .env(format!("CC_{under}"), &clang)
        .env(format!("CXX_{under}"), bin.join(format!("{}{API}-clang++", abi.triple)))
        .env(format!("AR_{under}"), bin.join("llvm-ar"))
        .env(format!("CARGO_TARGET_{}_LINKER", under.to_uppercase()), &clang)
        .env("OCCT_INCLUDE_DIR", occt.join("include/opencascade"))
        .env("OCCT_LIB_DIR", occt.join("lib"))
        .status()
        .context("couldn't start cargo")?;
    if !status.success() {
        bail!("cargo build of {package} for {} failed ({status})", abi.triple);
    }
    Ok(bin)
}

/// Checks every loadable segment of an ELF file is aligned for 16 KB pages (Android 15+ devices); returns how many.
fn check_pages(bin: &Path, file: &Path) -> Result<usize> {
    let segments = output(s(&bin.join("llvm-readelf")), &["--program-headers", "--wide", s(file)])?;
    let loads: Vec<&str> = segments.lines().filter(|l| l.trim_start().starts_with("LOAD")).collect();
    let aligned = |l: &&str| l.split_whitespace().last().and_then(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok());
    if loads.is_empty() || loads.iter().any(|l| aligned(l).is_none_or(|a| a < 0x4000)) {
        bail!("{} has segments not aligned for 16 KB pages:\n{}", file.display(), loads.join("\n"));
    }
    Ok(loads.len())
}

/// The Android package name, which also names the app's folders on the device.
pub const PACKAGE: &str = "com.jackharvest.shapecraft";

/// The app as an installable APK: apps/shapecraft-android built as libshapecraft.so, the manifest and icon linked by
/// aapt2 against the newest platform in the SDK, the library stored uncompressed on a 16 KB boundary (so Android maps
/// it straight from the APK), then aligned and signed with the debug key (~/.android/debug.keystore, made if missing).
/// Needs ANDROID_HOME (or ANDROID_SDK_ROOT) with build-tools and a platform, and a JDK for keytool and apksigner.
pub fn apk(rest: &[String]) -> Result<()> {
    let abi = abi(rest)?;
    let bin = build(&abi, "shapecraft-android")?;
    let so = target().join(abi.triple).join("release/libshapecraft.so");
    check_pages(&bin, &so)?;
    let sdk = PathBuf::from(
        std::env::var_os("ANDROID_HOME").or_else(|| std::env::var_os("ANDROID_SDK_ROOT")).context("set ANDROID_HOME to the Android SDK")?,
    );
    // Release versions only: an APK built against a preview platform refuses to install on any released Android
    // ("Requires development platform …"), and CI images carry previews (android-37.2 beta on GitHub's runner).
    let tools =
        newest(&sdk.join("build-tools"), |n| !n.contains('-')).context("no build-tools in the SDK (sdkmanager \"build-tools;35.0.1\")")?;
    let platforms = sdk.join("platforms");
    let platform = newest(&platforms, |n| released_platform(&platforms, n))
        .context("no released platform in the SDK (sdkmanager \"platforms;android-35\")")?;
    let target_sdk = platform.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_prefix("android-")).unwrap_or("35").to_string();
    let version = crate::util::version()?;
    let code = version_code(&version)?;

    let work = target().join("android").join(format!("apk-{}", abi.ndk));
    clear(&work)?;
    // The launcher icon: Android's adaptive layers (the blue tile behind, the white shape in front, cut to the
    // device's icon shape; the shape alone for themed icons), with the flat icon for anything that can't use them.
    let mipmap = work.join("res/mipmap-xxxhdpi");
    std::fs::create_dir_all(&mipmap)?;
    copy(&root().join("assets/icon/png/shapecraft-256.png"), &mipmap.join("ic_launcher.png"))?;
    let (back, front) = crate::icons::android_layers();
    // 108 dp at xxxhdpi (4 px per dp).
    std::fs::write(mipmap.join("ic_launcher_background.png"), crate::icons::png(&back, 432)?)?;
    std::fs::write(mipmap.join("ic_launcher_foreground.png"), crate::icons::png(&front, 432)?)?;
    let adaptive = work.join("res/mipmap-anydpi-v26");
    std::fs::create_dir_all(&adaptive)?;
    std::fs::write(
        adaptive.join("ic_launcher.xml"),
        r#"<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@mipmap/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
    <monochrome android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
"#,
    )?;
    let manifest = std::fs::read_to_string(root().join("packaging/android/AndroidManifest.xml"))?.replace("PACKAGE", PACKAGE);
    std::fs::write(work.join("AndroidManifest.xml"), manifest)?;
    let aapt2 = tools.join("aapt2");
    run(s(&aapt2), &["compile", "--dir", s(&work.join("res")), "-o", s(&work.join("res.zip"))])?;
    let linked = work.join("linked.apk");
    run(
        s(&aapt2),
        &[
            "link",
            "-o",
            s(&linked),
            "--manifest",
            s(&work.join("AndroidManifest.xml")),
            "-I",
            s(&platform.join("android.jar")),
            "--min-sdk-version",
            &API.to_string(),
            "--target-sdk-version",
            &target_sdk,
            "--version-code",
            &code.to_string(),
            "--version-name",
            &version,
            s(&work.join("res.zip")),
        ],
    )?;
    add_library(&linked, &format!("lib/{}/libshapecraft.so", abi.ndk), &so)?;
    let aligned = work.join("aligned.apk");
    run(s(&tools.join("zipalign")), &["-P", "16", "-f", "4", s(&linked), s(&aligned)])?;
    let key = debug_key()?;
    let apk = target().join(format!("android/ShapeCraft-{version}-{}.apk", abi.ndk));
    run(
        s(&tools.join("apksigner")),
        &["sign", "--ks", s(&key), "--ks-pass", "pass:android", "--key-pass", "pass:android", "--out", s(&apk), s(&aligned)],
    )?;
    run(s(&tools.join("apksigner")), &["verify", s(&apk)])?;
    println!("{} ({}; {PACKAGE} {version}, code {code}, API {API}-{target_sdk})", apk.display(), mb(&apk));
    Ok(())
}

/// `android-35` style folders (a whole API level) whose source.properties names no preview codename.
fn released_platform(dir: &Path, name: &str) -> bool {
    let level = name.strip_prefix("android-").unwrap_or("");
    if level.is_empty() || !level.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let props = std::fs::read_to_string(dir.join(name).join("source.properties")).unwrap_or_default();
    !props.lines().any(|l| l.trim().strip_prefix("AndroidVersion.CodeName=").is_some_and(|v| !v.trim().is_empty() && v.trim() != "REL"))
}

/// The subfolder of `dir` with the highest version-like name that passes `keep` (35.0.1 over 34.0.0, android-35 over android-9).
fn newest(dir: &Path, keep: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let rank = |name: &str| -> Vec<u32> { name.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect() };
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| keep(n))
        .max_by_key(|n| rank(n))
        .map(|n| dir.join(n))
}

/// 0.6.10 → 6010 (major·10⁶ + minor·10³ + patch): grows with every release, as Android requires for updates.
fn version_code(version: &str) -> Result<u32> {
    let core = version.split('-').next().unwrap_or(version);
    let parts: Vec<u32> = core.split('.').map(|p| p.parse::<u32>()).collect::<Result<_, _>>().context("version isn't x.y.z")?;
    match parts.as_slice() {
        [major, minor, patch] if *minor < 1000 && *patch < 1000 && *major < 2000 => Ok(major * 1_000_000 + minor * 1000 + patch),
        _ => bail!("version {version} isn't x.y.z with minor and patch under 1000"),
    }
}

/// Adds the library to the APK uncompressed, its data starting on a 16 KB boundary (zipalign -P 16 keeps it there).
fn add_library(apk: &Path, name: &str, so: &Path) -> Result<()> {
    use std::io::Write;
    let file = std::fs::OpenOptions::new().read(true).write(true).open(apk)?;
    let mut zip = zip::ZipWriter::new_append(file)?;
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).with_alignment(16384);
    zip.start_file(name, options)?;
    zip.write_all(&std::fs::read(so)?)?;
    zip.finish()?;
    Ok(())
}

/// The standard Android debug key, made once with keytool like Android Studio would.
fn debug_key() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("no HOME for ~/.android/debug.keystore")?;
    let key = PathBuf::from(home).join(".android/debug.keystore");
    if !key.exists() {
        if let Some(dir) = key.parent() {
            std::fs::create_dir_all(dir)?;
        }
        run(
            "keytool",
            &[
                "-genkeypair",
                "-keystore",
                s(&key),
                "-storepass",
                "android",
                "-keypass",
                "android",
                "-alias",
                "androiddebugkey",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "10000",
                "-dname",
                "CN=Android Debug,O=Android,C=US",
            ],
        )?;
    }
    Ok(key)
}

/// Installs the APK on the attached device, starts it, waits for the first frames and saves a screenshot.
pub fn apk_run(rest: &[String]) -> Result<()> {
    let abi = abi(rest)?;
    let version = crate::util::version()?;
    let apk = target().join(format!("android/ShapeCraft-{version}-{}.apk", abi.ndk));
    if !apk.exists() {
        bail!("no {}: run cargo xtask android-apk first", apk.display());
    }
    run("adb", &["install", "-r", s(&apk)])?;
    run("adb", &["logcat", "-c"])?;
    run("adb", &["shell", &format!("am start -W -n {PACKAGE}/android.app.NativeActivity")])?;
    std::thread::sleep(std::time::Duration::from_secs(8));
    let pid = output("adb", &["shell", &format!("pidof {PACKAGE}")])?;
    let log = output("adb", &["logcat", "-d", "-s", "ShapeCraft:*", "RustPanic:*", "AndroidRuntime:E", "DEBUG:*"])?;
    for l in log.lines().take(60) {
        println!("  {l}");
    }
    if pid.trim().is_empty() {
        bail!("ShapeCraft isn't running on the device (see the log above)");
    }
    let shot = target().join("android/screen.png");
    let png = Command::new("adb").args(["exec-out", "screencap", "-p"]).output().context("adb screencap")?;
    std::fs::write(&shot, &png.stdout)?;
    println!("running (pid {}); screenshot {}", pid.trim(), shot.display());
    Ok(())
}

/// Volume of the smoke test's part, from geometry: a 60×40×10 box with every edge rounded r = 1, less an r = 5
/// hole through its 10 mm height.
fn smoke_volume() -> f64 {
    use std::f64::consts::PI;
    let (a, b, c, r) = (60.0, 40.0, 10.0, 1.0);
    let (x, y, z) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
    let rounded = x * y * z + 2.0 * r * (x * y + x * z + y * z) + PI * r * r * (x + y + z) + 4.0 / 3.0 * PI * r * r * r;
    rounded - PI * 5.0 * 5.0 * 10.0
}

/// Pushes the CLI and packaging/android/smoke.jsonl to the attached device, runs it, and checks both the modelled
/// part and its STEP round trip against `smoke_volume`.
pub fn smoke(rest: &[String]) -> Result<()> {
    let abi = abi(rest)?;
    let exe = target().join(abi.triple).join("release/shapecraft-cli");
    if !exe.exists() {
        bail!("no {}: run cargo xtask android-cli first", exe.display());
    }
    let dir = "/data/local/tmp/shapecraft-smoke";
    let script = std::fs::read_to_string(root().join("packaging/android/smoke.jsonl"))?.replace("OUT", dir);
    let local = target().join("android/smoke.jsonl");
    std::fs::create_dir_all(target().join("android"))?;
    std::fs::write(&local, script)?;
    run("adb", &["shell", &format!("rm -rf {dir} && mkdir -p {dir}")])?;
    run("adb", &["push", s(&exe), &format!("{dir}/shapecraft-cli")])?;
    run("adb", &["push", s(&local), &format!("{dir}/smoke.jsonl")])?;
    // adb shell's own exit status is unreliable on older devices: the CLI's is printed and read back.
    let text =
        output("adb", &["shell", &format!("cd {dir} && chmod 755 shapecraft-cli && ./shapecraft-cli run smoke.jsonl 2>&1; echo rc=$?")])?;
    for l in text.lines().filter(|l| !l.starts_with("sketch") && !l.starts_with("rectangle") && !l.starts_with("circle")) {
        println!("  {}", l.chars().take(160).collect::<String>());
    }
    if !text.lines().any(|l| l.trim() == "rc=0") {
        bail!("the CLI did not finish on the device");
    }
    let mut volumes = Vec::new();
    for l in text.lines().filter_map(|l| l.strip_prefix("measure: ")) {
        let v: serde_json::Value = serde_json::from_str(l).context("reading a measure line")?;
        for item in v["items"].as_array().into_iter().flatten() {
            volumes.push(item["volume_mm3"].as_f64().context("measure without a volume")?);
        }
    }
    let want = smoke_volume();
    if volumes.len() != 2 {
        bail!("expected 2 measurements (modelled, STEP round trip), got {volumes:?}");
    }
    for (name, v) in ["modelled", "STEP round trip"].iter().zip(&volumes) {
        let ok = (v - want).abs() < 1e-6 * want;
        println!("{name}: {v:.6} mm³ (geometry says {want:.6}) {}", if ok { "ok" } else { "WRONG" });
        if !ok {
            bail!("{name} volume is wrong");
        }
    }
    println!(
        "PASS on {}",
        output("adb", &["shell", "getprop ro.product.model; getprop ro.build.version.release"])?
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" Android ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn previews_are_never_the_platform() {
        let dir = std::env::temp_dir().join(format!("sc-platforms-{}", std::process::id()));
        for (name, props) in [
            ("android-35", "AndroidVersion.ApiLevel=35\n"),
            ("android-36", "AndroidVersion.ApiLevel=36\nAndroidVersion.CodeName=Baklava\n"),
            ("android-37.2", "AndroidVersion.ApiLevel=37\n"),
            ("android-34", "AndroidVersion.ApiLevel=34\nAndroidVersion.CodeName=REL\n"),
        ] {
            let d = dir.join(name);
            std::fs::create_dir_all(&d).ok();
            std::fs::write(d.join("source.properties"), props).ok();
        }
        let picked = super::newest(&dir, |n| super::released_platform(&dir, n));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(picked.and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).as_deref(), Some("android-35"));
    }

    #[test]
    fn version_codes_grow_with_versions() {
        assert_eq!(super::version_code("0.6.10").ok(), Some(6010));
        assert_eq!(super::version_code("0.6.10-alpha").ok(), Some(6010));
        assert_eq!(super::version_code("1.0.0").ok(), Some(1_000_000));
        assert!(super::version_code("0.6").is_err());
    }

    #[test]
    fn smoke_volume_matches_the_desktop_kernel() {
        // Worked out separately by hand/Python; the desktop CLI measures 23121.51626278067 mm³ for the same script.
        assert!((super::smoke_volume() - 23121.516_262_780_68).abs() < 1e-6);
    }
}
