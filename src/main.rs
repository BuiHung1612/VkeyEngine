// no console window on Windows
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "macos")]
    vkeyengine::mac::run();
    #[cfg(windows)]
    vkeyengine::win::run();
}
