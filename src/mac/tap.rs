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
use crate::engine::{encode_output, is_double_code, Code, KeyEvent, KeyState};
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
/// Some apps truncate long unicode strings in one event.
const MAX_UNITS_PER_EVENT: usize = 16;

thread_local! {
    /// The running tap, reachable without borrowing the app state (see `callback`).
    static PORT: std::cell::RefCell<Option<CFRetained<CFMachPort>>> = const { std::cell::RefCell::new(None) };
}

pub struct Tap {
    port: Option<CFRetained<CFMachPort>>,
    rl_source: Option<CFRetained<CFRunLoopSource>>,
    source: Option<CFRetained<CGEventSource>>,
    source_id: i64,
    proxy: CGEventTapProxy,
    /// UTF-16 length of each character of the current word (double-code tables only)
    sync_key: Vec<u8>,
    last_flag: u64,
    has_just_used_hotkey: bool,
    keycode: u16,
    flag: u64,
}

fn has(flags: u64, f: CGEventFlags) -> bool {
    flags & f.0 != 0
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
            sync_key: Vec::new(),
            last_flag: 0,
            has_just_used_hotkey: false,
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

    pub fn clear_sync(&mut self) {
        self.sync_key.clear();
    }

    // ---------- output ----------

    fn post(&self, ev: &CGEvent) {
        unsafe { CGEvent::tap_post_event(self.proxy, Some(ev)) };
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

    /// Post a string, split in chunks without breaking a character.
    fn post_string(&self, chars: &[Vec<u16>]) {
        let mut buf: Vec<u16> = Vec::new();
        for c in chars {
            if buf.len() + c.len() > MAX_UNITS_PER_EVENT {
                self.post_units(&buf);
                buf.clear();
            }
            buf.extend_from_slice(c);
        }
        if !buf.is_empty() {
            self.post_units(&buf);
        }
    }

    fn uses_compound_workaround(app: &App) -> bool {
        UNICODE_COMPOUND_APPS.iter().any(|p| app.front_app.starts_with(p))
    }

    fn backspace(&mut self, double: bool, compound_app: bool, table: u8) {
        self.post_key(KEY_DELETE, None);
        if double {
            if self.sync_key.last().is_some_and(|&l| l > 1) && !(table == 3 && compound_app) {
                self.post_key(KEY_DELETE, None);
            }
            self.sync_key.pop();
        }
    }

    fn shift_left(&mut self, double: bool, compound_app: bool, table: u8) {
        let flags = Some(CGEventFlags::MaskShift.0);
        self.post_key(KEY_LEFT, flags);
        if double {
            if self.sync_key.last().is_some_and(|&l| l > 1) && !(table == 3 && compound_app) {
                self.post_key(KEY_LEFT, flags);
            }
            self.sync_key.pop();
        }
    }

    fn empty_char(&mut self, double: bool, front_app: &str) {
        if double {
            self.sync_key.push(1);
        }
        let c = if NICE_SPACE_APPS.contains(&front_app) { 0x200C } else { 0x202F };
        self.post_units(&[c]);
    }

    fn spotlight(&mut self) -> bool {
        sys::is_spotlight_visible()
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
        with_app(|a| {
            a.engine.start_new_session();
            a.tap.clear_sync();
        });
        return event.as_ptr();
    }
    let ev = unsafe { event.as_ref() };
    let outcome = with_app(|app| handle(app, _proxy, ty, ev)).unwrap_or(Outcome::Pass);
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
    let t = &mut app.tap;
    t.flag = CGEvent::flags(Some(ev)).0;
    t.keycode = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode) as u16;
    let ch = if ty == CGEventType::KeyDown { event_char(ev) } else { None };
    if ty == CGEventType::KeyDown && app.opt.perform_layout_compat {
        if let Some(k) = ch.and_then(keycode_for_char) {
            t.keycode = k;
        }
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
                t.has_just_used_hotkey = true;
                switch_language(app);
                return Outcome::Swallow;
            }
            if hotkey::key(convert_key) == kc && check_hotkey(convert_key, t.last_flag, kc, true) {
                t.last_flag = 0;
                t.has_just_used_hotkey = true;
                ui::defer(ui::quick_convert);
                return Outcome::Swallow;
            }
        }
        t.has_just_used_hotkey = t.last_flag != 0;
    } else if ty == CGEventType::FlagsChanged {
        if t.last_flag == 0 || t.last_flag < t.flag {
            t.last_flag = t.flag;
        } else if t.last_flag > t.flag {
            let has_key = |h: i64| hotkey::key(h) != hotkey::NO_KEY as u16;
            if check_hotkey(switch_key, t.last_flag, t.keycode, has_key(switch_key)) {
                t.last_flag = 0;
                t.has_just_used_hotkey = true;
                switch_language(app);
                return Outcome::Swallow;
            }
            if check_hotkey(convert_key, t.last_flag, t.keycode, has_key(convert_key)) {
                t.last_flag = 0;
                t.has_just_used_hotkey = true;
                ui::defer(ui::quick_convert);
                return Outcome::Swallow;
            }
            if app.opt.temp_off_spelling && !t.has_just_used_hotkey && has(t.last_flag, CGEventFlags::MaskControl) {
                app.engine.temp_off_spell_checking();
            }
            if app.opt.temp_off_app && !t.has_just_used_hotkey && has(t.last_flag, CGEventFlags::MaskCommand) {
                app.engine.temp_off_engine(true);
            }
            let t = &mut app.tap;
            t.last_flag = 0;
            t.has_just_used_hotkey = false;
        }
    }

    let is_mouse = [
        CGEventType::LeftMouseDown,
        CGEventType::RightMouseDown,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDragged,
    ]
    .contains(&ty);
    if ty != CGEventType::KeyDown && ty != CGEventType::KeyUp && !is_mouse {
        return Outcome::Pass;
    }
    app.tap.proxy = proxy;
    let flag = app.tap.flag;
    let shift = has(flag, CGEventFlags::MaskShift);
    // Case of the character the OS will actually type, not the modifier flags.
    let caps = match ch {
        Some(c) if c.is_alphabetic() => c.is_uppercase(),
        Some(c) => char_to_key(c as u32).map_or(shift, |k| k & CAPS_MASK != 0),
        None => shift,
    };
    let caps_status = if !caps { 0 } else if shift { 1 } else { 2 };

    if app.engine.s.language == 0 {
        if app.engine.s.use_macro && app.engine.s.use_macro_in_english && ty == CGEventType::KeyDown {
            let kc = app.tap.keycode;
            app.engine.english_mode(KeyState::KeyDown, kc, caps, other_control_key(flag));
            if app.engine.hook.code == Code::ReplaceMacro {
                handle_macro(app, caps);
                return Outcome::Swallow;
            }
        }
        return Outcome::Pass;
    }

    if is_mouse {
        app.engine.handle_event(KeyEvent::Mouse, KeyState::MouseDown, 0, 0, false);
        app.tap.clear_sync();
        return Outcome::Pass;
    }

    if app.opt.other_language && !app.english_input_source {
        return Outcome::Pass;
    }

    if ty != CGEventType::KeyDown {
        return Outcome::Pass;
    }

    let kc = app.tap.keycode;
    app.engine.handle_event(KeyEvent::Keyboard, KeyState::KeyDown, kc, caps_status, other_control_key(flag));
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    let compound_app = Tap::uses_compound_workaround(app);

    match app.engine.hook.code {
        Code::DoNothing => {
            if double {
                let t = &mut app.tap;
                match app.engine.hook.ext_code {
                    1 => t.sync_key.clear(),
                    2 => {
                        if let Some(&last) = t.sync_key.last() {
                            if last > 1 && (table == 2 || !compound_app) {
                                t.post_key(KEY_DELETE, None);
                            }
                            t.sync_key.pop();
                        }
                    }
                    3 => t.sync_key.push(1),
                    _ => {}
                }
            }
            Outcome::Pass
        }
        Code::WillProcess | Code::Restore | Code::RestoreAndStartNewSession => {
            let front = app.front_app.clone();
            let mut bs = app.engine.hook.backspace_count;
            if should_use_recommend_workaround(app) && app.engine.hook.ext_code != 4 {
                if app.opt.fix_chromium_browser && CHROMIUM_APPS.contains(&front.as_str()) {
                    if bs > 0 {
                        app.tap.shift_left(double, compound_app, table);
                        if bs == 1 {
                            bs = 0;
                        }
                    }
                } else {
                    app.tap.empty_char(double, &front);
                    bs += 1;
                }
            }
            if bs > 0 && (front == SPOTLIGHT || app.tap.spotlight()) {
                for _ in 0..bs {
                    app.tap.shift_left(double, compound_app, table);
                }
                bs = 0;
            }
            if bs > 0 && bs < crate::engine::MAX_BUFF as i32 {
                for _ in 0..bs {
                    app.tap.backspace(double, compound_app, table);
                }
            }
            // the typed key, as the OS would have typed it (None: control key like tab)
            let typed = (key_to_char(kc as u32) != 0).then(|| match ch {
                Some(c) if !c.is_control() && (c as u32) <= 0xFFFF => c as u16,
                _ => key_to_char(kc as u32 | if caps { CAPS_MASK } else { 0 }),
            });
            send_new_chars(app, typed);
            Outcome::Swallow
        }
        Code::ReplaceMacro => {
            handle_macro(app, caps);
            Outcome::Swallow
        }
        Code::BreakWord => Outcome::Pass,
    }
}

