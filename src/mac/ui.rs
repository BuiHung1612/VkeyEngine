//! Menu bar item and windows, built in code (no storyboard).
//!
//! Every checkbox / popup is described by a row in `TOGGLES` / `POPUPS` (NSUserDefaults key +
//! label). One generic action writes the preference, then the app re-reads its settings and
//! every control is refreshed from the preferences, so there is no per-option glue code.
//!
//! Look: monochrome. Pure white / black windows with transparent title bars, options grouped
//! in rounded cards (label left, control right). The black / white accent color for switches,
//! default buttons and selections comes from `AccentColor` in `macos/Assets.xcassets`.

use super::{hotkey, prefs, read_settings, sys, with_app, DEFAULT_SWITCH_KEY, EMPTY_HOTKEY};
use crate::convert::convert;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::cell::RefCell;
use std::ptr::NonNull;

// ---------------------------------------------------------------- tags

const T_LANG: isize = 1;
const T_INPUT: isize = 10; // + input type
const T_CODE: isize = 20; // + code table
const T_CONVERT_TOOL: isize = 30;
const T_QUICK_CONVERT: isize = 31;
const T_SETTINGS: isize = 32;
const T_MACROS: isize = 33;
const T_ABOUT: isize = 34;
const T_QUIT: isize = 35;
const T_PAGE: isize = 36;
const T_CHECK_UPDATE: isize = 37;
const T_TOGGLE: isize = 100; // + index in TOGGLES
const T_POPUP: isize = 200; // + index in POPUPS
const T_RECORD_SWITCH: isize = 300;
const T_RECORD_CONVERT: isize = 301;
const T_DEFAULTS: isize = 302;
const T_MACRO_ADD: isize = 400;
const T_MACRO_DEL: isize = 401;
const T_MACRO_IMPORT: isize = 402;
const T_MACRO_EXPORT: isize = 403;
const T_CONVERT_RUN: isize = 500;
const T_CONVERT_SWAP: isize = 501;
const T_CLOSE: isize = 502;

const INPUT_TYPES: [&str; 4] = ["Telex", "VNI", "Simple Telex 1", "Simple Telex 2"];
const CODE_TABLES: [&str; 5] = ["Unicode dựng sẵn", "TCVN3 (ABC)", "VNI Windows", "Unicode tổ hợp", "Vietnamese Locale CP 1258"];

/// Card each toggle is shown in
const P_SPELL: u8 = 0;
const P_QUICK: u8 = 1;
const P_MACRO: u8 = 2;
const P_STARTUP: u8 = 3;
const P_SMART: u8 = 4;
const P_COMPAT: u8 = 5;
const P_CONVERT: u8 = 6;
const P_SWITCH_KEY: u8 = 7;
const P_CONVERT_KEY: u8 = 8;

struct Toggle {
    key: &'static str,
    /// 0: the key is a bool; otherwise the bit to flip in an int key
    bit: i64,
    default: i64,
    title: &'static str,
    page: u8,
}

const fn t(key: &'static str, default: bool, title: &'static str, page: u8) -> Toggle {
    Toggle { key, bit: 0, default: default as i64, title, page }
}

const fn hk(key: &'static str, bit: i64, default: i64, title: &'static str, page: u8) -> Toggle {
    Toggle { key, bit, default, title, page }
}

static TOGGLES: &[Toggle] = &[
    t("Spelling", true, "Kiểm tra chính tả", P_SPELL),
    t("RestoreIfInvalidWord", false, "Tự khôi phục phím gõ nếu từ sai chính tả", P_SPELL),
    t("vTempOffSpelling", false, "Tạm tắt kiểm tra chính tả bằng phím ⌃", P_SPELL),
    t("vAllowConsonantZFWJ", false, "Cho phép phụ âm đầu Z, F, W, J", P_SPELL),
    t("ModernOrthography", false, "Đặt dấu kiểu mới (oà, uý thay vì òa, úy)", P_SPELL),
    t("QuickTelex", false, "Gõ nhanh: cc→ch gg→gi kk→kh nn→ng qq→qu pp→ph tt→th", P_QUICK),
    t("vQuickStartConsonant", false, "Gõ tắt phụ âm đầu: f→ph j→gi w→qu", P_QUICK),
    t("vQuickEndConsonant", false, "Gõ tắt phụ âm cuối: g→ng h→nh k→ch", P_QUICK),
    t("UpperCaseFirstChar", false, "Tự viết hoa chữ cái đầu câu", P_QUICK),
    t("UseMacro", true, "Bật gõ tắt", P_MACRO),
    t("UseMacroInEnglishMode", false, "Gõ tắt cả khi đang ở chế độ tiếng Anh", P_MACRO),
    t("vAutoCapsMacro", false, "Tự viết hoa theo từ gõ tắt (btw → Btw → BTW)", P_MACRO),
    t("RunOnStartup", true, "Khởi động cùng hệ thống", P_STARTUP),
    t("ShowUIOnStartup", false, "Hiện bảng điều khiển khi khởi động", P_STARTUP),
    t("CheckUpdate", true, "Tự kiểm tra bản mới", P_STARTUP),
    t("vShowIconOnDock", false, "Hiện biểu tượng trên Dock", P_STARTUP),
    t("UseSmartSwitchKey", true, "Tự nhớ chế độ gõ Việt / Anh theo từng ứng dụng", P_SMART),
    t("vRememberCode", true, "Tự nhớ bảng mã theo từng ứng dụng", P_SMART),
    t("vOtherLanguage", true, "Tắt tiếng Việt khi dùng bàn phím ngôn ngữ khác", P_SMART),
    t("vTempOffOpenKey", false, "Tạm tắt VKeyEngine bằng phím ⌘", P_SMART),
    t("FixRecommendBrowser", true, "Sửa lỗi gợi ý từ (thanh địa chỉ trình duyệt, Excel...)", P_COMPAT),
    t("vFixChromiumBrowser", false, "Sửa lỗi gợi ý cho Chrome / Edge / Brave (thử nghiệm)", P_COMPAT),
    t("SendKeyStepByStep", false, "Gửi từng phím một (cho ứng dụng cũ)", P_COMPAT),
    t("vPerformLayoutCompat", false, "Tương thích bố cục bàn phím khác (AZERTY, Dvorak...)", P_COMPAT),
    t("convertToolRemoveMark", false, "Bỏ dấu", P_CONVERT),
    t("convertToolDontAlertWhenCompleted", false, "Thông báo khi chuyển mã xong", P_CONVERT),
    hk("SwitchKeyStatus", hotkey::CONTROL, DEFAULT_SWITCH_KEY, "⌃", P_SWITCH_KEY),
    hk("SwitchKeyStatus", hotkey::OPTION, DEFAULT_SWITCH_KEY, "⌥", P_SWITCH_KEY),
    hk("SwitchKeyStatus", hotkey::COMMAND, DEFAULT_SWITCH_KEY, "⌘", P_SWITCH_KEY),
    hk("SwitchKeyStatus", hotkey::SHIFT, DEFAULT_SWITCH_KEY, "⇧", P_SWITCH_KEY),
    hk("SwitchKeyStatus", hotkey::BEEP, DEFAULT_SWITCH_KEY, "Âm báo khi chuyển chế độ", P_SWITCH_KEY),
    hk("convertToolHotKey", hotkey::CONTROL, EMPTY_HOTKEY, "⌃", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::OPTION, EMPTY_HOTKEY, "⌥", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::COMMAND, EMPTY_HOTKEY, "⌘", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::SHIFT, EMPTY_HOTKEY, "⇧", P_CONVERT_KEY),
];

/// Keys that need a value even when 0 means "unset".
fn int_pref(key: &str, default: i64) -> i64 {
    match prefs::int(key, default) {
        0 if default > 1 => default,
        v => v,
    }
}

fn toggle_on(t: &Toggle) -> bool {
    if t.bit == 0 {
        prefs::flag(t.key, t.default != 0)
    } else {
        int_pref(t.key, t.default) & t.bit != 0
    }
}

struct Popup {
    /// "" = convert case options (4 exclusive bool keys)
    key: &'static str,
    label: &'static str,
    items: &'static [&'static str],
    default: i64,
}

const CASE_KEYS: [&str; 4] =
    ["convertToolToAllCaps", "convertToolToAllNonCaps", "convertToolToCapsFirstLetter", "convertToolToCapsEachWord"];

