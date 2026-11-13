//! Putting a staged update in place. Files in use can't be swapped safely from inside the running app (Windows
//! locks them; elsewhere a half-swapped app could load a mix), so a small helper script waits for ShapeCraft to
//! close, swaps the new version in (keeping the old one until the new one is in place), then starts it again if
//! asked. Its log is `update.log` in the updates folder.
//!
//! | Installed as | Swap |
//! |---|---|
//! | macOS `.app` | move the old bundle aside, move the new one in, remove the old |
//! | Windows MSI | install the new MSI with a progress bar only (a major upgrade of the same product) |
//! | Windows setup program (before the MSI) | install the MSI into the same folder, then remove the setup program's registration |
//! | Windows portable zip | copy the new files over the folder |
//! | AppImage | move the new AppImage over the old file |
//! | Linux download folder | move the old folder aside, move the new one in, remove the old |

use std::ffi::OsString;
use std::path::Path;

use crate::install::Install;

/// Where a helper that couldn't install the update leaves the reason, beside its log.
pub(crate) const FAILURE_FILE: &str = "last-failure.txt";

/// Starts the helper. ShapeCraft must close straight after (the helper waits for it, up to three minutes).
pub fn launch(install: &Install, staged: &Path, version_dir: &Path, log: &Path, restart: bool, args: &[OsString]) -> Result<(), String> {
    let pid = std::process::id();
    let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    if cfg!(windows) {
        let script = windows_script(install, staged, version_dir, log, pid, restart, &args)?;
        spawn_windows(&script)
    } else {
        let script = unix_script(install, staged, version_dir, log, pid, restart, &args)?;
        let file = version_dir.parent().unwrap_or(version_dir).join("update-helper.sh");
        std::fs::write(&file, script).map_err(|e| format!("couldn't write the update helper: {e}"))?;
        spawn_unix(&file)
    }
}

/// A POSIX shell word.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn p(path: &Path) -> String {
    sq(&path.to_string_lossy())
}

/// Swaps a folder or bundle: the old one moves aside until the new one is in, and comes back if it can't be.
fn swap(target: &Path, new: &Path) -> String {
    let (t, n) = (p(target), p(new));
    let old = sq(&format!("{}.previous", target.to_string_lossy()));
    format!(
        "rm -rf {old}\nmv {t} {old} || fail \"couldn't move the old version aside\"\n\
         if ! mv {n} {t}; then rm -rf {t}; mv {old} {t}; fail \"couldn't put the new version in place, so the old one was kept\"; fi\n\
         rm -rf {old}\n"
    )
}

pub(crate) fn unix_script(
    install: &Install,
    staged: &Path,
    version_dir: &Path,
    log: &Path,
    pid: u32,
    restart: bool,
    args: &[String],
) -> Result<String, String> {
    let words = args.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ");
    let (steps, relaunch) = match install {
        Install::MacApp(bundle) => (swap(bundle, staged), format!("open {} --args {words}", p(bundle))),
        Install::AppImage(image) => (
            format!("chmod +x {s}\nmv -f {s} {i} || fail \"couldn't replace the AppImage\"\n", s = p(staged), i = p(image)),
            format!("env -u APPIMAGE -u APPDIR -u OWD -u ARGV0 {} {words} >/dev/null 2>&1 &", p(image)),
        ),
        Install::LinuxFolder(root) => {
            (swap(root, staged), format!("{} {words} >/dev/null 2>&1 &", p(&root.join("bin").join("shapecraft"))))
        }
        _ => return Err("updates can't install themselves here".into()),
    };
    let relaunch = if restart { relaunch } else { String::new() };
    // A failed install says why at the next start (`take_failure`) and, when asked to, still reopens the old version.
    Ok(format!(
        "#!/bin/sh\n# ShapeCraft's update helper: waits for ShapeCraft to close, then swaps in the new version.\n\
         exec >>{log} 2>&1\necho \"== $(date): updating ShapeCraft\"\n\
         fail() {{ echo \"$1\"; printf '%s\\n' \"$1\" > {failed}; cd \"$HOME\" 2>/dev/null; {relaunch}\n exit 1; }}\nn=0\n\
         while kill -0 {pid} 2>/dev/null; do n=$((n+1)); [ $n -gt 900 ] && {{ echo \"ShapeCraft didn't close; nothing changed\"; exit 1; }}; sleep 0.2; done\n\
         [ -e {staged} ] || fail \"the downloaded update went missing before it could be installed\"\n\
         {steps}echo \"updated\"\nrm -f {failed}\nrm -rf {dir}\ncd \"$HOME\" 2>/dev/null\n{relaunch}\n",
        log = p(log),
        failed = p(&log.with_file_name(FAILURE_FILE)),
        staged = p(staged),
        dir = p(version_dir),
    ))
}

