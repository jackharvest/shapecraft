//! Android builds of the command-line tools (issue #3's first step): OpenCASCADE cross-compiled with the NDK, the
//! CLI linked against it statically, and a kernel smoke test run on a device or emulator through adb.
//!
//!   ANDROID_NDK_HOME=… OCCT_SRC=… cargo xtask android-occt [arm64|x86_64]   → target/android/occt-<abi>
//!   ANDROID_NDK_HOME=… cargo xtask android-cli [arm64|x86_64]                → target/<triple>/release/shapecraft-cli
//!   cargo xtask android-smoke [arm64|x86_64]                                 (adb on PATH, one device attached)
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
    let ndk = ndk()?;
    let bin = toolchain(&ndk)?;
    let occt = occt_dir(&abi);
    if !occt.join("lib/libTKernel.a").exists() {
        bail!("no OCCT at {}: run cargo xtask android-occt first", occt.display());
    }
    let clang = bin.join(format!("{}{API}-clang", abi.triple));
    let under = abi.triple.replace('-', "_");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root())
        .args(["build", "--release", "-p", "shapecraft-cli", "--target", abi.triple])
        .env(format!("CC_{under}"), &clang)
        .env(format!("CXX_{under}"), bin.join(format!("{}{API}-clang++", abi.triple)))
        .env(format!("AR_{under}"), bin.join("llvm-ar"))
        .env(format!("CARGO_TARGET_{}_LINKER", under.to_uppercase()), &clang)
        .env("OCCT_INCLUDE_DIR", occt.join("include/opencascade"))
        .env("OCCT_LIB_DIR", occt.join("lib"))
        .status()
        .context("couldn't start cargo")?;
    if !status.success() {
        bail!("cargo build for {} failed ({status})", abi.triple);
    }
    let exe = target().join(abi.triple).join("release/shapecraft-cli");
    let segments = output(s(&bin.join("llvm-readelf")), &["--program-headers", "--wide", s(&exe)])?;
    let loads: Vec<&str> = segments.lines().filter(|l| l.trim_start().starts_with("LOAD")).collect();
    let aligned = |l: &&str| l.split_whitespace().last().and_then(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok());
    if loads.is_empty() || loads.iter().any(|l| aligned(l).is_none_or(|a| a < 0x4000)) {
        bail!("{} has segments not aligned for 16 KB pages:\n{}", exe.display(), loads.join("\n"));
    }
    println!("{} ({}, {} LOAD segments, 16 KB aligned)", exe.display(), mb(&exe), loads.len());
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
    fn smoke_volume_matches_the_desktop_kernel() {
        // Worked out separately by hand/Python; the desktop CLI measures 23121.51626278067 mm³ for the same script.
        assert!((super::smoke_volume() - 23121.516_262_780_68).abs() < 1e-6);
    }
}