static POPUPS: &[Popup] = &[
    Popup { key: "InputMethod", label: "Chế độ gõ", items: &["Tiếng Anh", "Tiếng Việt"], default: 1 },
    Popup { key: "InputType", label: "Kiểu gõ", items: &INPUT_TYPES, default: 0 },
    Popup { key: "CodeTable", label: "Bảng mã", items: &CODE_TABLES, default: 0 },
    Popup { key: "convertToolFromCode", label: "Từ bảng mã", items: &CODE_TABLES, default: 0 },
    Popup { key: "convertToolToCode", label: "Sang bảng mã", items: &CODE_TABLES, default: 0 },
    Popup {
        key: "",
        label: "Chữ hoa / thường",
        items: &["Giữ nguyên", "VIẾT HOA TOÀN BỘ", "viết thường toàn bộ", "Viết hoa chữ đầu câu", "Viết Hoa Chữ Đầu Mỗi Từ"],
        default: 0,
    },
];
const POP_LANG: usize = 0;
const POP_INPUT: usize = 1;
const POP_CODE: usize = 2;
const POP_FROM: usize = 3;
const POP_TO: usize = 4;
const POP_CASE: usize = 5;

fn popup_value(i: usize) -> i64 {
    let p = &POPUPS[i];
    if p.key.is_empty() {
        return CASE_KEYS.iter().position(|k| prefs::flag(k, false)).map_or(0, |i| i as i64 + 1);
    }
    prefs::int(p.key, p.default)
}

fn set_popup_value(i: usize, v: i64) {
    let p = &POPUPS[i];
    if p.key.is_empty() {
        for (n, k) in CASE_KEYS.iter().enumerate() {
            prefs::set_int(k, (v == n as i64 + 1) as i64);
        }
    } else {
        prefs::set_int(p.key, v);
    }
}

// ---------------------------------------------------------------- UI state

/// Why typing may not work right now, shown in the menu.
#[derive(Default, Clone, PartialEq)]
struct Health {
    no_permission: bool,
    /// app that turned on Secure Input
    secure_input: Option<String>,
}

#[derive(Default)]
struct Ui {
    status_item: Option<Retained<NSStatusItem>>,
    menu_switch: Option<Retained<NSSwitch>>,
    menu_title: Option<Retained<NSTextField>>,
    menu_status: Option<Retained<NSTextField>>,
    menu_input_holder: Option<Retained<NSMenuItem>>,
    menu_input: Vec<Retained<NSMenuItem>>,
    menu_code: Vec<Retained<NSMenuItem>>,
    menu_quick_convert: Option<Retained<NSMenuItem>>,
    /// switches and modifier buttons
    toggles: Vec<(usize, Retained<NSControl>)>,
    popups: Vec<(usize, Retained<NSPopUpButton>)>,
    record_switch: Vec<Retained<NSButton>>,
    record_convert: Vec<Retained<NSButton>>,
    settings: Option<Retained<NSWindow>>,
    /// settings pages with their heights, and the height of the header above them
    pages: Vec<(Retained<NSView>, f64)>,
    pages_top: f64,
    page: usize,
    /// "Hoạt động" page: value labels (see ACTIVITY_ROWS) and the CPU graph
    activity_values: Vec<Retained<NSTextField>>,
    cpu_graph: Option<Retained<GraphView>>,
    macros: Option<Retained<NSWindow>>,
    convert: Option<Retained<NSWindow>>,
    about: Option<Retained<NSWindow>>,
    macro_table: Option<Retained<NSTableView>>,
    macro_name: Option<Retained<NSTextField>>,
    macro_content: Option<Retained<NSTextField>>,
    macro_add: Option<Retained<NSButton>>,
    key_monitor: Option<Retained<AnyObject>>,
    watchdog: Option<Retained<NSTimer>>,
    health: Health,
}

thread_local! {
    static UI: RefCell<Ui> = RefCell::new(Ui::default());
    static DELEGATE: RefCell<Option<Retained<Delegate>>> = const { RefCell::new(None) };
}

/// Borrow the UI state. Never call into AppKit from `f` in a way that can call back into
/// our delegate (reloadData, makeFirstResponder...): the nested `ui()` would panic, and a
/// panic aborts the app. Clone the handles out and call AppKit after.
fn ui<R>(f: impl FnOnce(&mut Ui) -> R) -> R {
    UI.with(|u| f(&mut u.borrow_mut()))
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("UI must run on the main thread")
}

fn delegate() -> Retained<Delegate> {
    DELEGATE.with(|d| d.borrow().clone()).expect("delegate")
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

// ---------------------------------------------------------------- delegate class

#[derive(Default)]
pub struct DelegateIvars {
    /// macro table rows (text, content)
    rows: RefCell<Vec<(String, String)>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    pub struct Delegate;

    impl Delegate {
        #[unsafe(method(onAction:))]
        fn on_action(&self, sender: &AnyObject) {
            let tag: isize = unsafe { msg_send![sender, tag] };
            action(tag, sender);
        }

        #[unsafe(method(didWake:))]
        fn did_wake(&self, _n: &NSNotification) {
            start_tap();
        }

        #[unsafe(method(willSleep:))]
        fn will_sleep(&self, _n: &NSNotification) {
            with_app(|a| a.tap.stop());
        }

        #[unsafe(method(spaceChanged:))]
        fn space_changed(&self, _n: &NSNotification) {
            with_app(|a| {
                a.engine.start_new_session();
                a.tap.clear_sync();
            });
        }

        #[unsafe(method(appActivated:))]
        fn app_activated(&self, _n: &NSNotification) {
            with_app(|a| a.on_active_app_changed());
        }

        #[unsafe(method(inputSourceChanged:))]
        fn input_source_changed(&self, _n: &NSNotification) {
            let english = sys::input_source_is_english();
            with_app(|a| a.english_input_source = english);
        }
    }

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _n: &NSNotification) {
            did_finish_launching();
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self, _n: &NSNotification) {
            // turned off while running as the agent: unregistering earlier would have killed us
            if !prefs::flag("RunOnStartup", true) {
                sys::set_run_at_login(false);
            }
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            show_settings();
            true
        }
    }

    unsafe impl NSTableViewDataSource for Delegate {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _tv: &NSTableView) -> isize {
            self.ivars().rows.borrow().len() as isize
        }

        #[unsafe(method_id(tableView:objectValueForTableColumn:row:))]
        fn object_value(&self, _tv: &NSTableView, col: Option<&NSTableColumn>, row: isize) -> Option<Retained<AnyObject>> {
            let rows = self.ivars().rows.borrow();
            let is_name = col.is_some_and(|c| c.identifier().to_string() == "name");
            rows.get(row as usize)
                .map(|(text, content)| Retained::into_super(Retained::into_super(ns(if is_name { text } else { content }))))
        }
    }

    unsafe impl NSTableViewDelegate for Delegate {
        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, _n: &NSNotification) {
            let row = ui(|u| u.macro_table.as_ref().map_or(-1, |t| t.selectedRow()));
            let Some((text, content)) = self.ivars().rows.borrow().get(row.max(0) as usize).cloned() else { return };
            if row < 0 {
                return;
            }
            ui(|u| {
                if let (Some(n), Some(c), Some(b)) = (&u.macro_name, &u.macro_content, &u.macro_add) {
                    n.setStringValue(&ns(&text));
                    c.setStringValue(&ns(&content));
                    b.setTitle(&ns("Sửa"));
                }
            });
        }
    }

    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, _n: &NSNotification) {
            let rows = self.ivars().rows.borrow();
            ui(|u| {
                if let (Some(n), Some(b)) = (&u.macro_name, &u.macro_add) {
                    let name = n.stringValue().to_string();
                    let exists = rows.iter().any(|(t, _)| *t == name);
                    b.setTitle(&ns(if exists { "Sửa" } else { "Thêm" }));
                }
            });
        }
    }

    unsafe impl NSTextFieldDelegate for Delegate {}
);

define_class!(
    /// Top-down coordinates, so windows are laid out from the top.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct FlippedView;

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

define_class!(
    /// CPU usage over the last minutes: a line with a light fill under it.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Vec<f32>>]
    struct GraphView;

    impl GraphView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let values = self.ivars().borrow();
            if values.len() < 2 {
                return;
            }
            let b = self.bounds();
            let (w, h) = (b.size.width, b.size.height);
            // at least 5 % full scale, so idle noise stays flat
            let max = values.iter().fold(5.0f32, |m, &v| m.max(v)) as f64;
            let step = w / (crate::stats::HISTORY - 1) as f64;
            let x0 = w - step * (values.len() - 1) as f64; // newest sample on the right
            let point = |i: usize, v: f32| NSPoint::new(x0 + step * i as f64, 4.0 + (h - 8.0) * v as f64 / max);
            let line = NSBezierPath::bezierPath();
            for (i, &v) in values.iter().enumerate() {
                if i == 0 {
                    line.moveToPoint(point(i, v));
                } else {
                    line.lineToPoint(point(i, v));
                }
            }
            let fill = line.copy();
            fill.lineToPoint(NSPoint::new(w, 0.0));
            fill.lineToPoint(NSPoint::new(x0, 0.0));
            fill.closePath();
            NSColor::labelColor().colorWithAlphaComponent(0.1).setFill();
            fill.fill();
            line.setLineWidth(1.5);
            line.setLineJoinStyle(NSLineJoinStyle::Round);
            NSColor::labelColor().setStroke();
            line.stroke();
        }
    }
);

