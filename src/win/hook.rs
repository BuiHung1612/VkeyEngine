//! Low level keyboard / mouse hooks and synthetic key output. Port of OpenKey.cpp (Win32).
//!
//! Fixes vs the C++ version:
//! - hooks live on a dedicated thread, and are re-installed when Windows silently drops them
//!   (it does so when a hook answers too slowly, e.g. while the machine is swapping); before,
//!   Vietnamese stayed "on" but every key went through unchanged.
//! - the engine works with macOS key codes; virtual keys are translated here. The C++ table
//!   had ` \ ' swapped (KEY_BACKQUOTE = VK_OEM_5, KEY_BACK_SLASH = VK_OEM_7, KEY_QUOTE = VK_OEM_3).
//! - output is sent as Unicode key events in one batch instead of through the clipboard +
//!   Shift+Insert (which overwrote whatever the user had copied).
//! - Caps Lock only changes letters (Caps Lock + "1" no longer restores to "!").

use super::{hotkey, ui, with_app, App};
use crate::engine::{encode_output, is_double_code, Code, KeyEvent, KeyState, MAX_BUFF};
use crate::tables::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// dwExtraInfo of the events we send, so the hook can skip them
const OUR_EVENT: usize = 0x564B_4559; // "VKEY"
const IMC_GETOPENSTATUS: usize = 5;
/// thread message: re-install the hooks
pub const WM_REHOOK: u32 = WM_APP + 1;

const CHROMIUM_APPS: &[&str] = &["chrome.exe", "brave.exe", "msedge.exe"];
/// unknown virtual keys are passed to the engine above the macOS key code range
const VK_BASE: u16 = 0x200;

// modifier bits of `Hook::flag`
const SHIFT: u32 = 0x01;
const CONTROL: u32 = 0x02;
const ALT: u32 = 0x04;
const CAPITAL: u32 = 0x08;
const WIN: u32 = 0x20;

#[derive(Default)]
pub struct Hook {
    /// UTF-16 length of each character of the current word (double-code tables only)
    pub sync_key: Vec<u8>,
    flag: u32,
    last_flag: u32,
    has_just_used_hotkey: bool,
    /// virtual key of the last non-modifier key
    vk: u16,
}

static KEYBOARD: AtomicIsize = AtomicIsize::new(0);
static MOUSE: AtomicIsize = AtomicIsize::new(0);
static THREAD: AtomicU32 = AtomicU32::new(0);
/// GetTickCount() of the last key seen by the keyboard hook, and by raw input
static LAST_HOOK_KEY: AtomicU32 = AtomicU32::new(0);
static LAST_RAW_KEY: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// Start the hook thread.
pub fn start() {
    std::thread::spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        install();
        let foreground: HWINEVENTHOOK = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_REHOOK {
                uninstall();
                install();
                with_app(|a| {
                    a.engine.start_new_session();
                    a.hook.sync_key.clear();
                });
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWinEvent(foreground);
        uninstall();
    });
}

unsafe fn install() {
    let module = GetModuleHandleW(std::ptr::null());
    KEYBOARD.store(SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0) as isize, Ordering::SeqCst);
    MOUSE.store(SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0) as isize, Ordering::SeqCst);
    LAST_HOOK_KEY.store(GetTickCount(), Ordering::SeqCst);
}

unsafe fn uninstall() {
    for h in [&KEYBOARD, &MOUSE] {
        let p = h.swap(0, Ordering::SeqCst);
        if p != 0 {
            UnhookWindowsHookEx(p as HHOOK);
        }
    }
}

/// A key event arrived through raw input (UI thread, WM_INPUT).
pub fn raw_key_seen() {
    LAST_RAW_KEY.store(unsafe { GetTickCount() }, Ordering::SeqCst);
}