fn should_use_recommend_workaround(app: &mut App) -> bool {
    if !app.opt.fix_recommend_browser || app.front_app == SPOTLIGHT {
        return false;
    }
    !app.tap.spotlight()
}

/// Send the engine's new characters (and the typed key when restoring).
fn send_new_chars(app: &mut App, typed: Option<u16>) {
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    let h = &app.engine.hook;
    let code = h.code;
    let mut chars: Vec<Vec<u16>> = Vec::new();
    let mut lens: Vec<u8> = Vec::new();
    for i in (0..h.new_char_count.clamp(0, crate::engine::MAX_BUFF as i32) as usize).rev() {
        let mut u = Vec::new();
        lens.push(encode_output(h.char_data[i], table, &mut u));
        chars.push(u);
    }
    let restore = matches!(code, Code::Restore | Code::RestoreAndStartNewSession);
    let mut send_control_key = false;
    if restore {
        match typed {
            Some(c) => {
                chars.push(vec![c]);
                lens.push(1);
            }
            None => send_control_key = true,
        }
    }
    if code == Code::RestoreAndStartNewSession {
        app.engine.start_new_session();
    }
    let t = &mut app.tap;
    if double {
        t.sync_key.extend_from_slice(&lens);
    }
    if app.opt.send_key_step_by_step {
        for c in &chars {
            t.post_units(c);
        }
    } else {
        t.post_string(&chars);
    }
    // invalid word ended with a control key (tab, arrows...): send that key too
    if send_control_key {
        t.post_key(t.keycode, Some(CGEventFlags::MaskNonCoalesced.0));
    }
}