impl GraphView {
    fn new(frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm()).set_ivars(RefCell::new(Vec::new()));
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars::default());
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------- startup

/// One VKeyEngine per user session (several users can each run their own). The launch agent
/// copy waits a little: it may start while the copy that handed over to it is still exiting.
pub fn acquire_single_instance_lock() -> bool {
    let tries = if sys::under_launch_agent() { 50 } else { 1 };
    (0..tries).any(|i| {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        try_lock()
    })
}

/// Another copy holds the lock within 3 s (the launch agent copy we just started).
pub fn wait_for_other_instance() -> bool {
    (0..30).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        lock_file().is_some_and(|f| f.try_lock().is_err()) // dropping `f` releases a lock we got
    })
}

fn lock_file() -> Option<std::fs::File> {
    let home = std::env::var_os("HOME")?;
    let dir = std::path::Path::new(&home).join("Library/Application Support/VKeyEngine");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("vkeyengine.lock")).ok()
}

fn try_lock() -> bool {
    let Some(file) = lock_file() else { return true };
    if file.try_lock().is_err() {
        return false;
    }
    std::mem::forget(file); // keep the lock for the process lifetime
    true
}

pub fn run_app() {
    let mtm = mtm();
    let app = NSApplication::sharedApplication(mtm);
    set_dock_icon(prefs::flag("vShowIconOnDock", false));
    let d = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*d)));
    DELEGATE.with(|x| *x.borrow_mut() = Some(d));
    app.run();
}

fn did_finish_launching() {
    keep_responsive();
    create_status_item();
    register_notifications();
    with_app(|a| {
        a.on_active_app_changed();
    });
    let at_login = prefs::flag("RunOnStartup", true);
    // turning it off while running as the agent waits for Quit (see applicationWillTerminate)
    if at_login || !sys::under_launch_agent() {
        sys::set_run_at_login(at_login);
    }
    start_watchdog();
    start_update_checks();
    if !start_tap() {
        sys::is_trusted(true);
        alert(
            "VKeyEngine cần quyền Trợ năng (Accessibility) để hoạt động",
            "Hãy bật VKeyEngine trong Cài đặt hệ thống → Quyền riêng tư & Bảo mật → Trợ năng. \
             VKeyEngine sẽ tự hoạt động ngay khi được cấp quyền.",
        );
    } else if prefs::flag("ShowUIOnStartup", false) {
        show_settings();
    }
}

/// Opt out of App Nap and ask for low latency. Without this, a menu bar app with no window
/// is treated as idle background work: under memory pressure it is throttled (the key
/// callback misses its deadline and macOS disables the tap) and is among the first to be
/// killed. The activity lasts for the whole process.
fn keep_responsive() {
    let options = NSActivityOptions::UserInitiatedAllowingIdleSystemSleep | NSActivityOptions::LatencyCritical;
    let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &ns("Xử lý bàn phím"));
    std::mem::forget(token);
}

fn start_tap() -> bool {
    if !sys::is_trusted(false) {
        return false;
    }
    with_app(|a| a.tap.start()).unwrap_or(false)
}

/// Every 2 s: keep the key tap alive whatever happened (permission granted late or revoked,
/// tap port invalidated, wake notification missed...) and keep the menu's warning current.
fn start_watchdog() {
    let block = RcBlock::new(|_t: NonNull<NSTimer>| watchdog());
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(2.0, true, &block) };
    timer.setTolerance(1.0); // let macOS coalesce the wake-ups
    ui(|u| u.watchdog = Some(timer));
}

fn watchdog() {
    let trusted = sys::is_trusted(false);
    with_app(|a| {
        if !trusted {
            a.tap.stop();
        } else if a.tap.keep_alive() {
            a.engine.start_new_session(); // keys typed meanwhile never reached the engine
            a.tap.clear_sync();
        }
    });
    let health = Health { no_permission: !trusted, secure_input: sys::secure_input_owner() };
    if ui(|u| std::mem::replace(&mut u.health, health.clone()) != health) {
        refresh();
    }
    let (cpu, memory) = sys::process_usage();
    crate::stats::sample(cpu, memory);
    update_activity();
}

fn register_notifications() {
    let d = delegate();
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let observe = |center: &NSNotificationCenter, s: Sel, name: &NSNotificationName| unsafe {
        center.addObserver_selector_name_object(&d, s, Some(name), None);
    };
    unsafe {
        observe(&center, sel!(didWake:), NSWorkspaceDidWakeNotification);
        observe(&center, sel!(willSleep:), NSWorkspaceWillSleepNotification);
        observe(&center, sel!(spaceChanged:), NSWorkspaceActiveSpaceDidChangeNotification);
        observe(&center, sel!(appActivated:), NSWorkspaceDidActivateApplicationNotification);
    }
    let dist = NSDistributedNotificationCenter::defaultCenter();
    let name = ns("com.apple.Carbon.TISNotifySelectedKeyboardInputSourceChanged");
    unsafe { dist.addObserver_selector_name_object(&d, sel!(inputSourceChanged:), Some(&name), None) };
}

/// Run `f` on the main run loop after the current event (e.g. after the tap callback).
pub fn defer(f: fn()) {
    let block = RcBlock::new(move |_t: NonNull<NSTimer>| f());
    unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.0, false, &block) };
}

fn set_dock_icon(show: bool) {
    let policy = if show { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
    NSApplication::sharedApplication(mtm()).setActivationPolicy(policy);
}

// ---------------------------------------------------------------- status menu

fn menu_item(title: &str, tag: isize, key: &str) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm()), &ns(title), Some(sel!(onAction:)), &ns(key))
    };
    unsafe { item.setTarget(Some(&delegate())) };
    item.setTag(tag);
    item
}

fn submenu(parent: &NSMenu, title: &str, items: &[Retained<NSMenuItem>]) -> Retained<NSMenuItem> {
    let mtm = mtm();
    let sub = NSMenu::initWithTitle(NSMenu::alloc(mtm), &ns(""));
    sub.setAutoenablesItems(false);
    for i in items {
        sub.addItem(i);
    }
    let holder = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &ns(title), None, &ns("")) };
    holder.setSubmenu(Some(&sub));
    parent.addItem(&holder);
    holder
}

/// SF Symbol shown before a menu item title.
fn with_symbol(item: Retained<NSMenuItem>, symbol: &str) -> Retained<NSMenuItem> {
    item.setImage(NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns(symbol), None).as_deref());
    item
}

fn section_header(menu: &NSMenu, title: &str) {
    let cls = NSMenuItem::class();
    let native: bool = unsafe { msg_send![cls, respondsToSelector: sel!(sectionHeaderWithTitle:)] }; // macOS 14+
    let item = if native {
        NSMenuItem::sectionHeaderWithTitle(&ns(title), mtm())
    } else {
        let i = menu_item(title, 0, "");
        i.setEnabled(false);
        i
    };
    menu.addItem(&item);
}

/// Top of the menu: app icon, current mode, and a Vietnamese / English switch.
fn menu_header() -> Retained<NSMenuItem> {
    let mtm = mtm();
    let (w, h) = (290.0, 62.0);
    let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));
    if let Some(icon) = NSApplication::sharedApplication(mtm).applicationIconImage() {
        let v = NSImageView::imageViewWithImage(&icon, mtm);
        v.setFrame(rect(12.0, 13.0, 36.0, 36.0));
        view.addSubview(&v);
    }
    let sw = NSSwitch::new(mtm);
    unsafe {
        sw.setTarget(Some(&delegate()));
        sw.setAction(Some(sel!(onAction:)));
    }
    sw.setTag(T_LANG);
    let s = size_to_fit(&sw);
    sw.setFrameOrigin(NSPoint::new(w - 14.0 - s.width, (h - s.height) / 2.0));
    view.addSubview(&sw);
    let title = text("", 13.0, true, false);
    title.setFrame(rect(56.0, 32.0, w - 70.0 - s.width, 18.0));
    view.addSubview(&title);
    let status = text("", 11.0, false, true);
    status.setFrame(rect(56.0, 14.0, w - 70.0 - s.width, 15.0));
    status.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    view.addSubview(&status);

    let item = NSMenuItem::new(mtm);
    item.setView(Some(&view));
    ui(|u| {
        u.menu_switch = Some(sw);
        u.menu_title = Some(title);
        u.menu_status = Some(status);
    });
    item
}

