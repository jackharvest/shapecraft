//! The Windows downloads from a release build: a portable zip (the app, the CLI and every DLL they load that Windows
//! doesn't ship: OpenCASCADE, TBB, the VC runtime…), an installer built from the same folder with Inno Setup, and an
//! MSI of the same folder with WiX (what the Craft App Suite's launcher installs).
//!
//!   cargo build --release -p shapecraft -p shapecraft-cli
//!   cargo xtask bundle-windows          (in a Visual Studio developer shell, for dumpbin)
//!   → target\shapecraft-<version>-windows-x64-portable.zip, …-windows-x64-setup.exe and …-windows-x64.msi
//!
//! DLLs are looked up in OCCT_BIN (default: CONDA_PREFIX\Library\bin), then CONDA_PREFIX; anything not found there
//! is treated as part of Windows. The installer needs Inno Setup 6 (iscc on PATH or in Program Files), the MSI WiX 5
//! (`wix` on PATH); either is skipped with a warning when its tool is missing.

use std::collections::{BTreeSet, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::util::{clear, copy, output, root, s, target, version};

/// The DLLs a file imports (dumpbin /dependents).
fn imports(file: &Path) -> Result<Vec<String>> {
    Ok(output("dumpbin", &["/nologo", "/dependents", s(file)])?
        .lines()
        .map(str::trim)
        .filter(|l| !l.contains(' ') && l.to_ascii_lowercase().ends_with(".dll"))
        .map(str::to_string)
        .collect())
}

/// Zips `dir` with its own folder name at the top, as Compress-Archive does.
fn zip_folder(dir: &Path, out: &Path) -> Result<()> {
    let top = dir.file_name().map(|n| n.to_string_lossy().into_owned()).context("a folder without a name")?;
    let file = std::fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(9));
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&d)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            let rel = p.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
            let name = format!("{top}/{rel}");
            if p.is_dir() {
                zip.add_directory(format!("{name}/"), opts)?;
                stack.push(p);
            } else {
                zip.start_file(name, opts)?;
                zip.write_all(&std::fs::read(&p)?)?;
            }
        }
    }
    zip.finish()?;
    Ok(())
}

pub fn bundle() -> Result<()> {
    let version = version()?;
    let release = target().join("release");
    let name = format!("shapecraft-{version}-windows-x64");
    let stage = target().join("windows-bundle").join("ShapeCraft");
    let mut search: Vec<PathBuf> = Vec::new();
    if let Some(b) = std::env::var_os("OCCT_BIN") {
        search.push(PathBuf::from(b));
    }
    if let Some(c) = std::env::var_os("CONDA_PREFIX") {
        search.push(PathBuf::from(&c).join("Library").join("bin"));
        search.push(PathBuf::from(c));
    }
    if search.is_empty() {
        bail!("set OCCT_BIN or activate the conda environment with OCCT");
    }
    clear(&target().join("windows-bundle"))?;
    std::fs::create_dir_all(&stage)?;
    for exe in ["shapecraft.exe", "shapecraft-cli.exe"] {
        if !release.join(exe).exists() {
            bail!("no release build: {}", release.join(exe).display());
        }
        copy(&release.join(exe), &stage.join(exe))?;
    }

    // The DLL closure: what each file imports, found in the search folders, copied, and walked in turn.
    let mut queue: VecDeque<PathBuf> = VecDeque::from([stage.join("shapecraft.exe"), stage.join("shapecraft-cli.exe")]);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(file) = queue.pop_front() {
        for dll in imports(&file)? {
            if !seen.insert(dll.to_ascii_lowercase()) {
                continue;
            }
            if let Some(src) = search.iter().map(|d| d.join(&dll)).find(|p| p.exists()) {
                copy(&src, &stage.join(&dll))?;
                queue.push_back(stage.join(&dll));
            }
        }
    }
    let count = std::fs::read_dir(&stage)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("dll")))
        .count();
    println!("{count} DLLs bundled");
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "DISCLAIMER.md"] {
        if root().join(f).exists() {
            copy(&root().join(f), &stage.join(f))?;
        }
    }
    copy(&root().join("assets/icon/shapecraft.ico"), &stage.join("shapecraft.ico"))?;

    // Both programs must start with only the folder's DLLs: the CLI is enough to load all of OCCT.
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let status = std::process::Command::new(stage.join("shapecraft-cli.exe"))
        .arg("commands")
        .env("PATH", format!("{windir}\\system32;{windir}"))
        .stdout(std::process::Stdio::null())
        .status()?;
    if !status.success() {
        bail!("shapecraft-cli.exe didn't start from the bundle folder ({status})");
    }

    let zip = target().join(format!("{name}-portable.zip"));
    let _ = std::fs::remove_file(&zip);
    zip_folder(&stage, &zip)?;
    println!("{}", zip.display());

    let iscc = std::process::Command::new("iscc").arg("/?").output().is_ok().then(|| PathBuf::from("iscc")).or_else(|| {
        let pf = std::env::var("ProgramFiles(x86)").ok()?;
        let p = PathBuf::from(pf).join("Inno Setup 6").join("ISCC.exe");
        p.exists().then_some(p)
    });
    let Some(iscc) = iscc else {
        eprintln!("warning: Inno Setup not found: no installer made");
        return msi(&stage, &version, &name);
    };
    let status = std::process::Command::new(&iscc)
        .args([
            "/Qp".to_string(),
            format!("/DVersion={version}"),
            format!("/DStage={}", stage.display()),
            format!("/DOutDir={}", target().display()),
            format!("/DOutName={name}-setup"),
            root().join("packaging/windows/shapecraft.iss").display().to_string(),
        ])
        .status()
        .context("couldn't start Inno Setup")?;
    if !status.success() {
        bail!("Inno Setup failed ({status})");
    }
    println!("{}", target().join(format!("{name}-setup.exe")).display());
    msi(&stage, &version, &name)
}

/// The MSI, from the same folder (WiX v5: `dotnet tool install --global wix --version 5.0.2`). The launcher of the
/// Craft App Suite installs apps from their MSI. Its version is the numeric X.Y.Z (MSI versions have no labels).
fn msi(stage: &Path, version: &str, name: &str) -> Result<()> {
    if std::process::Command::new("wix").arg("--version").output().is_err() {
        eprintln!("warning: WiX not found: no MSI made");
        return Ok(());
    }
    let numeric = version.split(['-', '+']).next().unwrap_or(version);
    let out = target().join(format!("{name}.msi"));
    let _ = std::fs::remove_file(&out);
    let status = std::process::Command::new("wix")
        .args([
            "build".to_string(),
            "-arch".into(),
            "x64".into(),
            "-d".into(),
            format!("Version={numeric}"),
            "-d".into(),
            format!("Stage={}", stage.display()),
            "-d".into(),
            format!("Marker={}", root().join("packaging/windows/installed-by-msi.txt").display()),
            "-o".into(),
            out.display().to_string(),
            root().join("packaging/windows/shapecraft.wxs").display().to_string(),
        ])
        .status()
        .context("couldn't start WiX")?;
    if !status.success() {
        bail!("WiX failed ({status})");
    }
    println!("{}", out.display());
    Ok(())
}