/// Called periodically by the UI thread: if a key was pressed well after the last key our
/// keyboard hook saw, Windows has removed the hook, so ask the hook thread to install it
/// again. Raw input is used as the reference because it does not depend on the hooks (the
/// mouse hook may survive alone, so "any input seen" would hide a dead keyboard hook).
pub fn check_alive() {
    let (raw, hooked) = (LAST_RAW_KEY.load(Ordering::SeqCst), LAST_HOOK_KEY.load(Ordering::SeqCst));
    if raw.wrapping_sub(hooked) as i32 > 1000 {
        LAST_HOOK_KEY.store(raw, Ordering::SeqCst);
        unsafe { PostThreadMessageW(THREAD.load(Ordering::SeqCst), WM_REHOOK, 0, 0) };
    }
}

/// Run `f` with the app state, unless we are already inside the hook (re-entrant call).
fn guarded<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    if IN_HOOK.with(|h| h.replace(true)) {
        return None;
    }
    let r = with_app(f);
    IN_HOOK.with(|h| h.set(false));
    r
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let next = || CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
    if code < 0 {
        return next();
    }
    LAST_HOOK_KEY.store(GetTickCount(), Ordering::SeqCst);
    let k = &*(lparam as *const KBDLLHOOKSTRUCT);
    if k.dwExtraInfo == OUR_EVENT {
        return next();
    }
    // The IME of another language (Japanese, Chinese...) is open. With a timeout: a hung
    // app must not stall the hook (Windows would remove it).
    let ime = ImmGetDefaultIMEWnd(GetForegroundWindow());
    let mut open = 0usize;
    if !ime.is_null()
        && SendMessageTimeoutW(ime, WM_IME_CONTROL, IMC_GETOPENSTATUS, 0, SMTO_ABORTIFHUNG, 50, &mut open) != 0
        && open != 0
    {
        return next();
    }
    let msg = wparam as u32;
    if guarded(|a| handle_key(a, msg, k.vkCode as u16)).unwrap_or(false) {
        1
    } else {
        next()
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let click = matches!(
            wparam as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
        );
        if click {
            guarded(|a| {
                a.engine.handle_event(KeyEvent::Mouse, KeyState::MouseDown, 0, 0, false);
                a.hook.sync_key.clear();
            });
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let Some(exe) = foreground_exe() else { return };
    if exe.eq_ignore_ascii_case("explorer.exe") {
        return;
    }
    guarded(|a| a.on_active_app_changed(exe));
}

/// Executable name of the foreground window's process.
fn foreground_exe() -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        ok.then(|| path.rsplit('\\').next().unwrap_or_default().to_string())
    }
}

/// Virtual key -> engine (macOS) key code.
fn engine_key(vk: u16) -> u16 {
    let ch = match vk {
        0x41..=0x5A => (vk as u8 + 32) as char, // A-Z
        0x30..=0x39 => vk as u8 as char,        // 0-9
        VK_SPACE => ' ',
        VK_OEM_1 => ';',
        VK_OEM_PLUS => '=',
        VK_OEM_COMMA => ',',
        VK_OEM_MINUS => '-',
        VK_OEM_PERIOD => '.',
        VK_OEM_2 => '/',
        VK_OEM_3 => '`',
        VK_OEM_4 => '[',
        VK_OEM_5 => '\\',
        VK_OEM_6 => ']',
        VK_OEM_7 => '\'',
        VK_BACK => return KEY_DELETE,
        VK_TAB => return KEY_TAB,
        VK_RETURN => return KEY_RETURN,
        VK_ESCAPE => return KEY_ESC,
        VK_LEFT => return KEY_LEFT,
        VK_RIGHT => return KEY_RIGHT,
        VK_UP => return KEY_UP,
        VK_DOWN => return KEY_DOWN,
        _ => return VK_BASE + vk,
    };
    char_to_key(ch as u32).map_or(VK_BASE + vk, |k| (k & CHAR_MASK) as u16)
}