fn create_status_item() {
    let mtm = mtm();
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &ns(""));
    menu.setAutoenablesItems(false);

    menu.addItem(&menu_header());
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let inputs: Vec<_> = INPUT_TYPES.iter().enumerate().map(|(i, s)| menu_item(s, T_INPUT + i as isize, "")).collect();
    let input_holder = with_symbol(submenu(&menu, "Kiểu gõ", &inputs), "keyboard");
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    section_header(&menu, "Bảng mã");
    let codes: Vec<_> = CODE_TABLES.iter().enumerate().map(|(i, s)| menu_item(s, T_CODE + i as isize, "")).collect();
    for c in &codes[..3] {
        menu.addItem(c);
    }
    submenu(&menu, "Bảng mã khác", &codes[3..]);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&with_symbol(menu_item("Công cụ chuyển mã…", T_CONVERT_TOOL, ""), "arrow.left.arrow.right"));
    let quick = with_symbol(menu_item("Chuyển mã nhanh", T_QUICK_CONVERT, ""), "bolt");
    menu.addItem(&quick);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&with_symbol(menu_item("Bảng điều khiển…", T_SETTINGS, ","), "gearshape"));
    menu.addItem(&with_symbol(menu_item("Gõ tắt…", T_MACROS, ""), "text.badge.plus"));
    menu.addItem(&with_symbol(menu_item("Kiểm tra bản mới…", T_CHECK_UPDATE, ""), "arrow.down.circle"));
    menu.addItem(&with_symbol(menu_item("Giới thiệu", T_ABOUT, ""), "info.circle"));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&with_symbol(menu_item("Thoát VKeyEngine", T_QUIT, "q"), "power"));
    item.setMenu(Some(&menu));

    ui(|u| {
        u.status_item = Some(item);
        u.menu_input_holder = Some(input_holder);
        u.menu_input = inputs;
        u.menu_code = codes;
        u.menu_quick_convert = Some(quick);
    });
    refresh();
}

fn on_off(b: bool) -> isize {
    if b {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

/// Update the menu and all open windows from the preferences.
pub fn refresh() {
    let (s, o) = read_settings();
    let spelling = s.check_spelling;
    ui(|u| {
        if let Some(button) = u.status_item.as_ref().and_then(|i| i.button(mtm())) {
            let name = if s.language == 1 { "Status" } else { "StatusEng" };
            if let Some(img) = NSImage::imageNamed(&ns(name)) {
                img.setTemplate(true); // black glyph, recolored by macOS for light / dark menu bars
                button.setImage(Some(&img));
            } else {
                button.setTitle(&ns(if s.language == 1 { "V" } else { "E" }));
            }
            // dimmed while keys cannot be processed (see the menu header for why)
            button.setAppearsDisabled(u.health != Health::default());
        }
        let vietnamese = s.language == 1;
        if let Some(m) = &u.menu_switch {
            m.setState(on_off(vietnamese));
        }
        let input = INPUT_TYPES.get(s.input_type as usize).copied().unwrap_or_default();
        if let Some(t) = &u.menu_title {
            t.setStringValue(&ns(if vietnamese { "Tiếng Việt" } else { "Tiếng Anh" }));
        }
        if let Some(t) = &u.menu_status {
            let code = CODE_TABLES.get(s.code_table as usize).copied().unwrap_or_default();
            let key = hotkey::describe(o.switch_key);
            let line = if u.health.no_permission {
                "⚠ Chưa có quyền Trợ năng (Accessibility)".to_string()
            } else if let Some(app) = &u.health.secure_input {
                format!("⚠ {app} đang chặn bàn phím (Secure Input)")
            } else if key.is_empty() {
                format!("{input} · {code}")
            } else {
                format!("{input} · {code} · {key}")
            };
            t.setStringValue(&ns(&line));
        }
        if let Some(m) = &u.menu_input_holder {
            m.setTitle(&ns(&format!("Kiểu gõ: {input}")));
        }
        for (i, m) in u.menu_input.iter().enumerate() {
            m.setState(on_off(i == s.input_type as usize));
        }
        for (i, m) in u.menu_code.iter().enumerate() {
            m.setState(on_off(i == s.code_table as usize));
        }
        if let Some(m) = &u.menu_quick_convert {
            let h = hotkey::describe(o.convert_hotkey);
            m.setTitle(&ns(&if h.is_empty() { "Chuyển mã nhanh".into() } else { format!("Chuyển mã nhanh - [{h}]") }));
        }
        for (i, b) in &u.toggles {
            let tg = &TOGGLES[*i];
            let _: () = unsafe { msg_send![&**b, setState: on_off(toggle_on(tg))] };
            let needs_spelling = matches!(tg.key, "RestoreIfInvalidWord" | "vTempOffSpelling" | "vAllowConsonantZFWJ");
            b.setEnabled(!needs_spelling || spelling);
            if tg.key == "vFixChromiumBrowser" {
                b.setEnabled(o.fix_recommend_browser);
            }
        }
        for (i, p) in &u.popups {
            p.selectItemAtIndex(popup_value(*i) as isize);
        }
        for b in &u.record_switch {
            b.setTitle(&ns(&key_title(o.switch_key)));
        }
        for b in &u.record_convert {
            b.setTitle(&ns(&key_title(o.convert_hotkey)));
        }
    });
}

fn key_title(h: i64) -> String {
    match hotkey::key(h) {
        k if k == hotkey::NO_KEY as u16 => "(không phím)".into(),
        crate::tables::KEY_SPACE => "Space".into(),
        _ => (((h >> 24) & 0xFF) as u8 as char).to_uppercase().to_string(),
    }
}

// ---------------------------------------------------------------- actions

fn action(tag: isize, sender: &AnyObject) {
    match tag {
        T_LANG => {
            with_app(|a| {
                let l = 1 - a.engine.s.language.min(1);
                a.set_language(l);
            });
        }
        T_INPUT..=13 => {
            prefs::set_int("InputType", (tag - T_INPUT) as i64);
            reload();
        }
        T_CODE..=24 => {
            let t = (tag - T_CODE) as u8;
            with_app(|a| a.set_code_table(t));
            refresh();
        }
        T_CONVERT_TOOL => show_convert(),
        T_QUICK_CONVERT | T_CONVERT_RUN => quick_convert_with_alert(tag == T_CONVERT_RUN),
        T_SETTINGS => show_settings(),
        T_PAGE => {
            let i: isize = unsafe { msg_send![sender, selectedSegment] };
            show_page(i.max(0) as usize);
        }
        T_MACROS => show_macros(),
        T_ABOUT => show_about(),
        T_CHECK_UPDATE => {
            std::thread::spawn(|| check_update(true));
        }
        T_QUIT => NSApplication::sharedApplication(mtm()).terminate(None),
        T_DEFAULTS => reset_defaults(),
        T_RECORD_SWITCH | T_RECORD_CONVERT => record_hotkey(tag, sender),
        T_MACRO_ADD => macro_add(),
        T_MACRO_DEL => macro_delete(),
        T_MACRO_IMPORT => macro_import(),
        T_MACRO_EXPORT => macro_export(),
        T_CONVERT_SWAP => {
            let (from, to) = (popup_value(POP_FROM), popup_value(POP_TO));
            set_popup_value(POP_FROM, to);
            set_popup_value(POP_TO, from);
            refresh();
        }
        T_CLOSE => {
            let w: Option<Retained<NSWindow>> = unsafe { msg_send![sender, window] };
            if let Some(w) = w {
                w.close();
            }
        }
        _ if (T_TOGGLE..T_POPUP).contains(&tag) => {
            let Some(tg) = TOGGLES.get((tag - T_TOGGLE) as usize) else { return };
            let state: isize = unsafe { msg_send![sender, state] };
            let on = state == NSControlStateValueOn;
            if tg.bit == 0 {
                prefs::set_int(tg.key, on as i64);
            } else {
                let v = int_pref(tg.key, tg.default);
                prefs::set_int(tg.key, if on { v | tg.bit } else { v & !tg.bit });
            }
            match tg.key {
                // turning it off while running as the agent waits for Quit (see applicationWillTerminate)
                "RunOnStartup" if on || !sys::under_launch_agent() => sys::set_run_at_login(on),
                "vShowIconOnDock" => set_dock_icon(on),
                _ => {}
            }
            reload();
        }
        _ if (T_POPUP..T_RECORD_SWITCH).contains(&tag) => {
            let i = (tag - T_POPUP) as usize;
            let v: isize = unsafe { msg_send![sender, indexOfSelectedItem] };
            set_popup_value(i, v as i64);
            reload();
        }
        _ => {}
    }
}

/// Preferences changed: apply them to the engine and refresh the UI.
fn reload() {
    with_app(|a| a.reload_settings());
    refresh();
}

fn reset_defaults() {
    if !confirm("Bạn có chắc chắn muốn thiết lập lại cấu hình mặc định?", "") {
        return;
    }
    for tg in TOGGLES.iter().filter(|t| t.bit == 0) {
        prefs::set_int(tg.key, tg.default);
    }
    prefs::set_int("SwitchKeyStatus", DEFAULT_SWITCH_KEY);
    prefs::set_int("convertToolHotKey", EMPTY_HOTKEY);
    for p in POPUPS {
        if !p.key.is_empty() {
            prefs::set_int(p.key, p.default);
        }
    }
    set_popup_value(POP_CASE, 0);
    sys::set_run_at_login(true);
    set_dock_icon(false);
    reload();
}

fn record_hotkey(tag: isize, sender: &AnyObject) {
    let button: &NSButton = unsafe { &*(sender as *const AnyObject as *const NSButton) };
    button.setTitle(&ns("Nhấn một phím..."));
    let key = if tag == T_RECORD_SWITCH { "SwitchKeyStatus" } else { "convertToolHotKey" };
    let default = if tag == T_RECORD_SWITCH { DEFAULT_SWITCH_KEY } else { EMPTY_HOTKEY };
    stop_recording();
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        let ev = unsafe { ev.as_ref() };
        let code = ev.keyCode();
        let ch = ev.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
        let (k, c): (i64, i64) = match code {
            49 => (49, 49),               // space
            51 | 117 => (0xFE, 0xFE),     // delete: modifiers only
            53 => (-1, -1),               // escape: cancel
            _ => (code as i64, ch.bytes().next().map_or(0, |b| b.to_ascii_lowercase() as i64)),
        };
        if k >= 0 {
            let v = int_pref(key, default);
            prefs::set_int(key, (v & 0x00FF_FF00) | k | (c << 24));
        }
        stop_recording();
        reload();
        std::ptr::null_mut()
    });
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    ui(|u| u.key_monitor = monitor);
}

