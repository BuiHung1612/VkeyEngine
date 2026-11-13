//! Per-app exclusion list. Remote desktop clients are excluded by default: the remote
//! machine runs its own input method, and the backspaces + Unicode characters we would
//! send do not survive the trip (the client forwards key codes, so the remote side gets
//! garbage, then its own input method adds marks again). "!app" in the list turns that
//! default off for one client (e.g. RDP to a machine without a Vietnamese input method).

/// Remote desktop clients: macOS bundle ids and Windows exe names, lower case.
const REMOTE_APPS: &[&str] = &[
    "com.teamviewer.teamviewer",
    "com.philandro.anydesk",
    "com.microsoft.rdc.macos",
    "com.microsoft.rdc.mac",
    "com.apple.screensharing",
    "com.realvnc.vncviewer",
    "com.carriez.flutterhbb", // RustDesk
    "tv.parsec.www",
    "mstsc.exe",
    "msrdc.exe",
    "teamviewer.exe",
    "anydesk.exe",
    "ultraviewer_desktop.exe",
    "rustdesk.exe",
    "vncviewer.exe",
    "parsecd.exe",
];

/// Shown when a remote desktop client comes to the front with Vietnamese on.
pub const REMOTE_NOTE: &str = "Đang remote: máy này không gõ dấu";

pub fn is_remote(app: &str) -> bool {
    REMOTE_APPS.iter().any(|r| r.eq_ignore_ascii_case(app))
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
    }
}
