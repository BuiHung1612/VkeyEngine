//! Per-app exclusion list. Remote desktop clients are excluded by default: the remote
//! machine runs its own input method, and the backspaces + Unicode characters we would
//! send do not survive the trip (the client forwards key codes, so the remote side gets
//! garbage, then its own input method adds marks again). "!app" in the list turns that
//! default off for one client (e.g. RDP to a machine without a Vietnamese input method).

/// Remote desktop clients on macOS: bundle ids (checked against Homebrew casks).
const MAC_REMOTE: &[&str] = &[
    "com.teamviewer.TeamViewer",
    "com.philandro.anydesk",
    "com.microsoft.rdc.macos", // Windows App, formerly Microsoft Remote Desktop
    "com.microsoft.rdc.mac",   // Microsoft Remote Desktop 8 (Mac App Store)
    "com.apple.ScreenSharing",
    "com.realvnc.vncviewer",
    "com.realvnc.rvncconnect", // RealVNC Connect
    "com.carriez.rustdesk",
    "tv.parsec.www",
    "com.splashtop.stb.macosx",
    "com.nomachine.nxplayer",
    "com.lemonmojo.RoyalTSX.App",
    "com.devolutions.remotedesktopmanager",
];

/// Remote desktop clients on Windows: start of the exe name, so renamed downloads
/// ("AnyDesk (1).exe", "rustdesk-1.3.0-x86_64.exe", "RoyalTS7.exe") are found too.
const WIN_REMOTE: &[&str] = &[
    "mstsc",
    "msrdc",
    "teamviewer",
    "anydesk",
    "ultraviewer",
    "rustdesk",
    "vncviewer",
    "tvnviewer", // TightVNC
    "parsecd",
    "strwinclt", // Splashtop
    "nxplayer",  // NoMachine
    "royalts",
    "remotedesktopmanager",
    "mremoteng",
    "rdcman",
];

/// Shown when a remote desktop client comes to the front with Vietnamese on.
pub const REMOTE_NOTE: &str = "Đang remote: máy này không gõ dấu";

pub fn is_remote(app: &str) -> bool {
    let lower = app.to_ascii_lowercase();
    match lower.strip_suffix(".exe") {
        Some(_) => WIN_REMOTE.iter().any(|r| lower.starts_with(r)),
        None => MAC_REMOTE.iter().any(|r| r.eq_ignore_ascii_case(app)),
    }
}

fn not(app: &str) -> String {
    format!("!{app}")
}

/// Keys go through untouched in `app`.
pub fn is_excluded(list: &[String], app: &str) -> bool {
    list.iter().any(|a| a == app) || (is_remote(app) && !list.contains(&not(app)))
}

/// Flip `app` in the list.
pub fn toggle(list: &mut Vec<String>, app: &str) {
    let on = !is_excluded(list, app);
    list.retain(|a| *a != app && *a != not(app));
    match (on, is_remote(app)) {
        (true, false) => list.push(app.to_string()),
        (false, true) => list.push(not(app)),
        _ => {}
    }
}

/// Entries to show the user (excluded apps, not the "!app" overrides).
pub fn shown(list: &[String]) -> impl Iterator<Item = &String> {
    list.iter().filter(|a| !a.starts_with('!'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_default_and_toggle() {
        let mut l = Vec::new();
        assert!(is_excluded(&l, "mstsc.exe"));
        assert!(!is_excluded(&l, "notepad.exe"));
        toggle(&mut l, "mstsc.exe");
        assert!(!is_excluded(&l, "mstsc.exe"));
        toggle(&mut l, "mstsc.exe");
        assert!(is_excluded(&l, "mstsc.exe") && l.is_empty());
        toggle(&mut l, "notepad.exe");
        assert!(is_excluded(&l, "notepad.exe"));
        toggle(&mut l, "notepad.exe");
        assert!(!is_excluded(&l, "notepad.exe") && l.is_empty());
        assert!(is_excluded(&l, "com.TeamViewer.TeamViewer"));
        assert!(is_remote("AnyDesk (1).exe") && is_remote("rustdesk-1.3.0-x86_64.exe"));
        assert!(!is_remote("anydesk") && !is_remote("chrome.exe") && !is_remote("com.philandro.anydesk.helper"));
    }
}