fn stop_recording() {
    if let Some(m) = ui(|u| u.key_monitor.take()) {
        unsafe { NSEvent::removeMonitor(&m) };
    }
}

// ---------------------------------------------------------------- alerts

fn activate() {
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm()).activateIgnoringOtherApps(true);
}

fn alert(msg: &str, info: &str) {
    activate();
    let a = NSAlert::new(mtm());
    a.setMessageText(&ns(msg));
    a.setInformativeText(&ns(info));
    a.addButtonWithTitle(&ns("OK"));
    a.runModal();
}

fn confirm(msg: &str, info: &str) -> bool {
    activate();
    let a = NSAlert::new(mtm());
    a.setMessageText(&ns(msg));
    a.setInformativeText(&ns(info));
    a.addButtonWithTitle(&ns("Có"));
    a.addButtonWithTitle(&ns("Không"));
    a.runModal() == NSAlertFirstButtonReturn
}

// ---------------------------------------------------------------- quick convert

/// Convert the clipboard (plain text and HTML) with the convert tool options.
pub fn quick_convert() {
    quick_convert_with_alert(false);
}

fn quick_convert_with_alert(always_alert: bool) {
    let (_, o) = read_settings();
    let pb = NSPasteboard::generalPasteboard();
    let html = unsafe { pb.stringForType(NSPasteboardTypeHTML) }.map(|s| convert(&s.to_string(), &o.convert));
    let text = unsafe { pb.stringForType(NSPasteboardTypeString) }.map(|s| convert(&s.to_string(), &o.convert));
    if html.is_none() && text.is_none() {
        alert("Không có dữ liệu trong clipboard!", "Hãy sao chép một đoạn văn bản để chuyển mã.");
        return;
    }
    pb.clearContents();
    if let Some(h) = html {
        unsafe { pb.setString_forType(&ns(&h), NSPasteboardTypeHTML) };
    }
    if let Some(t) = text {
        unsafe { pb.setString_forType(&ns(&t), NSPasteboardTypeString) };
    }
    if o.convert_alert || always_alert {
        alert("Chuyển mã thành công!", "Kết quả đã được lưu trong clipboard.");
    }
}

// ---------------------------------------------------------------- window building

const PAD: f64 = 24.0;
const ROW_H: f64 = 36.0;
/// space reserved for the (transparent) title bar
const TITLEBAR: f64 = 36.0;
/// inner padding of a card
const INSET: f64 = 14.0;

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// Gray that is `light` in light mode and `dark` in dark mode (0 = black, 1 = white).
fn mono(light: f64, dark: f64) -> Retained<NSColor> {
    // the two concrete colors live for the whole process
    let leak = |w: f64| NonNull::new(Retained::into_raw(NSColor::colorWithWhite_alpha(w, 1.0))).expect("color");
    let (l, d) = (leak(light), leak(dark));
    let block = RcBlock::new(move |a: NonNull<NSAppearance>| {
        if unsafe { a.as_ref() }.name().to_string().contains("Dark") {
            d
        } else {
            l
        }
    });
    unsafe { NSColor::colorWithName_dynamicProvider(None, &block) }
}

fn flipped(w: f64, h: f64) -> Retained<NSView> {
    let v: Retained<FlippedView> = unsafe { msg_send![FlippedView::alloc(mtm()), initWithFrame: rect(0.0, 0.0, w, h)] };
    Retained::into_super(v)
}

fn text(s: &str, size: f64, bold: bool, secondary: bool) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&ns(s), mtm());
    let font = if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    l.setFont(Some(&font));
    if secondary {
        l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    }
    l
}

fn size_to_fit(v: &NSView) -> NSSize {
    let s = v.fittingSize();
    v.setFrameSize(s);
    s
}

fn button(title: &str, tag: isize) -> Retained<NSButton> {
    let b = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), Some(&delegate()), Some(sel!(onAction:)), mtm()) };
    b.setTag(tag);
    b
}

fn switch(i: usize) -> Retained<NSView> {
    let s = NSSwitch::new(mtm());
    s.setControlSize(NSControlSize::Small);
    unsafe {
        s.setTarget(Some(&delegate()));
        s.setAction(Some(sel!(onAction:)));
    }
    s.setTag(T_TOGGLE + i as isize);
    size_to_fit(&s);
    let c = Retained::into_super(s);
    ui(|u| u.toggles.push((i, c.clone())));
    Retained::into_super(c)
}

fn toggle_index(key: &str, bit: i64) -> usize {
    TOGGLES.iter().position(|t| t.key == key && t.bit == bit).expect("toggle")
}

fn popup(i: usize, w: f64) -> Retained<NSView> {
    let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm()), rect(0.0, 0.0, w, 24.0), false);
    let items: Vec<Retained<NSString>> = POPUPS[i].items.iter().map(|s| ns(s)).collect();
    p.addItemsWithTitles(&NSArray::from_retained_slice(&items));
    unsafe {
        p.setTarget(Some(&delegate()));
        p.setAction(Some(sel!(onAction:)));
    }
    p.setTag(T_POPUP + i as isize);
    p.setFrameSize(NSSize::new(w, p.fittingSize().height));
    ui(|u| u.popups.push((i, p.clone())));
    Retained::into_super(Retained::into_super(Retained::into_super(p)))
}

/// ⌃ ⌥ ⌘ ⇧ toggle buttons + key recorder
fn hotkey_control(page: u8, record_tag: isize) -> Retained<NSView> {
    let mut views = Vec::new();
    for (i, tg) in TOGGLES.iter().enumerate().filter(|(_, t)| t.page == page && t.bit != hotkey::BEEP) {
        let b = button(tg.title, T_TOGGLE + i as isize);
        b.setButtonType(NSButtonType::PushOnPushOff);
        b.setControlSize(NSControlSize::Small);
        let s = size_to_fit(&b);
        b.setFrameSize(NSSize::new(32.0, s.height));
        let c = Retained::into_super(b);
        ui(|u| u.toggles.push((i, c.clone())));
        views.push(Retained::into_super(c));
    }
    let rec = button("", record_tag);
    rec.setControlSize(NSControlSize::Small);
    let s = size_to_fit(&rec);
    rec.setFrameSize(NSSize::new(104.0, s.height));
    ui(|u| if record_tag == T_RECORD_SWITCH { u.record_switch.push(rec.clone()) } else { u.record_convert.push(rec.clone()) });
    views.push(Retained::into_super(Retained::into_super(rec)));

    let h = views.iter().map(|v| v.frame().size.height).fold(0.0, f64::max);
    let w = views.iter().map(|v| v.frame().size.width + 4.0).sum::<f64>() + 4.0;
    let container = NSView::initWithFrame(NSView::alloc(mtm()), rect(0.0, 0.0, w, h));
    let mut x = 0.0;
    for (n, v) in views.iter().enumerate() {
        x += if n + 1 == views.len() { 8.0 } else { 0.0 };
        v.setFrameOrigin(NSPoint::new(x, (h - v.frame().size.height) / 2.0));
        container.addSubview(v);
        x += v.frame().size.width + 4.0;
    }
    container
}

fn text_field(placeholder: &str, w: f64) -> Retained<NSView> {
    let f = NSTextField::textFieldWithString(&ns(""), mtm());
    f.setPlaceholderString(Some(&ns(placeholder)));
    f.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);
    f.setFrameSize(NSSize::new(w, 24.0));
    unsafe { f.setDelegate(Some(ProtocolObject::from_ref(&*delegate()))) };
    Retained::into_super(Retained::into_super(f))
}