/// A PowerShell string.
fn pq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub(crate) fn windows_script(
    install: &Install,
    staged: &Path,
    version_dir: &Path,
    log: &Path,
    pid: u32,
    restart: bool,
    args: &[String],
) -> Result<String, String> {
    let s = |path: &Path| pq(&path.to_string_lossy());
    let (steps, exe) = match install {
        // 3010: installed, a restart would finish it (nothing of ShapeCraft's needs one). An install for everyone asks
        // Windows for permission first.
        Install::WindowsMsi(dir) => (
            format!(
                "$r = Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i',('\"' + {} + '\"'),'/passive','/norestart' -Wait -PassThru\n\
                 if ($r.ExitCode -ne 0 -and $r.ExitCode -ne 3010) {{ throw \"the MSI stopped with code $($r.ExitCode)\" }}\n",
                s(staged)
            ),
            dir.join("shapecraft.exe"),
        ),
        // A copy from the old setup program (Inno Setup) moves to the MSI: the MSI goes into the same folder, for the
        // same users, replacing every file (REINSTALLMODE a: the setup program's files carry their own dates, which
        // Windows Installer would otherwise keep as "changed by the user"). Only once it is in does the setup program's
        // registration go (its uninstaller files, its Apps & features entry, its Start menu folder), so a failed MSI
        // leaves the old copy as it was.
        Install::WindowsInstalled(dir) => {
            let everyone = dir.to_string_lossy().to_ascii_lowercase().contains("program files");
            let scope = if everyone { "'ALLUSERS=1'" } else { "'ALLUSERS=2','MSIINSTALLPERUSER=1'" };
            let hive = if everyone { "HKLM" } else { "HKCU" };
            let menu = if everyone { "CommonPrograms" } else { "Programs" };
            (
                format!(
                    "$r = Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i',('\"' + {staged} + '\"'),'/passive','/norestart',{scope},'REINSTALLMODE=amus',('INSTALLFOLDER=\"' + {dir} + '\"') -Wait -PassThru\n\
                     if ($r.ExitCode -ne 0 -and $r.ExitCode -ne 3010) {{ throw \"the MSI stopped with code $($r.ExitCode)\" }}\n\
                     Say 'moved from the setup program to the MSI'\n\
                     Get-ChildItem -LiteralPath {dir} -Filter 'unins*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue\n\
                     Remove-Item -LiteralPath '{hive}:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{{6F1C2A4E-3B7D-4C59-9E2A-5D8B1F0C7A31}}_is1' -Recurse -Force -ErrorAction SilentlyContinue\n\
                     Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('{menu}')) 'ShapeCraft') -Recurse -Force -ErrorAction SilentlyContinue\n",
                    staged = s(staged),
                    dir = s(dir),
                ),
                dir.join("shapecraft.exe"),
            )
        }
        Install::WindowsPortable(dir) => (
            format!(
                "& robocopy {} {} /E /R:10 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null\n\
                 if ($LASTEXITCODE -ge 8) {{ throw \"copying the new files failed (robocopy $LASTEXITCODE)\" }}\n",
                s(staged),
                s(dir)
            ),
            dir.join("shapecraft.exe"),
        ),
        _ => return Err("updates can't install themselves here".into()),
    };
    let relaunch = if !restart {
        String::new()
    } else if args.is_empty() {
        format!("Start-Process -FilePath {}\n", s(&exe))
    } else {
        let list = args.iter().map(|a| pq(&format!("\"{}\"", a.replace('"', "\\\"")))).collect::<Vec<_>>().join(",");
        format!("Start-Process -FilePath {} -ArgumentList {list}\n", s(&exe))
    };
    Ok(format!(
        "$ErrorActionPreference = 'Stop'\n$log = {log}\n$failed = {failed}\n\
         function Say($m) {{ Add-Content -LiteralPath $log -Value ((Get-Date -Format s) + ' ' + $m) }}\n\
         $p = Get-Process -Id {pid} -ErrorAction SilentlyContinue\n\
         if ($p -and -not $p.WaitForExit(180000)) {{ Say \"ShapeCraft didn't close; nothing changed\"; exit 1 }}\n\
         try {{\nSay 'updating ShapeCraft'\nStart-Sleep -Milliseconds 500\n\
         if (-not (Test-Path -LiteralPath {staged})) {{ throw 'the downloaded update went missing before it could be installed' }}\n\
         {steps}Say 'updated'\nRemove-Item -LiteralPath $failed -Force -ErrorAction SilentlyContinue\n\
         Remove-Item -LiteralPath {dir} -Recurse -Force -ErrorAction SilentlyContinue\n{relaunch}\
         }} catch {{ Say ('failed: ' + $_); Set-Content -LiteralPath $failed -Value ([string]$_); {relaunch}exit 1 }}\n",
        log = s(log),
        failed = s(&log.with_file_name(FAILURE_FILE)),
        staged = s(staged),
        dir = s(version_dir),
    ))
}

#[cfg(unix)]
fn spawn_unix(script: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("/bin/sh")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't start the update helper: {e}"))
}

