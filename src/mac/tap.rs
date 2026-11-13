//! Keyboard hook: CGEventTap callback and synthetic key output. Port of OpenKey.mm.
//!
//! Fixes vs Objective-C:
//! - the tap is re-enabled when macOS disables it (timeout / user input, e.g. when the
//!   machine is swapping under memory pressure); before, Vietnamese stayed "on" but every
//!   key went through unchanged (no diacritics) until OpenKey was restarted.
//! - letter case comes from the character the OS produces, not from modifier flags, so a
//!   stuck Shift / Caps Lock state can no longer produce "cÁc bẠn" (#233 #259 #290 #320).
//! - quick-convert alert is shown after the callback returns (a modal inside the tap
//!   callback froze typing and got the tap disabled).
//! - Spotlight is detected (window list scan) only when a key is actually rewritten, and
//!   the result is cached briefly.
//! - empty sync-key stack no longer crashes (VNI / Unicode Compound backspace).

use super::{hotkey, sys, ui, with_app, App};
use crate::engine::{Code, KeyEvent, KeyState};
use crate::output::{BrowserFix, Ctx, Output, Step, Typed};
use crate::tables::*;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use std::ffi::c_void;
use std::ptr::NonNull;

const NICE_SPACE_APPS: &[&str] = &["com.sublimetext.3", "com.sublimetext.2"];
/// Apps that break with Unicode Compound (prefix match).
const UNICODE_COMPOUND_APPS: &[&str] = &[
    "com.apple.",
    "com.google.Chrome",
    "com.brave.Browser",
    "com.microsoft.edgemac.Dev",
    "com.microsoft.edgemac.Beta",
    "com.microsoft.Edge.Dev",
    "com.microsoft.Edge",
];
const CHROMIUM_APPS: &[&str] = &[
    "com.google.Chrome",
    "com.brave.Browser",
    "com.microsoft.edgemac.Dev",
    "com.microsoft.edgemac.Beta",
    "com.microsoft.Edge.Dev",
    "com.microsoft.Edge",
];
const SPOTLIGHT: &str = "com.apple.Spotlight";

