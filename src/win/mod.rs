//! Windows front end: tray icon + low level keyboard hook.
//!
//! The hooks run on their own thread so that UI work never delays a keystroke (Windows
//! silently removes a low level hook that answers too slowly). The app state is shared
//! between that thread and the UI thread through `APP`.

pub mod hook;
pub mod reg;
pub mod ui;

use crate::convert::ConvertOptions;
use crate::engine::{Engine, Settings};
use crate::smart_switch::SmartSwitch;
use std::sync::Mutex;

/// Alt + Z (key = virtual key code, character in bits 24-31)
pub const DEFAULT_SWITCH_KEY: i64 = 0x5A00025A;
pub const EMPTY_HOTKEY: i64 = 0xFE0000FE;

/// Same layout as macOS: bits 0-7 virtual key (0xFE: modifiers only), 8 Ctrl, 9 Alt,
/// 10 Win, 11 Shift, 15 beep, 24-31 character shown in the UI.
pub mod hotkey {
    pub const CONTROL: i64 = 0x100;
    pub const ALT: i64 = 0x200;
    pub const WIN: i64 = 0x400;
    pub const SHIFT: i64 = 0x800;
    pub const BEEP: i64 = 0x8000;
    pub const NO_KEY: i64 = 0xFE;

    pub fn key(h: i64) -> u16 {
        (h & 0xFF) as u16
    }

    pub fn describe(h: i64) -> String {
        let mut parts = Vec::new();
        for (bit, s) in [(CONTROL, "Ctrl"), (ALT, "Alt"), (WIN, "Win"), (SHIFT, "Shift")] {
            if h & bit != 0 {
                parts.push(s.to_string());
            }
        }
        if key(h) != NO_KEY as u16 {
            parts.push(key_title(h));
        }
        parts.join(" + ")
    }

    pub fn key_title(h: i64) -> String {
        match key(h) {
            k if k == NO_KEY as u16 => "(không phím)".into(),
            0x20 => "Space".into(),
            _ => (((h >> 24) & 0xFF) as u8 as char).to_uppercase().to_string(),
        }
    }
}

/// Front-end options that are not part of the engine.
#[derive(Default, Clone)]
pub struct Options {
    pub switch_key: i64,
    pub send_key_step_by_step: bool,
    pub use_smart_switch_key: bool,
    pub temp_off_spelling: bool,
    pub remember_code: bool,
    pub temp_off_app: bool,
    pub fix_recommend_browser: bool,
    pub fix_chromium_browser: bool,
    pub convert: ConvertOptions,
    pub convert_alert: bool,
    pub convert_hotkey: i64,
}

pub struct App {
    pub engine: Engine,
    pub opt: Options,
    pub smart: SmartSwitch,
    pub hook: hook::Hook,
    /// executable name of the foreground app, e.g. "chrome.exe"
    pub front_app: String,
}

pub static APP: Mutex<Option<App>> = Mutex::new(None);

/// Run `f` with the app state (shared by the hook and UI threads; keep it short).
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    let mut g = APP.lock().unwrap_or_else(|e| e.into_inner());
    g.as_mut().map(f)
}

pub fn read_settings() -> (Settings, Options) {
    use reg::{flag, int};
    let s = Settings {
        language: int("vLanguage", 1).clamp(0, 1) as u8,
        input_type: int("vInputType", 0).clamp(0, 3) as u8,
        free_mark: false,
        code_table: int("vCodeTable", 0).clamp(0, 4) as u8,
        check_spelling: flag("vCheckSpelling", true),
        modern_orthography: flag("vUseModernOrthography", false),
        quick_telex: flag("vQuickTelex", false),
        restore_if_wrong_spelling: flag("vRestoreIfWrongSpelling", true),
        use_macro: flag("vUseMacro", true),
        use_macro_in_english: flag("vUseMacroInEnglishMode", false),
        auto_caps_macro: flag("vAutoCapsMacro", false),
        upper_case_first_char: flag("vUpperCaseFirstChar", false),
        allow_consonant_zfwj: flag("vAllowConsonantZFWJ", false),
        quick_start_consonant: flag("vQuickStartConsonant", false),
        quick_end_consonant: flag("vQuickEndConsonant", false),
    };
    // OpenKey for Windows wrote the macOS default (key code 6, which no key sends) on a
    // fresh install, so that value means "default" too.
    let switch_key = match int("vSwitchKeyStatus", DEFAULT_SWITCH_KEY) {
        0 | 0x7A000206 => DEFAULT_SWITCH_KEY,
        v => v,
    };
    let convert_hotkey = match int("convertToolHotKey", EMPTY_HOTKEY) {
        0 => EMPTY_HOTKEY,
        v => v,
    };
    let o = Options {
        switch_key,
        send_key_step_by_step: flag("vSendKeyStepByStep", true),
        use_smart_switch_key: flag("vUseSmartSwitchKey", true),
        temp_off_spelling: flag("vTempOffSpelling", false),
        remember_code: flag("vRememberCode", true),
        temp_off_app: flag("vTempOffOpenKey", false),
        fix_recommend_browser: flag("vFixRecommendBrowser", true),
        fix_chromium_browser: flag("vFixChromiumBrowser", false),
        convert: ConvertOptions {
            to_all_caps: flag("convertToolToAllCaps", false),
            to_all_non_caps: flag("convertToolToAllNonCaps", false),
            caps_first_letter: flag("convertToolToCapsFirstLetter", false),
            caps_each_word: flag("convertToolToCapsEachWord", false),
            remove_mark: flag("convertToolRemoveMark", false),
            from_code: int("convertToolFromCode", 0).clamp(0, 4) as u8,
            to_code: int("convertToolToCode", 0).clamp(0, 4) as u8,
        },
        // the value name is historical: it stores "alert when completed"
        convert_alert: flag("convertToolDontAlertWhenCompleted", false),
        convert_hotkey,
    };
    (s, o)
}