#[cfg(not(unix))]
fn spawn_unix(_script: &Path) -> Result<(), String> {
    Err("no shell here".into())
}

/// Runs the script in a hidden PowerShell, passed as an encoded command (so no script file and no execution policy is
/// involved), in its own process group, out of any job ShapeCraft runs in.
#[cfg(windows)]
fn spawn_windows(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64(&utf16);
    let shell = std::env::var_os("SystemRoot").map_or_else(
        || std::path::PathBuf::from("powershell.exe"),
        |r| std::path::PathBuf::from(r).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
    );
    let start = |flags: u32| {
        std::process::Command::new(&shell)
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", &encoded])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    start(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| start(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP))
        .map(|_| ())
        .map_err(|e| format!("couldn't start the update helper: {e}"))
}

#[cfg(not(windows))]
fn spawn_windows(_script: &str) -> Result<(), String> {
    Err("not Windows".into())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn the_shell_helper_parses() {
        for install in [
            Install::MacApp(PathBuf::from("/Applications/Shape Craft's.app")),
            Install::AppImage(PathBuf::from("/home/a/ShapeCraft.AppImage")),
            Install::LinuxFolder(PathBuf::from("/home/a/.local/lib/shapecraft")),
        ] {
            for restart in [true, false] {
                let s = unix_script(
                    &install,
                    Path::new("/c/0.4.3/new"),
                    Path::new("/c/0.4.3"),
                    Path::new("/c/update.log"),
                    9,
                    restart,
                    &["--x".into()],
                )
                .unwrap_or_default();
                let dir = std::env::temp_dir().join(format!("sc-helper-{}-{restart}", std::process::id()));
                let _ = std::fs::write(&dir, &s);
                let ok = std::process::Command::new("/bin/sh").arg("-n").arg(&dir).status().map(|s| s.success()).unwrap_or(false);
                let _ = std::fs::remove_file(&dir);
                assert!(ok, "{s}");
                assert!(s.contains("fail()") && s.contains("last-failure.txt"));
            }
        }
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn shell_words_survive_quotes() {
        assert_eq!(sq("it's"), r"'it'\''s'");
        assert_eq!(pq("it's"), "'it''s'");
    }

    #[test]
    fn scripts_swap_and_relaunch() {
        let app = Install::MacApp(PathBuf::from("/Applications/ShapeCraft.app"));
        let s = unix_script(
            &app,
            Path::new("/c/0.4.1/ShapeCraft.app"),
            Path::new("/c/0.4.1"),
            Path::new("/c/update.log"),
            42,
            true,
            &["--control".into(), "4".into()],
        )
        .unwrap_or_default();
        assert!(s.contains("kill -0 42"));
        assert!(s.contains("mv '/Applications/ShapeCraft.app' '/Applications/ShapeCraft.app.previous'"));
        assert!(s.contains("open '/Applications/ShapeCraft.app' --args '--control' '4'"));
        let quiet = unix_script(&app, Path::new("/n"), Path::new("/c"), Path::new("/l"), 1, false, &[]).unwrap_or_default();
        assert!(!quiet.contains("open "));
        let win = Install::WindowsInstalled(PathBuf::from(r"C:\Users\O'Neil\AppData\Local\Programs\ShapeCraft"));
        let w =
            windows_script(&win, Path::new(r"C:\c\x.msi"), Path::new(r"C:\c"), Path::new(r"C:\c\log"), 7, true, &[]).unwrap_or_default();
        assert!(w.contains("Get-Process -Id 7"));
        assert!(w.contains("'ALLUSERS=2','MSIINSTALLPERUSER=1','REINSTALLMODE=amus',('INSTALLFOLDER=\"' + 'C:\\Users\\O''Neil"));
        assert!(w.contains("_is1"));
        // The setup program's registration goes only after the MSI went in.
        assert!(w.find("msiexec") < w.find("unins*"));
        assert!(w.contains(r"Start-Process -FilePath 'C:\Users\O''Neil\AppData\Local\Programs\ShapeCraft"));
        let msi = Install::WindowsMsi(PathBuf::from(r"C:\Program Files\ShapeCraft"));
        let m = windows_script(
            &msi,
            Path::new(r"C:\c\shapecraft-0.8.0-windows-x64.msi"),
            Path::new(r"C:\c"),
            Path::new(r"C:\c\log"),
            7,
            true,
            &[],
        )
        .unwrap_or_default();
        assert!(m.contains(r#"'msiexec.exe' -ArgumentList '/i',('"' + 'C:\c\shapecraft-0.8.0-windows-x64.msi' + '"'),'/passive'"#));
        assert!(m.contains("-ne 3010"));
        assert!(m.contains(r"Start-Process -FilePath 'C:\Program Files\ShapeCraft"));
        assert!(windows_script(&Install::Dev, Path::new("x"), Path::new("x"), Path::new("x"), 1, true, &[]).is_err());
    }
}