/// Rounded card, the building block of every window.
fn card(x: f64, y: f64, w: f64, h: f64) -> Retained<NSBox> {
    let b = NSBox::initWithFrame(NSBox::alloc(mtm()), rect(x, y, w, h));
    b.setBoxType(NSBoxType::Custom);
    b.setTitlePosition(NSTitlePosition::NoTitle);
    b.setContentViewMargins(NSSize::ZERO);
    b.setCornerRadius(10.0);
    b.setBorderWidth(1.0);
    b.setBorderColor(&mono(0.88, 0.2));
    b.setFillColor(&mono(0.965, 0.08));
    b
}

/// Top-down layout of a window or settings page.
struct Page {
    view: Retained<NSView>,
    y: f64,
    w: f64,
}

impl Page {
    fn new(w: f64) -> Page {
        Page { view: flipped(w, 0.0), y: 0.0, w }
    }

    fn add(&self, v: &NSView) {
        self.view.addSubview(v);
    }

    /// Big title + one line of description, below the title bar.
    fn header(&mut self, title: &str, subtitle: &str) {
        self.y = TITLEBAR;
        let t = text(title, 22.0, true, false);
        t.setFrame(rect(PAD, self.y, self.w - 2.0 * PAD, 28.0));
        self.add(&t);
        let s = text(subtitle, 12.0, false, true);
        s.setFrame(rect(PAD, self.y + 30.0, self.w - 2.0 * PAD, 16.0));
        self.add(&s);
        self.y += 56.0;
    }

    fn section(&mut self, title: &str) {
        self.y += 14.0;
        let l = text(&title.to_uppercase(), 11.0, true, true);
        l.setFrame(rect(PAD + INSET, self.y, self.w - 2.0 * PAD, 14.0));
        self.add(&l);
        self.y += 20.0;
    }

    /// One row per (label, control), separated by hairlines; controls are right-aligned.
    fn card(&mut self, rows: Vec<(&str, Retained<NSView>)>) {
        let (w, h) = (self.w - 2.0 * PAD, ROW_H * rows.len() as f64);
        let card = card(PAD, self.y, w, h);
        let Some(content) = card.contentView() else { return };
        for (i, (label, control)) in rows.iter().enumerate() {
            let top = h - ROW_H * i as f64; // the card itself is not flipped
            if i > 0 {
                let line = NSBox::initWithFrame(NSBox::alloc(mtm()), rect(INSET, top, w - 2.0 * INSET, 1.0));
                line.setBoxType(NSBoxType::Separator);
                content.addSubview(&line);
            }
            let cs = control.frame().size;
            control.setFrameOrigin(NSPoint::new(w - INSET - cs.width, top - ROW_H + (ROW_H - cs.height) / 2.0));
            let l = text(label, 13.0, false, false);
            l.setFrame(rect(INSET, top - ROW_H + (ROW_H - 17.0) / 2.0, w - 3.0 * INSET - cs.width, 17.0));
            l.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            content.addSubview(&l);
            content.addSubview(control);
        }
        self.add(&card);
        self.y += h;
    }

    fn toggle_card(&mut self, page: u8) {
        let rows = TOGGLES.iter().enumerate().filter(|(_, t)| t.page == page).map(|(i, t)| (t.title, switch(i))).collect();
        self.card(rows);
    }

    fn note(&mut self, s: &str) {
        self.y += 10.0;
        let l = text(s, 12.0, false, true);
        l.setFrame(rect(PAD + INSET, self.y, self.w - 2.0 * (PAD + INSET), 16.0));
        self.add(&l);
        self.y += 16.0;
    }

    /// Buttons aligned left and right; `primary` (if any) becomes the default (Return) button.
    fn buttons(&mut self, left: &[(&str, isize)], right: &[(&str, isize)], primary: isize) -> Vec<Retained<NSButton>> {
        self.y += 18.0;
        let mut all = Vec::new();
        let mut place = |items: &mut dyn Iterator<Item = &(&str, isize)>, mut x: f64, dir: f64| {
            for (title, tag) in items {
                let b = button(title, *tag);
                if *tag == primary {
                    b.setKeyEquivalent(&ns("\r"));
                }
                let s = size_to_fit(&b);
                let w = s.width.max(80.0);
                b.setFrame(rect(if dir < 0.0 { x - w } else { x }, self.y, w, s.height));
                self.view.addSubview(&b);
                x += dir * (w + 8.0);
                all.push(b);
            }
        };
        place(&mut left.iter(), PAD, 1.0);
        place(&mut right.iter().rev(), self.w - PAD, -1.0);
        self.y += all.first().map_or(0.0, |b| b.frame().size.height);
        all
    }

    /// Fix the page height to its content.
    fn finish(&mut self) -> f64 {
        self.y += PAD;
        self.view.setFrameSize(NSSize::new(self.w, self.y));
        self.y
    }
}

fn new_window(title: &str, page: &mut Page) -> Retained<NSWindow> {
    let h = page.finish();
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::FullSizeContentView;
    let win = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm()),
            rect(0.0, 0.0, page.w, h),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { win.setReleasedWhenClosed(false) };
    win.setTitle(&ns(title));
    win.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    win.setTitlebarAppearsTransparent(true);
    win.setMovableByWindowBackground(true);
    win.setBackgroundColor(Some(&mono(1.0, 0.0)));
    win.setContentView(Some(&page.view));
    win.center();
    win
}

/// Resize the window's content, keeping its top edge in place.
fn resize_content(win: &NSWindow, w: f64, h: f64) {
    let old = win.frame();
    let mut f = win.frameRectForContentRect(rect(0.0, 0.0, w, h));
    f.origin = NSPoint::new(old.origin.x, old.origin.y + old.size.height - f.size.height);
    win.setFrame_display_animate(f, true, win.isVisible());
}

fn present(win: &NSWindow) {
    activate();
    win.setLevel(NSFloatingWindowLevel);
    win.makeKeyAndOrderFront(None);
}

fn version() -> String {
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&ns("CFBundleShortVersionString"))
        .and_then(|v| v.downcast::<NSString>().ok())
        .map_or_else(|| env!("CARGO_PKG_VERSION").to_string(), |s| s.to_string())
}

// ---------------------------------------------------------------- windows

pub fn show_settings() {
    if let Some(w) = ui(|u| u.settings.clone()) {
        refresh();
        present(&w);
        return;
    }
    let w = 580.0;
    let mut root = Page::new(w);
    root.header("VKeyEngine", &format!("Bộ gõ tiếng Việt · phiên bản {}", version()));
    let labels: Vec<Retained<NSString>> = ["Bộ gõ", "Gõ tắt", "Hệ thống", "Hoạt động"].iter().map(|s| ns(s)).collect();
    let seg = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &NSArray::from_retained_slice(&labels),
            NSSegmentSwitchTracking::SelectOne,
            Some(&delegate()),
            Some(sel!(onAction:)),
            mtm(),
        )
    };
    seg.setTag(T_PAGE);
    seg.setSelectedSegment(0);
    let s = size_to_fit(&seg);
    seg.setFrameOrigin(NSPoint::new(w - PAD - s.width, TITLEBAR + 4.0));
    root.add(&seg);

    let build: [&dyn Fn(&mut Page); 4] = [
        &|p| {
            p.section("Bộ gõ");
            p.card(vec![
                (POPUPS[POP_LANG].label, popup(POP_LANG, 200.0)),
                (POPUPS[POP_INPUT].label, popup(POP_INPUT, 200.0)),
                (POPUPS[POP_CODE].label, popup(POP_CODE, 200.0)),
                ("Phím chuyển Việt / Anh", hotkey_control(P_SWITCH_KEY, T_RECORD_SWITCH)),
                (TOGGLES[toggle_index("SwitchKeyStatus", hotkey::BEEP)].title, switch(toggle_index("SwitchKeyStatus", hotkey::BEEP))),
            ]);
            p.section("Chính tả");
            p.toggle_card(P_SPELL);
            p.section("Gõ nhanh");
            p.toggle_card(P_QUICK);
        },
        &|p| {
            p.section("Gõ tắt");
            p.toggle_card(P_MACRO);
            p.note("Ví dụ: gõ \"vn\" rồi nhấn phím cách để được \"Việt Nam\".");
            p.buttons(&[], &[("Bảng gõ tắt…", T_MACROS)], 0);
        },
        &|p| {
            p.section("Khởi động & giao diện");
            p.toggle_card(P_STARTUP);
            p.section("Tự động chuyển");
            p.toggle_card(P_SMART);
            p.section("Tương thích");
            p.toggle_card(P_COMPAT);
            p.buttons(&[("Khôi phục mặc định", T_DEFAULTS)], &[("Thoát VKeyEngine", T_QUIT)], 0);
        },
        &|p| {
            p.section("CPU · 2 phút gần nhất");
            let (cw, ch) = (p.w - 2.0 * PAD, 96.0);
            let graph_card = card(PAD, p.y, cw, ch);
            let graph = GraphView::new(rect(INSET, 8.0, cw - 2.0 * INSET, ch - 16.0));
            if let Some(c) = graph_card.contentView() {
                c.addSubview(&graph);
            }
            p.add(&graph_card);
            p.y += ch;
            ui(|u| u.cpu_graph = Some(graph));
            let rows = |p: &mut Page, range: std::ops::Range<usize>| {
                p.card(ACTIVITY_ROWS[range].iter().map(|&label| (label, activity_value())).collect());
            };
            p.section("Tài nguyên");
            rows(p, 0..3);
            p.section("Bộ gõ");
            rows(p, 3..7);
            p.note("Cập nhật mỗi 2 giây. Khi để không, VKeyEngine gần như không dùng CPU.");
        },
    ];
    let top = root.y;
    let pages: Vec<_> = build
        .iter()
        .map(|b| {
            let mut p = Page::new(w);
            b(&mut p);
            let h = p.finish();
            p.view.setFrameOrigin(NSPoint::new(0.0, top));
            root.add(&p.view);
            (p.view, h)
        })
        .collect();
    let win = new_window(&format!("VKeyEngine {}", version()), &mut root);
    ui(|u| {
        u.settings = Some(win.clone());
        u.pages = pages;
        u.pages_top = top;
    });
    show_page(0);
    win.center();
    refresh();
    present(&win);
}