thread_local! {
    /// The running tap, reachable without borrowing the app state (see `callback`).
    static PORT: std::cell::RefCell<Option<CFRetained<CFMachPort>>> = const { std::cell::RefCell::new(None) };
    /// when the tap last saw a key press
    static LAST_KEY: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// Seconds since the tap last saw a key press (counted from the first call before any).
pub fn seconds_since_key() -> f64 {
    let at = LAST_KEY.get().unwrap_or_else(|| {
        let now = std::time::Instant::now();
        LAST_KEY.set(Some(now));
        now
    });
    at.elapsed().as_secs_f64()
}

pub struct Tap {
    port: Option<CFRetained<CFMachPort>>,
    rl_source: Option<CFRetained<CFRunLoopSource>>,
    source: Option<CFRetained<CGEventSource>>,
    source_id: i64,
    proxy: CGEventTapProxy,
    out: Output,
    last_flag: u64,
    keycode: u16,
    flag: u64,
}

fn has(flags: u64, f: CGEventFlags) -> bool {
    flags & f.0 != 0
}

impl Default for Tap {
    fn default() -> Tap {
        Tap::new()
    }
}

impl Tap {
    pub fn new() -> Tap {
        let source = CGEventSource::new(CGEventSourceStateID::Private);
        let source_id = CGEventSource::source_state_id(source.as_deref()).0 as i64;
        Tap {
            port: None,
            rl_source: None,
            source,
            source_id,
            proxy: std::ptr::null_mut(),
            out: Output::default(),
            last_flag: 0,
            keycode: 0,
            flag: 0,
        }
    }

    pub fn is_running(&self) -> bool {
        self.port.is_some()
    }

    pub fn start(&mut self) -> bool {
        if self.port.is_some() {
            return true;
        }
        let mask: CGEventMask = [
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            CGEventType::LeftMouseDown,
            CGEventType::RightMouseDown,
            CGEventType::LeftMouseDragged,
            CGEventType::RightMouseDragged,
        ]
        .iter()
        .fold(0, |m, t| m | (1u64 << t.0));
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                std::ptr::null_mut(),
            )
        };
        let Some(port) = port else { return false };
        let src = CFMachPort::new_run_loop_source(None, Some(&port), 0);
        if let Some(rl) = CFRunLoop::main() {
            rl.add_source(src.as_deref(), unsafe { kCFRunLoopCommonModes });
        }
        CGEvent::tap_enable(&port, true);
        PORT.with(|p| *p.borrow_mut() = Some(port.clone()));
        self.port = Some(port);
        self.rl_source = src;
        true
    }

    pub fn stop(&mut self) {
        if let (Some(rl), Some(src)) = (CFRunLoop::main(), self.rl_source.take()) {
            rl.remove_source(Some(&src), unsafe { kCFRunLoopCommonModes });
        }
        PORT.with(|p| *p.borrow_mut() = None);
        if let Some(port) = self.port.take() {
            port.invalidate();
        }
    }

    /// Start the tap if it is not running, re-create it if its port died, re-enable it if
    /// macOS disabled it without telling us. Returns true if it had to repair something.
    pub fn keep_alive(&mut self) -> bool {
        let (dead, disabled) = match &self.port {
            None => (true, false),
            Some(p) => (!p.is_valid(), !CGEvent::tap_is_enabled(p)),
        };
        if dead {
            self.stop();
            self.start();
        } else if disabled {
            if let Some(p) = &self.port {
                CGEvent::tap_enable(p, true);
            }
        }
        dead || disabled
    }

    pub fn clear_sync(&mut self) {
        self.out.clear();
    }

    // ---------- output ----------

    fn post(&self, ev: &CGEvent) {
        unsafe { CGEvent::tap_post_event(self.proxy, Some(ev)) };
    }

    /// Cmd + `key` posted outside the tap callback (copy / paste for "convert selection").
    /// Our tap skips it: it comes from our event source.
    pub fn post_command_key(&self, key: u16) {
        for down in [true, false] {
            if let Some(ev) = CGEvent::new_keyboard_event(self.source.as_deref(), key, down) {
                CGEvent::set_flags(Some(&ev), CGEventFlags::MaskCommand);
                CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&ev));
            }
        }
    }

    fn post_key(&self, key: u16, flags: Option<u64>) {
        for down in [true, false] {
            if let Some(ev) = CGEvent::new_keyboard_event(self.source.as_deref(), key, down) {
                if let Some(f) = flags {
                    CGEvent::set_flags(Some(&ev), CGEventFlags(f));
                }
                self.post(&ev);
            }
        }
    }

    fn post_units(&self, units: &[u16]) {
        for down in [true, false] {
            if let Some(ev) = CGEvent::new_keyboard_event(self.source.as_deref(), 0, down) {
                unsafe { CGEvent::keyboard_set_unicode_string(Some(&ev), units.len() as _, units.as_ptr()) };
                self.post(&ev);
            }
        }
    }

    fn run(&self, steps: &[Step]) {
        for s in steps {
            match s {
                Step::Delete => self.post_key(KEY_DELETE, None),
                Step::SelectLeft => self.post_key(KEY_LEFT, Some(CGEventFlags::MaskShift.0)),
                Step::Text(u) => self.post_units(u),
                // with the modifiers the user holds: Shift+Enter must stay Shift+Enter
                Step::Key => self.post_key(self.keycode, Some(self.flag | CGEventFlags::MaskNonCoalesced.0)),
            }
        }
    }
}

/// Character produced by the event (layout aware), if any.
fn event_char(ev: &CGEvent) -> Option<char> {
    let mut len: std::ffi::c_ulong = 0;
    let mut buf = [0u16; 4];
    unsafe { CGEvent::keyboard_get_unicode_string(Some(ev), buf.len() as _, &mut len as *mut _ as *mut _, buf.as_mut_ptr()) };
    char::decode_utf16(buf[..(len as usize).min(4)].iter().copied()).next()?.ok()
}

/// Key code of a character on the US layout (for non-US layouts, "layout compat" option).
fn keycode_for_char(c: char) -> Option<u16> {
    char_to_key(c.to_ascii_lowercase() as u32).map(|k| (k & CHAR_MASK) as u16)
}

fn other_control_key(f: u64) -> bool {
    [
        CGEventFlags::MaskCommand,
        CGEventFlags::MaskControl,
        CGEventFlags::MaskAlternate,
        CGEventFlags::MaskSecondaryFn,
        CGEventFlags::MaskNumericPad,
        CGEventFlags::MaskHelp,
    ]
    .iter()
    .any(|&m| has(f, m))
}