fn handle_macro(app: &mut App, shift: bool) {
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    let compound_app = Tap::uses_compound_workaround(app);
    let front = app.front_app.clone();
    let mut bs = app.engine.hook.backspace_count;
    if should_use_recommend_workaround(app) {
        app.tap.empty_char(double, &front);
        bs += 1;
    }
    for _ in 0..bs.max(0) {
        app.tap.backspace(double, compound_app, table);
    }
    let mut chars: Vec<Vec<u16>> = Vec::new();
    for &c in &app.engine.hook.macro_data {
        let mut u = Vec::new();
        let l = encode_output(c, table, &mut u);
        if double {
            app.tap.sync_key.push(l);
        }
        chars.push(u);
    }
    let t = &mut app.tap;
    if app.opt.send_key_step_by_step {
        for c in &chars {
            t.post_units(c);
        }
    } else {
        t.post_string(&chars);
    }
    // re-send the key that triggered the macro (space, punctuation)
    let shift_flag = if shift { CGEventFlags::MaskShift.0 } else { 0 };
    t.post_key(t.keycode, Some(shift_flag | CGEventFlags::MaskNonCoalesced.0));
}

fn switch_language(app: &mut App) {
    let lang = if app.engine.s.language == 0 { 1 } else { 0 };
    if app.opt.switch_key & hotkey::BEEP != 0 {
        sys::beep();
    }
    app.set_language(lang);
}