fn show_page(i: usize) {
    let Some((win, top, h)) = ui(|u| {
        u.page = i;
        for (n, (v, _)) in u.pages.iter().enumerate() {
            v.setHidden(n != i);
        }
        Some((u.settings.clone()?, u.pages_top, u.pages.get(i)?.1))
    }) else {
        return;
    };
    resize_content(&win, win.contentView().map_or(0.0, |v| v.frame().size.width), top + h);
    update_activity();
}

const ACTIVITY_PAGE: usize = 3;
const ACTIVITY_ROWS: [&str; 7] = [
    "Bộ nhớ (RAM)",
    "CPU hiện tại",
    "Thời gian chạy",
    "Trạng thái",
    "Số phím đã xử lý",
    "Thời gian xử lý trung bình mỗi phím",
    "Chậm nhất",
];

fn activity_value() -> Retained<NSView> {
    let l = text("—", 13.0, false, true);
    l.setAlignment(NSTextAlignment::Right);
    l.setFrameSize(NSSize::new(230.0, 17.0));
    ui(|u| u.activity_values.push(l.clone()));
    Retained::into_super(Retained::into_super(l))
}

/// Fill the "Hoạt động" page, if it is on screen.
fn update_activity() {
    let Some((values, graph, health)) = ui(|u| {
        let visible = u.page == ACTIVITY_PAGE && u.settings.as_ref().is_some_and(|w| w.isVisible());
        visible.then(|| (u.activity_values.clone(), u.cpu_graph.clone(), u.health.clone()))
    }) else {
        return;
    };
    use crate::stats;
    let s = stats::snapshot();
    let vietnamese = read_settings().0.language == 1;
    let status = if health.no_permission {
        "⚠ Chưa có quyền Trợ năng".to_string()
    } else if let Some(app) = &health.secure_input {
        format!("⚠ Bị chặn bởi {app}")
    } else if vietnamese {
        "Đang hoạt động · Tiếng Việt".to_string()
    } else {
        "Đang hoạt động · Tiếng Anh".to_string()
    };
    let texts = [
        stats::memory(s.memory),
        stats::percent(s.cpu),
        stats::duration(s.uptime),
        status,
        stats::thousands(s.keys),
        stats::latency(s.avg_key),
        stats::latency(s.max_key),
    ];
    for (l, t) in values.iter().zip(texts) {
        l.setStringValue(&ns(&t));
    }
    if let Some(g) = graph {
        *g.ivars().borrow_mut() = s.cpu_history;
        g.setNeedsDisplay(true);
    }
}

pub fn show_convert() {
    if let Some(w) = ui(|u| u.convert.clone()) {
        refresh();
        present(&w);
        return;
    }
    let mut p = Page::new(520.0);
    p.header("Chuyển mã", "Chuyển văn bản trong clipboard giữa các bảng mã tiếng Việt.");
    p.section("Bảng mã");
    p.card(vec![
        (POPUPS[POP_FROM].label, popup(POP_FROM, 220.0)),
        (POPUPS[POP_TO].label, popup(POP_TO, 220.0)),
        (POPUPS[POP_CASE].label, popup(POP_CASE, 220.0)),
    ]);
    p.section("Tuỳ chọn");
    let mut rows: Vec<_> =
        TOGGLES.iter().enumerate().filter(|(_, t)| t.page == P_CONVERT).map(|(i, t)| (t.title, switch(i))).collect();
    rows.push(("Phím tắt chuyển mã nhanh", hotkey_control(P_CONVERT_KEY, T_RECORD_CONVERT)));
    p.card(rows);
    p.note("Sao chép văn bản, bấm \"Chuyển mã\", rồi dán kết quả.");
    p.buttons(&[("⇅ Đảo bảng mã", T_CONVERT_SWAP)], &[("Đóng", T_CLOSE), ("Chuyển mã", T_CONVERT_RUN)], T_CONVERT_RUN);
    let win = new_window("Công cụ chuyển mã", &mut p);
    ui(|u| u.convert = Some(win.clone()));
    refresh();
    present(&win);
}

pub fn show_about() {
    if let Some(w) = ui(|u| u.about.clone()) {
        present(&w);
        return;
    }
    let w = 360.0;
    let mut p = Page::new(w);
    p.y = TITLEBAR;
    if let Some(icon) = NSApplication::sharedApplication(mtm()).applicationIconImage() {
        let v = NSImageView::imageViewWithImage(&icon, mtm());
        v.setFrame(rect((w - 80.0) / 2.0, p.y, 80.0, 80.0));
        p.add(&v);
        p.y += 92.0;
    }
    for (s, size, bold, secondary, h) in [
        ("VKeyEngine", 20.0, true, false, 26.0),
        (&*format!("Phiên bản {}", version()), 12.0, false, true, 22.0),
        ("Bộ gõ tiếng Việt mã nguồn mở", 13.0, false, false, 18.0),
        ("Dựa trên OpenKey của Mai Vũ Tuyên · GPL v3", 12.0, false, true, 18.0),
    ] {
        let l = text(s, size, bold, secondary);
        l.setAlignment(NSTextAlignment::Center);
        l.setFrame(rect(PAD, p.y, w - 2.0 * PAD, h));
        p.add(&l);
        p.y += h;
    }
    let win = new_window("Giới thiệu VKeyEngine", &mut p);
    ui(|u| u.about = Some(win.clone()));
    present(&win);
}

pub fn show_macros() {
    if let Some(w) = ui(|u| u.macros.clone()) {
        reload_macro_rows();
        present(&w);
        return;
    }
    let mtm = mtm();
    let d = delegate();
    let w = 580.0;
    let mut p = Page::new(w);
    p.header("Gõ tắt", "Gõ từ viết tắt rồi nhấn phím cách, VKeyEngine sẽ thay bằng nội dung đầy đủ.");
    p.y += 14.0;

    let (cw, ch) = (w - 2.0 * PAD, 260.0);
    let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, cw, ch));
    for (id, title, width) in [("name", "Từ gõ tắt", 150.0), ("content", "Nội dung", cw - 170.0)] {
        let col = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &ns(id));
        col.setTitle(&ns(title));
        col.setWidth(width);
        col.setEditable(false);
        table.addTableColumn(&col);
    }
    table.setStyle(NSTableViewStyle::FullWidth);
    table.setRowHeight(24.0);
    table.setBackgroundColor(&NSColor::clearColor());
    unsafe {
        table.setDataSource(Some(ProtocolObject::from_ref(&*d)));
        table.setDelegate(Some(ProtocolObject::from_ref(&*d)));
    }
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(1.0, 1.0, cw - 2.0, ch - 2.0));
    scroll.setDocumentView(Some(&table));
    scroll.setHasVerticalScroller(true);
    scroll.setBorderType(NSBorderType::NoBorder);
    scroll.setDrawsBackground(false);
    let table_card = card(PAD, p.y, cw, ch);
    if let Some(c) = table_card.contentView() {
        c.addSubview(&scroll);
    }
    p.add(&table_card);
    p.y += ch;

    p.section("Thêm / sửa");
    let name = text_field("ví dụ: vn", 340.0);
    let content = text_field("ví dụ: Việt Nam", 340.0);
    let caps = toggle_index("vAutoCapsMacro", 0);
    p.card(vec![("Từ gõ tắt", name.clone()), ("Nội dung", content.clone()), (TOGGLES[caps].title, switch(caps))]);
    let buttons = p.buttons(
        &[("Nhập từ file…", T_MACRO_IMPORT), ("Xuất ra file…", T_MACRO_EXPORT)],
        &[("Xoá", T_MACRO_DEL), ("Thêm", T_MACRO_ADD)],
        T_MACRO_ADD,
    );
    let add = buttons.into_iter().find(|b| b.tag() == T_MACRO_ADD);
    let win = new_window("Gõ tắt", &mut p);
    ui(|u| {
        u.macros = Some(win.clone());
        u.macro_table = Some(table);
        u.macro_name = name.downcast().ok();
        u.macro_content = content.downcast().ok();
        u.macro_add = add;
    });
    reload_macro_rows();
    refresh();
    present(&win);
}