fn check_hotkey(h: i64, last_flag: u64, keycode: u16, check_keycode: bool) -> bool {
    if (h & !hotkey::BEEP) == super::EMPTY_HOTKEY {
        return false;
    }
    let pairs = [
        (hotkey::CONTROL, CGEventFlags::MaskControl),
        (hotkey::OPTION, CGEventFlags::MaskAlternate),
        (hotkey::COMMAND, CGEventFlags::MaskCommand),
        (hotkey::SHIFT, CGEventFlags::MaskShift),
    ];
    if pairs.iter().any(|&(bit, f)| (h & bit != 0) != has(last_flag, f)) {
        return false;
    }
    !check_keycode || hotkey::key(h) == keycode
}

enum Outcome {
    Pass,
    Swallow,
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    ty: CGEventType,
    event: NonNull<CGEvent>,
    _info: *mut c_void,
) -> *mut CGEvent {
    if ty == CGEventType::TapDisabledByTimeout || ty == CGEventType::TapDisabledByUserInput {
        // Re-enable even when the app state is busy, or the tap would stay off for good.
        PORT.with(|p| {
            if let Some(port) = &*p.borrow() {
                CGEvent::tap_enable(port, true);
            }
        });
        // keys typed while the tap was off never reached the engine
        with_app(|a| a.forget_screen());
        return event.as_ptr();
    }
    let ev = unsafe { event.as_ref() };
    let started = std::time::Instant::now();
    if ty == CGEventType::KeyDown {
        LAST_KEY.set(Some(started));
    }
    let outcome = with_app(|app| handle(app, _proxy, ty, ev)).unwrap_or(Outcome::Pass);
    if ty == CGEventType::KeyDown {
        crate::stats::record_key(started.elapsed());
    }
    match outcome {
        Outcome::Pass => event.as_ptr(),
        Outcome::Swallow => std::ptr::null_mut(),
    }
}

