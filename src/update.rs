//! Update check against the GitHub releases of the project. Uses the system `curl`
//! (macOS, Windows 10+), so there is no HTTP / JSON dependency: the few fields needed are
//! read straight from the API's JSON.

use std::path::Path;
use std::process::Command;

pub const REPO: &str = "BuiHung1612/VkeyEngine";
pub const RELEASES_PAGE: &str = "https://github.com/BuiHung1612/VkeyEngine/releases/latest";

#[cfg(windows)]
const CURL: &str = "curl.exe";
#[cfg(not(windows))]
const CURL: &str = "/usr/bin/curl";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// "1.0.2" (tag without the leading v)
    pub version: String,
    /// download URL of the asset for this platform
    pub asset: Option<String>,
}

fn curl() -> Command {
    let mut c = Command::new(CURL);
    c.args(["-fsSL", "--max-time", "60", "-H", "User-Agent: VKeyEngine"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing
    }
    c
}

/// Latest published release; `asset_name` is the file to download for this platform.
pub fn latest(asset_name: &str) -> Result<Release, String> {
    let out = curl()
        .args(["-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    parse_release(&String::from_utf8_lossy(&out.stdout), asset_name).ok_or_else(|| "phản hồi không hợp lệ".into())
}

fn parse_release(json: &str, asset_name: &str) -> Option<Release> {
    let version = json_strings(json, "tag_name").into_iter().next()?.trim_start_matches('v').to_string();
    let asset = json_strings(json, "browser_download_url").into_iter().find(|u| u.ends_with(&format!("/{asset_name}")));
    Some(Release { version, asset })
}

/// Every string value of `key` in a JSON text (values here never contain escapes).
fn json_strings(json: &str, key: &str) -> Vec<String> {
    let pat = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(i) = rest.find(&pat) {
        rest = rest[i + pat.len()..].trim_start();
        let Some(r) = rest.strip_prefix(':') else { continue };
        let r = r.trim_start();
        if let Some(r) = r.strip_prefix('"') {
            if let Some(end) = r.find('"') {
                out.push(r[..end].to_string());
            }
        }
        rest = r;
    }
    out
}

/// `remote` is a higher version than `current` ("1.0.10" > "1.0.9").
pub fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(remote) > parse(current)
}

pub fn download(url: &str, dest: &Path) -> Result<(), String> {
    let status = curl().arg("-o").arg(dest).arg(url).status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tải về thất bại ({status})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("1.0.2", "1.0.1"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(is_newer("2.0", "1.9.9"));
        assert!(!is_newer("1.0.1", "1.0.1"));
        assert!(!is_newer("1.0.0", "1.0.1"));
    }

    #[test]
    fn reads_the_release_json() {
        let json = r#"{
          "tag_name": "v1.0.2",
          "assets": [
            {"name": "VKeyEngine-macos.zip",
             "browser_download_url": "https://github.com/x/y/releases/download/v1.0.2/VKeyEngine-macos.zip"},
            {"name": "VKeyEngine-windows-x64.exe",
             "browser_download_url":"https://github.com/x/y/releases/download/v1.0.2/VKeyEngine-windows-x64.exe"}
          ]
        }"#;
        let r = parse_release(json, "VKeyEngine-windows-x64.exe").unwrap();
        assert_eq!(r.version, "1.0.2");
        assert_eq!(r.asset.as_deref(), Some("https://github.com/x/y/releases/download/v1.0.2/VKeyEngine-windows-x64.exe"));
        assert_eq!(parse_release(json, "other.zip").unwrap().asset, None);
    }
}