fn reload_macro_rows() {
    let rows = with_app(|a| a.engine.macros.all()).unwrap_or_default();
    *delegate().ivars().rows.borrow_mut() = rows;
    // reloadData may change the selection, which calls tableViewSelectionDidChange
    let (table, add) = ui(|u| (u.macro_table.clone(), u.macro_add.clone()));
    if let Some(t) = table {
        t.reloadData();
    }
    if let Some(b) = add {
        b.setTitle(&ns("Thêm"));
    }
}

fn macro_fields() -> (String, String) {
    ui(|u| {
        let get = |f: &Option<Retained<NSTextField>>| f.as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default();
        (get(&u.macro_name), get(&u.macro_content))
    })
}

fn clear_macro_fields() {
    let (win, name, content) = ui(|u| (u.macros.clone(), u.macro_name.clone(), u.macro_content.clone()));
    for f in [&name, &content].into_iter().flatten() {
        f.setStringValue(&ns(""));
    }
    if let (Some(w), Some(n)) = (win, name) {
        w.makeFirstResponder(Some(&n));
    }
}

fn macro_add() {
    let (name, content) = macro_fields();
    if name.is_empty() || content.is_empty() {
        alert("Gõ tắt", "Bạn hãy nhập từ gõ tắt và nội dung!");
        return;
    }
    with_app(|a| {
        let t = a.engine.s.code_table;
        a.engine.macros.add(&name, &content, t);
        a.save_macros();
    });
    clear_macro_fields();
    reload_macro_rows();
}

fn macro_delete() {
    let (name, _) = macro_fields();
    if name.is_empty() {
        alert("Gõ tắt", "Bạn hãy chọn từ cần xoá!");
        return;
    }
    with_app(|a| {
        let t = a.engine.s.code_table;
        if a.engine.macros.delete(&name, t) {
            a.save_macros();
        }
    });
    clear_macro_fields();
    reload_macro_rows();
}

fn macro_import() {
    let mtm = mtm();
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setMessage(Some(&ns("Chọn file dữ liệu gõ tắt (.txt)")));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    activate();
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URL().and_then(|u| u.path()) else { return };
    let text = match std::fs::read(path.to_string()) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return alert("Không đọc được file", &e.to_string()),
    };
    let keep = confirm("Dữ liệu gõ tắt", "Bạn có muốn giữ lại các dữ liệu hiện tại không?");
    with_app(|a| {
        let t = a.engine.s.code_table;
        a.engine.macros.import_text(&text, keep, t);
        a.save_macros();
    });
    reload_macro_rows();
}

fn macro_export() {
    let panel = NSSavePanel::savePanel(mtm());
    panel.setMessage(Some(&ns("Chọn nơi lưu dữ liệu gõ tắt")));
    panel.setNameFieldStringValue(&ns("VKeyEngineMacro.txt"));
    panel.setCanCreateDirectories(true);
    activate();
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URL().and_then(|u| u.path()) else { return };
    let text = with_app(|a| a.engine.macros.export_text()).unwrap_or_default();
    if let Err(e) = std::fs::write(path.to_string(), text) {
        alert("Không ghi được file", &e.to_string());
    }
}

// ---------------------------------------------------------------- updates

const UPDATE_ASSET: &str = "VKeyEngine-macos.zip";

enum UpdateEvent {
    Checked { manual: bool, result: Result<crate::update::Release, String> },
    /// the new app is unpacked and the swap script waits for us to quit
    Ready,
    Failed(String),
}

static UPDATE_EVENT: std::sync::Mutex<Option<UpdateEvent>> = std::sync::Mutex::new(None);

/// Run `f` on the main thread (from any thread).
fn on_main(f: fn()) {
    extern "C" {
        static _dispatch_main_q: std::ffi::c_void;
        fn dispatch_async_f(queue: *const std::ffi::c_void, context: *mut std::ffi::c_void, work: extern "C" fn(*mut std::ffi::c_void));
    }
    extern "C" fn trampoline(f: *mut std::ffi::c_void) {
        let f: fn() = unsafe { std::mem::transmute(f) };
        f();
    }
    unsafe { dispatch_async_f(&raw const _dispatch_main_q, f as *mut std::ffi::c_void, trampoline) };
}

fn post_update_event(e: UpdateEvent) {
    *UPDATE_EVENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
    on_main(handle_update_event);
}

/// 10 s after launch, then once a day, if the option is on.
fn start_update_checks() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(10));
        loop {
            if prefs::flag("CheckUpdate", true) {
                check_update(false);
            }
            std::thread::sleep(std::time::Duration::from_secs(24 * 3600));
        }
    });
}

/// Background thread: ask GitHub for the latest release.
fn check_update(manual: bool) {
    let result = crate::update::latest(UPDATE_ASSET);
    post_update_event(UpdateEvent::Checked { manual, result });
}

fn handle_update_event() {
    let Some(event) = UPDATE_EVENT.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    let current = version();
    match event {
        UpdateEvent::Checked { result: Ok(r), .. } if crate::update::is_newer(&r.version, &current) => {
            let info = format!("Bạn đang dùng bản {current}. Tải và cài đặt bản {} ngay? VKeyEngine sẽ tự khởi động lại.", r.version);
            if !confirm(&format!("Đã có VKeyEngine {}", r.version), &info) {
                return;
            }
            match r.asset {
                Some(url) => {
                    std::thread::spawn(move || install_update(&url));
                }
                None => open_releases_page(),
            }
        }
        UpdateEvent::Checked { manual: true, result: Ok(_) } => {
            alert("Bạn đang dùng bản mới nhất", &format!("VKeyEngine {current}"));
        }
        UpdateEvent::Checked { manual: true, result: Err(e) } => alert("Không kiểm tra được bản mới", &e),
        UpdateEvent::Checked { .. } => {} // automatic check: stay quiet
        UpdateEvent::Ready => NSApplication::sharedApplication(mtm()).terminate(None),
        UpdateEvent::Failed(e) => {
            alert("Không cài được bản mới", &format!("{e}\nTrang tải về sẽ được mở để bạn cài thủ công."));
            open_releases_page();
        }
    }
}

fn open_releases_page() {
    let _ = std::process::Command::new("/usr/bin/open").arg(crate::update::RELEASES_PAGE).status();
}

/// Background thread: download and unpack the new app, then start a script that swaps
/// the bundles once this process has quit and opens the new one.
fn install_update(url: &str) {
    let result = (|| -> Result<(), String> {
        let app = std::env::current_exe().map_err(|e| e.to_string())?;
        // .../VKeyEngine.app/Contents/MacOS/VKeyEngine -> .../VKeyEngine.app
        let app = app.ancestors().nth(3).filter(|p| p.extension().is_some_and(|e| e == "app")).ok_or("không tìm thấy VKeyEngine.app")?;
        let parent = app.parent().ok_or("thư mục cài đặt không hợp lệ")?;
        let probe = parent.join(".vkeyengine-update-probe");
        std::fs::write(&probe, b"").map_err(|_| format!("không ghi được vào {}", parent.display()))?;
        let _ = std::fs::remove_file(&probe);

        let dir = std::env::temp_dir().join("vkeyengine-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let zip = dir.join(UPDATE_ASSET);
        crate::update::download(url, &zip)?;
        let unpacked = std::process::Command::new("/usr/bin/ditto").arg("-x").arg("-k").arg(&zip).arg(&dir).status();
        let new_app = dir.join("VKeyEngine.app");
        if !unpacked.is_ok_and(|s| s.success()) || !new_app.join("Contents/MacOS/VKeyEngine").exists() {
            return Err("tệp tải về bị lỗi".into());
        }
        let script = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done
rm -rf "$2.old" && mv "$2" "$2.old" && mv "$3" "$2" && rm -rf "$2.old"
open "$2""#;
        use std::os::unix::process::CommandExt;
        std::process::Command::new("/bin/sh")
            .args(["-c", script, "sh"])
            .arg(std::process::id().to_string())
            .arg(app)
            .arg(&new_app)
            .process_group(0) // outlive us, even when launchd ends our process group
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    post_update_event(match result {
        Ok(()) => UpdateEvent::Ready,
        Err(e) => UpdateEvent::Failed(e),
    });
}

