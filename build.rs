// Windows: embed the app icon and the manifest (common controls v6, per-monitor DPI).
fn main() {
    println!("cargo:rerun-if-changed=windows");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("windows/app.ico").set_manifest_file("windows/VKeyEngine.manifest");
        res.set("ProductName", "VKeyEngine").set("FileDescription", "VKeyEngine - Bộ gõ tiếng Việt");
        res.compile().expect("windows resources");
    }
}
