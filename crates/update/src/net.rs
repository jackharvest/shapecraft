//! HTTPS: the one API request a check makes, and downloads. Nothing else is sent: no identifiers, no usage data.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetError {
    /// GitHub's limit for anonymous requests (60 an hour) was reached.
    RateLimited,
    Failed(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::RateLimited => f.write_str("GitHub asked us to wait a while before checking again"),
            NetError::Failed(e) => f.write_str(e),
        }
    }
}

/// No release file is bigger than this (they are 30–60 MB).
pub const MAX_DOWNLOAD: u64 = 1 << 30;
const MAX_API_ANSWER: u64 = 4 << 20;

fn agent(quick: bool) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let mut config = ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(format!("ShapeCraft/{}", crate::current_text()))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .http_status_as_error(false);
    config = if quick {
        config.timeout_global(Some(Duration::from_secs(30)))
    } else {
        config.timeout_recv_body(Some(Duration::from_secs(30 * 60)))
    };
    config.build().into()
}

fn describe(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => "the connection timed out".into(),
        ureq::Error::Timeout(_) => "the connection timed out".into(),
        ureq::Error::HostNotFound => "no internet connection (couldn't find github.com)".into(),
        other => other.to_string(),
    }
}

/// A short text answer (the release list, `SHA256SUMS.txt`).
pub fn get_text(url: &str, api: bool) -> Result<String, NetError> {
    let mut req = agent(true).get(url);
    if api {
        req = req.header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
    }
    let mut resp = req.call().map_err(|e| NetError::Failed(describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 429 || (status == 403 && resp.headers().get("x-ratelimit-remaining").is_some_and(|v| v == "0")) {
        return Err(NetError::RateLimited);
    }
    if status == 404 {
        return Err(NetError::Failed("ShapeCraft's releases aren't published yet".into()));
    }
    if status != 200 {
        return Err(NetError::Failed(format!("GitHub answered {status}")));
    }
    resp.body_mut().with_config().limit(MAX_API_ANSWER).read_to_string().map_err(|e| NetError::Failed(describe(&e)))
}

/// Downloads `url` to `dest` (through `dest.part`), checking its SHA-256 against `sha256` before it gets its name.
/// `progress(done, total)` is told as it goes; `stop` ends it early.
pub fn download(url: &str, dest: &Path, sha256: &str, stop: &AtomicBool, progress: &dyn Fn(u64, Option<u64>)) -> Result<(), String> {
    let mut resp = agent(false).get(url).call().map_err(|e| describe(&e))?;
    let status = resp.status().as_u16();
    if status != 200 {
        return Err(format!("the download answered {status}"));
    }
    let total = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    if total.is_some_and(|t| t > MAX_DOWNLOAD) {
        return Err("the download is larger than any ShapeCraft release".into());
    }
    let part = dest.with_extension(format!("{}part", dest.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()));
    let result = (|| {
        let mut file = std::fs::File::create(&part).map_err(|e| format!("couldn't write {}: {e}", part.display()))?;
        let mut reader = resp.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut done = 0u64;
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped".to_string());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("the download broke off: {e}"))?;
            if n == 0 {
                break;
            }
            let chunk = buf.get(..n).unwrap_or_default();
            hasher.update(chunk);
            file.write_all(chunk).map_err(|e| format!("couldn't write the download: {e}"))?;
            done += n as u64;
            progress(done, total);
        }
        file.sync_all().map_err(|e| format!("couldn't write the download: {e}"))?;
        let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !got.eq_ignore_ascii_case(sha256) {
            return Err("the download didn't match its published checksum, so it was thrown away".to_string());
        }
        std::fs::rename(&part, dest).map_err(|e| format!("couldn't keep the download: {e}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

/// SHA-256 of a file, lowercase hex.
pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n)?);
    }
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
