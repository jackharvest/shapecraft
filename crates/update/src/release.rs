//! Versions, GitHub's release list, and `SHA256SUMS.txt`. Everything here comes from the network, so it is read
//! defensively: sizes are capped, and URLs must point at this project's own releases.

use serde_json::Value;

use crate::{DOWNLOAD_PREFIX, RELEASE_PAGE_PREFIX};

/// `MAJOR.MINOR.PATCH`. Tags carry `-alpha` while ShapeCraft is in alpha (`v0.4.0-alpha`); the suffix names the stage,
/// not an order, so it is left out of comparisons.
pub type Version = (u64, u64, u64);

/// `0.4.1`, `v0.4.1-alpha`, `0.4.1+build` → (0, 4, 1).
pub fn parse_version(text: &str) -> Option<Version> {
    let t = text.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let core = t.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let v = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(v)
}

pub fn version_text(v: Version) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: Version,
    /// The release notes (Markdown, capped).
    pub notes: String,
    /// Its page on GitHub.
    pub page: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version_text(&self) -> String {
        version_text(self.version)
    }

    /// The asset whose name ends with `suffix` (`-windows-x64-portable.zip`), named for this app.
    pub fn asset_ending(&self, suffix: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.starts_with(&format!("{}-", crate::STEM)) && a.name.ends_with(suffix))
    }

    pub fn asset_named(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

const MAX_NOTES: usize = 32 * 1024;
const MAX_ASSETS: usize = 200;

/// Releases in GitHub's list (`/releases`), drafts left out, in the order GitHub gives (newest first).
pub fn parse_list(json: &str) -> Result<Vec<Release>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("GitHub's answer wasn't readable: {e}"))?;
    let list = v.as_array().ok_or("GitHub's answer wasn't a list of releases")?;
    Ok(list.iter().filter_map(parse_one).collect())
}

fn parse_one(v: &Value) -> Option<Release> {
    if v.get("draft").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let version = parse_version(&tag)?;
    let page = v.get("html_url").and_then(Value::as_str).filter(|u| u.starts_with(RELEASE_PAGE_PREFIX))?.to_string();
    let mut notes: String = v.get("body").and_then(Value::as_str).unwrap_or("").to_string();
    if notes.len() > MAX_NOTES {
        let mut cut = MAX_NOTES;
        while !notes.is_char_boundary(cut) {
            cut -= 1;
        }
        notes.truncate(cut);
    }
    let assets = v
        .get("assets")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(MAX_ASSETS)
                .filter_map(|a| {
                    let name = a.get("name")?.as_str()?.to_string();
                    let url = a.get("browser_download_url")?.as_str()?.to_string();
                    // Only this project's own release downloads, nothing a release could point elsewhere.
                    if !url.starts_with(DOWNLOAD_PREFIX) || name.contains(['/', '\\']) || name.starts_with('.') {
                        return None;
                    }
                    Some(Asset { name, url, size: a.get("size").and_then(Value::as_u64).unwrap_or(0) })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release { tag, version, notes, page, assets })
}

/// The release to offer a copy running `current`: the newest one with a higher version, unless it is `skipped`.
pub fn offer(list: &[Release], current: Version, skipped: Option<Version>) -> Option<Release> {
    let newest = list.iter().filter(|r| r.version > current).max_by_key(|r| r.version)?;
    (Some(newest.version) != skipped).then(|| newest.clone())
}

/// `SHA256SUMS.txt`: lines of `<64 hex> [ *]<name>`.
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (hash, name) = l.trim().split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (hash.to_ascii_lowercase(), name.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Release> {
        let json = r#"[
          {"tag_name":"v0.5.0-alpha","draft":true,"html_url":"https://github.com/jackharvest/shapecraft/releases/tag/v0.5.0-alpha","assets":[]},
          {"tag_name":"v0.4.1-alpha","html_url":"https://github.com/jackharvest/shapecraft/releases/tag/v0.4.1-alpha","body":"Fixed things",
           "assets":[
             {"name":"shapecraft-0.4.1-windows-x64-portable.zip","size":5,"browser_download_url":"https://github.com/jackharvest/shapecraft/releases/download/v0.4.1-alpha/shapecraft-0.4.1-windows-x64-portable.zip"},
             {"name":"evil.zip","size":5,"browser_download_url":"https://example.com/evil.zip"},
             {"name":"SHA256SUMS.txt","size":5,"browser_download_url":"https://github.com/jackharvest/shapecraft/releases/download/v0.4.1-alpha/SHA256SUMS.txt"}]},
          {"tag_name":"v0.4.0-alpha","html_url":"https://github.com/jackharvest/shapecraft/releases/tag/v0.4.0-alpha","assets":[]},
          {"tag_name":"nightly","html_url":"https://github.com/jackharvest/shapecraft/releases/tag/nightly","assets":[]},
          {"tag_name":"v9.0.0","html_url":"https://github.com/someone-else/shapecraft/releases/tag/v9.0.0","assets":[]}
        ]"#;
        parse_list(json).unwrap_or_default()
    }

    #[test]
    fn versions_ignore_the_stage_suffix() {
        assert_eq!(parse_version("v0.4.1-alpha"), Some((0, 4, 1)));
        assert_eq!(parse_version("0.10.2"), Some((0, 10, 2)));
        assert_eq!(parse_version("0.4"), None);
        assert_eq!(parse_version("0.4.1.2"), None);
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn drafts_foreign_pages_and_foreign_assets_are_left_out() {
        let l = list();
        assert_eq!(l.iter().map(|r| r.tag.as_str()).collect::<Vec<_>>(), ["v0.4.1-alpha", "v0.4.0-alpha"]);
        assert_eq!(l[0].assets.len(), 2);
        assert!(l[0].asset_ending("-windows-x64-portable.zip").is_some());
        assert!(l[0].asset_named(crate::SUMS).is_some());
    }

    #[test]
    fn offers_only_something_newer_and_not_skipped() {
        let l = list();
        assert_eq!(offer(&l, (0, 4, 0), None).map(|r| r.tag), Some("v0.4.1-alpha".into()));
        assert_eq!(offer(&l, (0, 4, 1), None), None);
        assert_eq!(offer(&l, (0, 5, 0), None), None);
        assert_eq!(offer(&l, (0, 3, 1), Some((0, 4, 1))), None);
    }

    #[test]
    fn sums_lines() {
        let h = "a".repeat(64);
        let text = format!("{h}  shapecraft-0.4.1-linux-x86_64.AppImage\n{h} *b.zip\nnot a line\n");
        assert_eq!(parse_sums(&text), vec![(h.clone(), "shapecraft-0.4.1-linux-x86_64.AppImage".into()), (h, "b.zip".into())]);
    }
}