fn modifier(vk: u16) -> Option<u32> {
    match vk {
        VK_LSHIFT | VK_RSHIFT | VK_SHIFT => Some(SHIFT),
        VK_LCONTROL | VK_RCONTROL | VK_CONTROL => Some(CONTROL),
        VK_LMENU | VK_RMENU | VK_MENU => Some(ALT),
        VK_LWIN | VK_RWIN => Some(WIN),
        VK_NUMLOCK | VK_SCROLL => Some(0),
        _ => None,
    }
}

fn check_hotkey(h: i64, last_flag: u32, vk: u16, check_key: bool) -> bool {
    if (h & !hotkey::BEEP) == super::EMPTY_HOTKEY {
        return false;
    }
    let pairs = [(hotkey::CONTROL, CONTROL), (hotkey::ALT, ALT), (hotkey::WIN, WIN), (hotkey::SHIFT, SHIFT)];
    if pairs.iter().any(|&(bit, f)| (h & bit != 0) != (last_flag & f != 0)) {
        return false;
    }
    !check_key || hotkey::key(h) == vk
}

/// We swallowed the key of an Alt / Win hot key, so the app would only see the modifier
/// pressed and released: Alt alone opens the menu bar, Win alone the Start menu. An
/// unassigned key in between prevents that (the same trick AutoHotkey uses).
fn mask_menu_key(flag: u32) {
    if flag & (ALT | WIN) != 0 {
        send_vk(0xE8, 0);
    }
}

fn switch_language(app: &mut App) {
    if app.opt.switch_key & hotkey::BEEP != 0 {
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(MB_OK) };
    }
    let lang = 1 - app.engine.s.language.min(1);
    app.set_language(lang);
}