fn handle(app: &mut App, proxy: CGEventTapProxy, ty: CGEventType, ev: &CGEvent) -> Outcome {
    // don't handle our own events
    if CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceStateID) == app.tap.source_id {
        return Outcome::Pass;
    }
    let is_mouse = [
        CGEventType::LeftMouseDown,
        CGEventType::RightMouseDown,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDragged,
    ]
    .contains(&ty);
    if is_mouse {
        // the caret may be anywhere now: in every mode (also English with macros), and
        // without a restore of the word being typed
        app.forget_screen();
        return Outcome::Pass;
    }
    let t = &mut app.tap;
    t.flag = CGEvent::flags(Some(ev)).0;
    t.keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u16;
    let ch = if ty == CGEventType::KeyDown { event_char(ev) } else { None };
    if ty == CGEventType::KeyDown && app.opt.perform_layout_compat {
        if let Some(k) = ch.and_then(keycode_for_char) {
            t.keycode = k;
        }
    }
    // Cmd + Space may be opening Spotlight, Esc / Return closing it
    if ty == CGEventType::KeyDown && (has(t.flag, CGEventFlags::MaskCommand) || matches!(t.keycode, KEY_ESC | KEY_RETURN)) {
        sys::forget_spotlight();
    }

    // hot keys: switch language, quick convert
    let (switch_key, convert_key) = (app.opt.switch_key, app.opt.convert_hotkey);
    if ty == CGEventType::KeyDown {
        let kc = t.keycode;
        if hotkey::key(switch_key) != kc && hotkey::key(convert_key) != kc {
            t.last_flag = 0;
        } else {
            if hotkey::key(switch_key) == kc && check_hotkey(switch_key, t.last_flag, kc, true) {
                t.last_flag = 0;
                switch_language(app);
                return Outcome::Swallow;
            }
            if hotkey::key(convert_key) == kc && check_hotkey(convert_key, t.last_flag, kc, true) {
                t.last_flag = 0;
                ui::defer(ui::convert_selection);
                return Outcome::Swallow;
            }
        }
    } else if ty == CGEventType::FlagsChanged {
        if t.last_flag == 0 || t.last_flag < t.flag {
            t.last_flag = t.flag;
        } else if t.last_flag > t.flag {
            let has_key = |h: i64| hotkey::key(h) != hotkey::NO_KEY as u16;
            if check_hotkey(switch_key, t.last_flag, t.keycode, has_key(switch_key)) {
                t.last_flag = 0;
                switch_language(app);
                return Outcome::Swallow;
            }
            if check_hotkey(convert_key, t.last_flag, t.keycode, has_key(convert_key)) {
                t.last_flag = 0;
                ui::defer(ui::convert_selection);
                return Outcome::Swallow;
            }
            app.tap.last_flag = 0;
        }
    }

    // statistics: counts only (a character never leaves this function)
    if ty == CGEventType::KeyDown && !other_control_key(app.tap.flag) {
        if let Some(c) = ch {
            crate::stats::record_char(c);
        }
    }
    // excluded app (Terminal, IDE...): stay out of the way, but hot keys above still work
    if app.front_excluded() || ty != CGEventType::KeyDown {
        return Outcome::Pass;
    }
    app.tap.proxy = proxy;
    let flag = app.tap.flag;
    let kc = app.tap.keycode;
    let shift = has(flag, CGEventFlags::MaskShift);
    // Case of the character the OS will actually type, not the modifier flags.
    let caps = match ch {
        Some(c) if c.is_alphabetic() => c.is_uppercase(),
        Some(c) => char_to_key(c as u32).map_or(shift, |k| k & CAPS_MASK != 0),
        None => shift,
    };
    let caps_status = if !caps { 0 } else if shift { 1 } else { 2 };
    // the typed key, as the OS would have typed it, when the engine gives it back
    let typed = if key_to_char(kc as u32) == 0 {
        Typed::Control
    } else {
        match ch {
            Some(c) if !c.is_control() && (c as u32) <= 0xFFFF => Typed::Char(c as u16),
            _ => Typed::Char(key_to_char(kc as u32 | if caps { CAPS_MASK } else { 0 })),
        }
    };

    if app.engine.s.language == 0 {
        if app.engine.s.use_macro && app.engine.s.use_macro_in_english {
            app.engine.english_mode(KeyState::KeyDown, kc, caps, other_control_key(flag));
            if app.engine.hook.code == Code::ReplaceMacro {
                output(app, Typed::Printable);
                return Outcome::Swallow;
            }
        }
        return Outcome::Pass;
    }

    if app.opt.other_language && !app.english_input_source {
        return Outcome::Pass;
    }

    // Esc right after a word was changed: give the typed keys back instead of the Esc
    let undone = kc == KEY_ESC && app.opt.esc_undo && !other_control_key(flag) && app.engine.undo_word();
    if !undone {
        app.engine.handle_event(KeyEvent::Keyboard, KeyState::KeyDown, kc, caps_status, other_control_key(flag));
    }
    match app.engine.hook.code {
        Code::DoNothing => {
            output(app, typed);
            Outcome::Pass
        }
        // the macro's trigger key (space, punctuation) is sent again as a key
        Code::ReplaceMacro => {
            output(app, Typed::Printable);
            Outcome::Swallow
        }
        _ => {
            output(app, typed);
            Outcome::Swallow
        }
    }
}

/// Send what the engine's last result needs on screen.
fn output(app: &mut App, typed: Typed) {
    let code = app.engine.hook.code;
    if code == Code::ReplaceMacro {
        if let Some(name) = app.engine.last_macro.take() {
            crate::stats::record_macro(&name);
        }
    }
    let front = app.front_app.as_str();
    let mut ctx = Ctx {
        grapheme_delete: UNICODE_COMPOUND_APPS.iter().any(|p| front.starts_with(p)),
        step_by_step: app.opt.send_key_step_by_step,
        empty_char: if NICE_SPACE_APPS.contains(&front) { 0x200C } else { 0x202F },
        ..Ctx::default()
    };
    // Spotlight is checked only when something is rewritten (the check is not free)
    if code != Code::DoNothing {
        let spotlight = front == SPOTLIGHT || sys::is_spotlight_visible();
        ctx.select_instead_of_delete = spotlight;
        if app.opt.fix_recommend_browser && !spotlight {
            ctx.browser_fix = if app.opt.fix_chromium_browser && CHROMIUM_APPS.contains(&front) && code != Code::ReplaceMacro {
                BrowserFix::SelectLeft
            } else {
                BrowserFix::EmptyChar
            };
        }
    }
    let steps = app.tap.out.plan(&mut app.engine, &ctx, typed);
    app.tap.run(&steps);
}

fn switch_language(app: &mut App) {
    let lang = if app.engine.s.language == 0 { 1 } else { 0 };
    if app.opt.switch_key & hotkey::BEEP != 0 {
        sys::beep();
    }
    app.set_language(lang);
    if app.opt.mode_hud {
        ui::defer(ui::show_mode_hud);
    }
}
