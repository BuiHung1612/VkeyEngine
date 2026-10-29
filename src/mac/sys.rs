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
    fn IsSecureEventInputEnabled() -> bool;
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

/// Label of the launch agent in Contents/Library/LaunchAgents (see build-macos.sh).
pub const AGENT_LABEL: &str = "com.vkeyengine.app.agent";

/// SMAppService (macOS 13+) for the launch agent, or for the plain login item.
fn service(agent: bool) -> *mut AnyObject {
    let Some(cls) = AnyClass::get(c"SMAppService") else { return std::ptr::null_mut() };
    unsafe {
        if agent {
            let plist = NSString::from_str(&format!("{AGENT_LABEL}.plist"));
            msg_send![cls, agentServiceWithPlistName: &*plist]
        } else {
            msg_send![cls, mainAppService]
        }
    }
}

/// SMAppServiceStatus: 0 not registered, 1 enabled, 2 requires approval, 3 not found.
fn status(service: *mut AnyObject) -> isize {
    unsafe { msg_send![service, status] }
}

/// Start at login. Through the launch agent when possible: it also restarts the app if it
/// is killed (memory pressure) or crashes, but not after Quit (a successful exit). An ad-hoc
/// signed build gets the plain login item instead (no restart).
pub fn set_run_at_login(on: bool) {
    let agent = can_use_launch_agent();
    set_registered(service(true), on && agent);
    set_registered(service(false), on && !agent);
}

fn set_registered(service: *mut AnyObject, on: bool) {
    if service.is_null() {
        return;
    }
    let registered = matches!(status(service), 1 | 2); // enabled, or waiting for approval
    if on == registered {
        return;
    }
    let mut err: *mut AnyObject = std::ptr::null_mut();
    unsafe {
        let _: bool = if on {
            msg_send![service, registerAndReturnError: &mut err]
        } else {
            msg_send![service, unregisterAndReturnError: &mut err]
        };
    }
}

/// launchd refuses to start an ad-hoc signed app as an agent ("Launch Constraint
/// Violation"), so only a build signed with a real certificate can use it.
pub fn can_use_launch_agent() -> bool {
    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecCodeCopySelf(flags: u32, code: *mut *const c_void) -> i32;
        fn SecCodeCopyStaticCode(code: *const c_void, flags: u32, static_code: *mut *const c_void) -> i32;
        fn SecCodeCopySigningInformation(code: *const c_void, flags: u32, info: *mut *const c_void) -> i32;
    }
    const SIGNING_INFORMATION: u32 = 1 << 1; // kSecCSSigningInformation
    const AD_HOC: u32 = 0x0002; // kSecCodeSignatureAdhoc
    unsafe {
        let (mut code, mut stat, mut info) = (std::ptr::null(), std::ptr::null(), std::ptr::null());
        let mut flags = None;
        if SecCodeCopySelf(0, &mut code) == 0 {
            if SecCodeCopyStaticCode(code, 0, &mut stat) == 0 {
                if SecCodeCopySigningInformation(stat, SIGNING_INFORMATION, &mut info) == 0 && !info.is_null() {
                    let dict = &*(info as *const NSDictionary<NSString, AnyObject>);
                    flags = dict.objectForKey(&NSString::from_str("flags")).and_then(|f| f.downcast::<NSNumber>().ok()).map(|n| n.unsignedIntValue());
                    CFRelease(info);
                }
                CFRelease(stat);
            }
            CFRelease(code);
        }
        flags.is_some_and(|f| f & AD_HOC == 0)
    }
}

/// Registered and allowed to run (SMAppServiceStatusEnabled).
pub fn launch_agent_enabled() -> bool {
    let agent = service(true);
    !agent.is_null() && status(agent) == 1
}

/// This process was started by the launch agent.
pub fn under_launch_agent() -> bool {
    std::env::var("XPC_SERVICE_NAME").as_deref() == Ok(AGENT_LABEL)
}

/// Ask launchd to start the agent copy of the app (no-op if it already runs).
pub fn start_launch_agent() -> bool {
    extern "C" {
        fn getuid() -> u32;
    }
    let target = format!("gui/{}/{AGENT_LABEL}", unsafe { getuid() });
    std::process::Command::new("/bin/launchctl").args(["kickstart", &target]).status().is_ok_and(|s| s.success())
}

/// Name of the app that turned on Secure Input (a password field, Terminal's "Secure
/// Keyboard Entry"...). While it is on, macOS gives keystrokes to no input method.
pub fn secure_input_owner() -> Option<String> {
    if !unsafe { IsSecureEventInputEnabled() } {
        return None;
    }
    let name = (|| {
        let session = objc2_core_graphics::CGSessionCopyCurrentDictionary()?;
        let session: &NSDictionary<NSString, AnyObject> =
            unsafe { &*(objc2_core_foundation::CFRetained::as_ptr(&session).as_ptr() as *const _) };
        let pid = session.objectForKey(&NSString::from_str("kCGSSessionSecureInputPID"))?.downcast::<NSNumber>().ok()?;
        let app = objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid.intValue())?;
        app.localizedName().map(|s| s.to_string())
    })();
    Some(name.unwrap_or_else(|| "Một ứng dụng".into()))
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

/// CPU time used by this process, and its memory footprint (the number Activity Monitor shows).
pub fn process_usage() -> (std::time::Duration, u64) {
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    /// struct rusage_info_v0 (<libproc.h>)
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfo {
        uuid: [u8; 16],
        user_time: u64,
        system_time: u64,
        pkg_idle_wkups: u64,
        interrupt_wkups: u64,
        pageins: u64,
        wired_size: u64,
        resident_size: u64,
        phys_footprint: u64,
        proc_start_abstime: u64,
        proc_exit_abstime: u64,
    }
    extern "C" {
        fn clock_gettime(clock: u32, tp: *mut Timespec) -> i32;
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut c_void) -> i32;
        fn getpid() -> i32;
    }
    const CLOCK_PROCESS_CPUTIME_ID: u32 = 12;
    const RUSAGE_INFO_V0: i32 = 0;
    let mut t = Timespec { sec: 0, nsec: 0 };
    let mut info = RusageInfo::default();
    unsafe {
        clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut t);
        proc_pid_rusage(getpid(), RUSAGE_INFO_V0, &mut info as *mut RusageInfo as *mut c_void);
    }
    (std::time::Duration::new(t.sec as u64, t.nsec as u32), info.phys_footprint)
}

pub fn beep() {
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        fn NSBeep();
    }
    unsafe { NSBeep() }
}