/// Returns true to swallow the key.
fn handle_key(app: &mut App, msg: u32, vk: u16) -> bool {
    let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let h = &mut app.hook;
    // Caps Lock can change without us seeing the key (e.g. pressed in an elevated app)
    if unsafe { GetKeyState(VK_CAPITAL as i32) } & 1 != 0 {
        h.flag |= CAPITAL;
    } else {
        h.flag &= !CAPITAL;
    }
    let modifier = modifier(vk);
    match modifier {
        Some(m) if down => h.flag |= m,
        Some(m) if up => h.flag &= !m,
        None if down => h.vk = vk,
        _ => {}
    }

    // hot keys: switch language, quick convert
    let (switch_key, convert_key) = (app.opt.switch_key, app.opt.convert_hotkey);
    let has_key = |k: i64| hotkey::key(k) != hotkey::NO_KEY as u16;
    if down && modifier.is_none() {
        let h = &mut app.hook;
        if hotkey::key(switch_key) != vk && hotkey::key(convert_key) != vk {
            h.last_flag = 0;
        } else {
            if check_hotkey(switch_key, h.last_flag, vk, true) && hotkey::key(switch_key) == vk {
                h.has_just_used_hotkey = true;
                h.vk = 0;
                mask_menu_key(h.last_flag);
                switch_language(app);
                return true;
            }
            if check_hotkey(convert_key, h.last_flag, vk, true) && hotkey::key(convert_key) == vk {
                h.has_just_used_hotkey = true;
                h.vk = 0;
                mask_menu_key(h.last_flag);
                ui::post_quick_convert();
                return true;
            }
        }
        h.has_just_used_hotkey = h.last_flag != 0;
    } else if modifier.is_some() {
        let h = &mut app.hook;
        if h.last_flag == 0 || h.last_flag < h.flag {
            h.last_flag = h.flag;
        } else if h.last_flag > h.flag {
            // modifiers released: modifier-only hot keys
            let (last, vk) = (h.last_flag, h.vk);
            if check_hotkey(switch_key, last, vk, has_key(switch_key)) {
                app.hook.has_just_used_hotkey = true;
                switch_language(app);
            }
            if check_hotkey(convert_key, last, vk, has_key(convert_key)) {
                app.hook.has_just_used_hotkey = true;
                ui::post_quick_convert();
            }
            let just_used = app.hook.has_just_used_hotkey;
            if app.opt.temp_off_spelling && !just_used && last & CONTROL != 0 {
                app.engine.temp_off_spell_checking();
            }
            if app.opt.temp_off_app && !just_used && last & ALT != 0 {
                app.engine.temp_off_engine(true);
            }
            let h = &mut app.hook;
            h.last_flag = h.flag;
            h.has_just_used_hotkey = false;
        }
        app.hook.vk = 0;
        return false;
    }

    let flag = app.hook.flag;
    let ek = engine_key(vk);
    let shift = flag & SHIFT != 0;
    let letter = (key_to_char(ek as u32) as u8).is_ascii_lowercase();
    let caps_lock = letter && flag & CAPITAL != 0;
    let caps = shift != caps_lock;
    let caps_status = if shift && caps_lock { 0 } else if shift { 1 } else if caps_lock { 2 } else { 0 };
    let other_control_key = flag & (CONTROL | ALT) != 0;

    if app.engine.s.language == 0 {
        if app.engine.s.use_macro && app.engine.s.use_macro_in_english && down {
            app.engine.english_mode(KeyState::KeyDown, ek, caps, other_control_key);
            if app.engine.hook.code == Code::ReplaceMacro {
                handle_macro(app, ek, shift);
                return true;
            }
        }
        return false;
    }
    if !down {
        return false;
    }

    app.engine.handle_event(KeyEvent::Keyboard, KeyState::KeyDown, ek, caps_status, other_control_key);
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    match app.engine.hook.code {
        Code::DoNothing => {
            if double {
                let h = &mut app.hook;
                match app.engine.hook.ext_code {
                    1 => h.sync_key.clear(),
                    2 => {
                        if let Some(last) = h.sync_key.pop() {
                            if last > 1 {
                                send_vk(VK_BACK, 0);
                            }
                        }
                    }
                    3 => h.sync_key.push(1),
                    _ => {}
                }
            }
            false
        }
        Code::WillProcess | Code::Restore | Code::RestoreAndStartNewSession => {
            let mut bs = app.engine.hook.backspace_count;
            if app.opt.fix_recommend_browser && app.engine.hook.ext_code != 4 {
                let chromium = CHROMIUM_APPS.iter().any(|c| c.eq_ignore_ascii_case(&app.front_app));
                if app.opt.fix_chromium_browser && chromium {
                    if bs > 0 {
                        shift_left(app, double);
                        if bs == 1 {
                            bs = 0;
                        }
                    }
                } else {
                    empty_char(app, double);
                    bs += 1;
                }
            }
            if bs > 0 && bs < MAX_BUFF as i32 {
                for _ in 0..bs {
                    backspace(app, double);
                }
            }
            let typed = key_to_char(ek as u32 | if caps { CAPS_MASK } else { 0 });
            send_new_chars(app, (typed != 0).then_some(typed), vk);
            true
        }
        Code::ReplaceMacro => {
            handle_macro(app, ek, shift);
            true
        }
        Code::BreakWord => false,
    }
}

// ---------------------------------------------------------------- output

fn key_input(vk: u16, unicode: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: unicode, dwFlags: flags, time: 0, dwExtraInfo: OUR_EVENT },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    }
}

fn send_vk(vk: u16, flags: KEYBD_EVENT_FLAGS) {
    send(&[key_input(vk, 0, flags), key_input(vk, 0, flags | KEYEVENTF_KEYUP)]);
}

