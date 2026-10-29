//! Small system helpers (accessibility, input source, login item, windows).

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> *const c_void;
    fn TISGetInputSourceProperty(source: *const c_void, key: *const c_void) -> *const c_void;
    static kTISPropertyInputSourceLanguages: *const c_void;
    fn CFRelease(cf: *const c_void);
}

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}

/// Accessibility permission (needed for the event tap). `prompt` opens the system dialog.
pub fn is_trusted(prompt: bool) -> bool {
    unsafe {
        if !prompt {
            return AXIsProcessTrustedWithOptions(std::ptr::null());
        }
        let key = &*(kAXTrustedCheckOptionPrompt as *const NSString);
        let yes = NSNumber::new_bool(true);
        let dict = NSDictionary::from_slices(&[key], &[&*yes]);
        AXIsProcessTrustedWithOptions(Retained::as_ptr(&dict) as *const c_void)
    }
}

/// True if the current keyboard input source is an English (or unknown) layout.
/// Fix: the Objective-C code over-released the language string (crash) and leaked the source.
pub fn input_source_is_english() -> bool {
    unsafe {
        let src = TISCopyCurrentKeyboardInputSource();
        if src.is_null() {
            return true;
        }
        let langs = TISGetInputSourceProperty(src, kTISPropertyInputSourceLanguages) as *const NSArray<NSString>;
        let english = match langs.as_ref().and_then(|l| l.firstObject()) {
            Some(first) => first.to_string().starts_with("en"),
            None => true,
        };
        CFRelease(src);
        english
    }
}

/// Start at login via SMAppService (macOS 13+).
pub fn set_run_on_startup(on: bool) {
    let Some(cls) = AnyClass::get(c"SMAppService") else { return };
    unsafe {
        let service: *mut AnyObject = msg_send![cls, mainAppService];
        if service.is_null() {
            return;
        }
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let _: bool = if on {
            msg_send![service, registerAndReturnError: &mut err]
        } else {
            msg_send![service, unregisterAndReturnError: &mut err]
        };
    }
}

/// Spotlight search panel is on screen (it is not the frontmost app, so check windows).
/// The window list scan costs a few ms, so a result is reused for half a second: typing a
/// word asks once instead of on every rewritten key.
pub fn is_spotlight_visible() -> bool {
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    thread_local! {
        static CACHE: Cell<Option<(Instant, bool)>> = const { Cell::new(None) };
    }
    if let Some((at, v)) = CACHE.get() {
        if at.elapsed() < Duration::from_millis(500) {
            return v;
        }
    }
    let v = scan_for_spotlight();
    CACHE.set(Some((Instant::now(), v)));
    v
}

fn scan_for_spotlight() -> bool {
    use objc2_core_graphics::{CGWindowListCopyWindowInfo, CGWindowListOption};
    let opts = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(opts, 0) else { return false };
    let list: &NSArray<NSDictionary<NSString, AnyObject>> = unsafe { &*(objc2_core_foundation::CFRetained::as_ptr(&list).as_ptr() as *const _) };
    let owner = NSString::from_str("kCGWindowOwnerName");
    list.iter().any(|w| {
        w.objectForKey(&owner)
            .and_then(|o| o.downcast::<NSString>().ok())
            .is_some_and(|s| s.to_string() == "Spotlight")
    })
}

pub fn beep() {
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        fn NSBeep();
    }
    unsafe { NSBeep() }
}