impl App {
    fn new() -> App {
        let (s, opt) = read_settings();
        let mut engine = Engine::new(s);
        let table = engine.s.code_table;
        engine.macros.load_binary(&reg::data("macroData"), table);
        let mut smart = SmartSwitch::default();
        smart.load_binary(&reg::data("smartSwitchKey"));
        App { engine, opt, smart, hook: hook::Hook::default(), front_app: String::new() }
    }

    /// Re-read settings after the UI changed them.
    pub fn reload_settings(&mut self) {
        let (s, opt) = read_settings();
        let old = self.engine.s.clone();
        if s.check_spelling != old.check_spelling {
            self.engine.set_check_spelling(s.check_spelling);
        }
        self.engine.s = s;
        if self.engine.s.code_table != old.code_table {
            self.on_code_table_changed();
        }
        if self.engine.s.language != old.language {
            self.on_language_changed();
        }
        self.opt = opt;
    }

    pub fn save_macros(&self) {
        reg::set_data("macroData", &self.engine.macros.save_binary());
    }

    fn smart_value(&self) -> i8 {
        (self.engine.s.language | (self.engine.s.code_table << 1)) as i8
    }

    fn save_smart(&mut self) {
        if self.front_app.is_empty() {
            return; // no app seen yet
        }
        let v = self.smart_value();
        self.smart.set(&self.front_app, v);
        reg::set_data("smartSwitchKey", &self.smart.save_binary());
    }

    fn on_code_table_changed(&mut self) {
        let t = self.engine.s.code_table;
        self.engine.set_code_table(t);
        self.hook.sync_key.clear();
        if self.opt.remember_code {
            self.save_smart();
        }
    }

    fn on_language_changed(&mut self) {
        self.engine.start_new_session();
        if self.opt.use_smart_switch_key {
            self.save_smart();
        }
    }

    /// Safe to call from the hook thread: the UI is refreshed through a posted message.
    pub fn set_language(&mut self, lang: u8) {
        reg::set_int("vLanguage", lang as i64);
        self.engine.s.language = lang;
        self.on_language_changed();
        ui::post_refresh();
    }

    pub fn set_code_table(&mut self, t: u8) {
        reg::set_int("vCodeTable", t as i64);
        self.engine.s.code_table = t;
        self.on_code_table_changed();
        ui::post_refresh();
    }

    /// Smart switch: restore the language / code table remembered for the new foreground app.
    pub fn on_active_app_changed(&mut self, exe: String) {
        self.front_app = exe;
        self.engine.start_new_session();
        self.engine.temp_off_engine(false);
        self.hook.sync_key.clear();
        if !self.opt.use_smart_switch_key && !self.opt.remember_code {
            return;
        }
        let current = self.smart_value();
        match self.smart.get(&self.front_app, current) {
            None => reg::set_data("smartSwitchKey", &self.smart.save_binary()),
            Some(v) => {
                let lang = (v & 1) as u8;
                if self.opt.use_smart_switch_key && lang != self.engine.s.language {
                    reg::set_int("vLanguage", lang as i64);
                    self.engine.s.language = lang;
                }
                let table = ((v >> 1) as u8).min(4);
                if self.opt.remember_code && table != self.engine.s.code_table {
                    reg::set_int("vCodeTable", table as i64);
                    self.engine.s.code_table = table;
                    self.engine.set_code_table(table);
                }
                ui::post_refresh();
            }
        }
    }
}

/// CPU time used by this process, and its private memory.
pub fn process_usage() -> (std::time::Duration, u64) {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    unsafe {
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user);
        let ticks = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64; // 100 ns
        let cpu = std::time::Duration::from_nanos((ticks(kernel) + ticks(user)) * 100);
        let mut mem: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        mem.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        GetProcessMemoryInfo(GetCurrentProcess(), &mut mem as *mut _ as *mut PROCESS_MEMORY_COUNTERS, mem.cb);
        (cpu, mem.PrivateUsage as u64)
    }
}

/// Run the app (returns when it quits).
pub fn run() {
    reg::import_openkey();
    if !ui::acquire_single_instance() {
        return;
    }
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(App::new());
    ui::run_app();
}