/// Unicode key events (down + up) for each UTF-16 unit.
fn units_input(units: &[u16], out: &mut Vec<INPUT>) {
    for &u in units {
        out.push(key_input(0, u, KEYEVENTF_UNICODE));
        out.push(key_input(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
}

fn send_chars(app: &App, chars: &[Vec<u16>]) {
    let mut inputs = Vec::new();
    for c in chars {
        units_input(c, &mut inputs);
        if app.opt.send_key_step_by_step {
            send(&inputs);
            inputs.clear();
        }
    }
    send(&inputs);
}

fn backspace(app: &mut App, double: bool) {
    send_vk(VK_BACK, 0);
    if double {
        if app.hook.sync_key.last().is_some_and(|&l| l > 1) {
            send_vk(VK_BACK, 0);
        }
        app.hook.sync_key.pop();
    }
}

/// Select the previous character (Chromium address bar autocomplete fix).
fn shift_left(app: &mut App, double: bool) {
    let times = if double && app.hook.sync_key.last().is_some_and(|&l| l > 1) { 2 } else { 1 };
    if double {
        app.hook.sync_key.pop();
    }
    let mut inputs = vec![key_input(VK_LSHIFT, 0, 0)];
    for _ in 0..times {
        inputs.push(key_input(VK_LEFT, 0, KEYEVENTF_EXTENDEDKEY));
        inputs.push(key_input(VK_LEFT, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP));
    }
    inputs.push(key_input(VK_LSHIFT, 0, KEYEVENTF_KEYUP));
    send(&inputs);
}

/// An invisible character that closes the browser's autocomplete suggestion.
fn empty_char(app: &mut App, double: bool) {
    if double {
        app.hook.sync_key.push(1);
    }
    let mut inputs = Vec::new();
    units_input(&[0x202F], &mut inputs);
    send(&inputs);
}

/// Send the engine's new characters (and the typed key when restoring).
fn send_new_chars(app: &mut App, typed: Option<u16>, vk: u16) {
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    let h = &app.engine.hook;
    let code = h.code;
    let mut chars: Vec<Vec<u16>> = Vec::new();
    let mut lens: Vec<u8> = Vec::new();
    for i in (0..h.new_char_count.clamp(0, MAX_BUFF as i32) as usize).rev() {
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
    if double {
        app.hook.sync_key.extend_from_slice(&lens);
    }
    send_chars(app, &chars);
    // invalid word ended with a control key (tab, arrows...): send that key too
    if send_control_key {
        send_vk(vk, 0);
    }
}

fn handle_macro(app: &mut App, ek: u16, shift: bool) {
    let table = app.engine.s.code_table;
    let double = is_double_code(table);
    let mut bs = app.engine.hook.backspace_count;
    if app.opt.fix_recommend_browser {
        empty_char(app, double);
        bs += 1;
    }
    for _ in 0..bs.max(0) {
        backspace(app, double);
    }
    let mut chars: Vec<Vec<u16>> = Vec::new();
    for &c in &app.engine.hook.macro_data {
        let mut u = Vec::new();
        let l = encode_output(c, table, &mut u);
        if double {
            app.hook.sync_key.push(l);
        }
        chars.push(u);
    }
    // re-send the key that triggered the macro (space, punctuation)
    match key_to_char(ek as u32 | if shift { CAPS_MASK } else { 0 }) {
        0 => {
            send_chars(app, &chars);
            send_vk(app.hook.vk, 0);
        }
        c => {
            chars.push(vec![c]);
            send_chars(app, &chars);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_keys_map_to_engine_keys() {
        assert_eq!(engine_key(0x41), KEY_A);
        assert_eq!(engine_key(0x5A), KEY_Z);
        assert_eq!(engine_key(0x31), KEY_1);
        // the C++ table had these three swapped
        assert_eq!(engine_key(VK_OEM_3), KEY_BACKQUOTE);
        assert_eq!(engine_key(VK_OEM_5), KEY_BACK_SLASH);
        assert_eq!(engine_key(VK_OEM_7), KEY_QUOTE);
        assert_eq!(engine_key(VK_BACK), KEY_DELETE);
        assert_eq!(engine_key(VK_F5), VK_BASE + VK_F5);
    }
}
