//! macOS front end: menu bar app + CGEventTap.

pub mod prefs;
pub mod sys;
pub mod tap;
pub mod ui;

use crate::convert::ConvertOptions;
use crate::engine::{Engine, Settings};
use crate::smart_switch::SmartSwitch;
use objc2_app_kit::NSWorkspace;
use std::cell::RefCell;

pub const BUNDLE_ID: &str = "com.vkeyengine.app";
/// the original Objective-C app, whose settings are imported on first run
pub const OPENKEY_BUNDLE_ID: &str = "com.tuyenmai.openkey";
/// option + z
pub const DEFAULT_SWITCH_KEY: i64 = 0x7A000206;
pub const EMPTY_HOTKEY: i64 = 0xFE0000FE;

/// Hot key layout: bits 0-7 key code (0xFE: modifiers only), bit 8 control, 9 option,
/// 10 command, 11 shift, 15 beep, bits 24-31 character shown in the UI.
pub mod hotkey {
    pub const CONTROL: i64 = 0x100;
    pub const OPTION: i64 = 0x200;
    pub const COMMAND: i64 = 0x400;
    pub const SHIFT: i64 = 0x800;
    pub const BEEP: i64 = 0x8000;
    pub const NO_KEY: i64 = 0xFE;

    pub fn key(h: i64) -> u16 {
        (h & 0xFF) as u16
    }

    pub fn describe(h: i64) -> String {
        let mut parts = Vec::new();
        for (bit, s) in [(CONTROL, "⌃"), (OPTION, "⌥"), (COMMAND, "⌘"), (SHIFT, "⇧")] {
            if h & bit != 0 {
                parts.push(s.to_string());
            }
        }
        let k = (h >> 24) & 0xFF;
        if key(h) != NO_KEY as u16 {
            parts.push(if key(h) == crate::tables::KEY_SPACE { "Space".into() } else { (k as u8 as char).to_uppercase().to_string() });
        }
        parts.join(" + ")
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
    pub other_language: bool,
    pub temp_off_app: bool,
    pub fix_recommend_browser: bool,
    pub fix_chromium_browser: bool,
    pub perform_layout_compat: bool,
    pub convert: ConvertOptions,
    pub convert_alert: bool,
    pub convert_hotkey: i64,
}

pub struct App {
    pub engine: Engine,
    pub opt: Options,
    pub smart: SmartSwitch,
    pub tap: tap::Tap,
    /// bundle id of the frontmost app other than VKeyEngine
    pub front_app: String,
    pub english_input_source: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Run `f` with the app state. Returns None if the state is busy (re-entrant call),
/// e.g. the event tap firing while a modal dialog runs inside a handler.
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn read_settings() -> (Settings, Options) {
    use prefs::{flag, int};
    let s = Settings {
        language: int("InputMethod", 1) as u8,
        input_type: int("InputType", 0).clamp(0, 3) as u8,
        free_mark: flag("FreeMark", false),
        code_table: int("CodeTable", 0).clamp(0, 4) as u8,
        check_spelling: flag("Spelling", true),
        modern_orthography: flag("ModernOrthography", false),
        quick_telex: flag("QuickTelex", false),
        restore_if_wrong_spelling: flag("RestoreIfInvalidWord", false),
        use_macro: flag("UseMacro", true),
        use_macro_in_english: flag("UseMacroInEnglishMode", false),
        auto_caps_macro: flag("vAutoCapsMacro", false),
        upper_case_first_char: flag("UpperCaseFirstChar", false),
        allow_consonant_zfwj: flag("vAllowConsonantZFWJ", false),
        quick_start_consonant: flag("vQuickStartConsonant", false),
        quick_end_consonant: flag("vQuickEndConsonant", false),
    };
    let mut switch_key = int("SwitchKeyStatus", DEFAULT_SWITCH_KEY);
    if switch_key == 0 {
        switch_key = DEFAULT_SWITCH_KEY;
    }
    let mut convert_hotkey = int("convertToolHotKey", EMPTY_HOTKEY);
    if convert_hotkey == 0 {
        convert_hotkey = EMPTY_HOTKEY;
    }
    let o = Options {
        switch_key,
        send_key_step_by_step: flag("SendKeyStepByStep", false),
        use_smart_switch_key: flag("UseSmartSwitchKey", true),
        temp_off_spelling: flag("vTempOffSpelling", false),
        remember_code: flag("vRememberCode", true),
        other_language: flag("vOtherLanguage", true),
        temp_off_app: flag("vTempOffOpenKey", false),
        fix_recommend_browser: flag("FixRecommendBrowser", true),
        fix_chromium_browser: flag("vFixChromiumBrowser", false),
        perform_layout_compat: flag("vPerformLayoutCompat", false),
        convert: ConvertOptions {
            to_all_caps: flag("convertToolToAllCaps", false),
            to_all_non_caps: flag("convertToolToAllNonCaps", false),
            caps_first_letter: flag("convertToolToCapsFirstLetter", false),
            caps_each_word: flag("convertToolToCapsEachWord", false),
            remove_mark: flag("convertToolRemoveMark", false),
            from_code: int("convertToolFromCode", 0).clamp(0, 4) as u8,
            to_code: int("convertToolToCode", 0).clamp(0, 4) as u8,
        },
        // the pref name is historical: it stores "alert when completed"
        convert_alert: flag("convertToolDontAlertWhenCompleted", false),
        convert_hotkey,
    };
    (s, o)
}

impl App {
    fn new() -> App {
        prefs::import_domain(OPENKEY_BUNDLE_ID);
        let (s, opt) = read_settings();
        let mut engine = Engine::new(s);
        let table = engine.s.code_table;
        engine.macros.load_binary(&prefs::data("macroData"), table);
        let mut smart = SmartSwitch::default();
        smart.load_binary(&prefs::data("smartSwitchKey"));
        App {
            engine,
            opt,
            smart,
            tap: tap::Tap::new(),
            front_app: String::new(),
            english_input_source: sys::input_source_is_english(),
        }
    }

    /// Re-read preferences after the settings UI changed them.
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
        prefs::set_data("macroData", &self.engine.macros.save_binary());
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
        prefs::set_data("smartSwitchKey", &self.smart.save_binary());
    }

    fn on_code_table_changed(&mut self) {
        let t = self.engine.s.code_table;
        self.engine.set_code_table(t);
        self.tap.clear_sync();
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

    /// The UI is refreshed after the current event: this also runs inside the key callback.
    pub fn set_language(&mut self, lang: u8) {
        prefs::set_int("InputMethod", lang as i64);
        self.engine.s.language = lang;
        self.on_language_changed();
        ui::defer(ui::refresh);
    }

    pub fn set_code_table(&mut self, t: u8) {
        prefs::set_int("CodeTable", t as i64);
        self.engine.s.code_table = t;
        self.on_code_table_changed();
        ui::defer(ui::refresh);
    }

    /// Smart switch: restore the language / code table remembered for the new front app.
    pub fn on_active_app_changed(&mut self) {
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else { return };
        let id = app
            .bundleIdentifier()
            .or_else(|| app.localizedName())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "UnknownApp".into());
        if id == BUNDLE_ID {
            return;
        }
        self.front_app = id;
        self.engine.start_new_session();
        self.tap.clear_sync();
        // either option alone must work (before, "remember code table" needed smart switch on)
        if !self.opt.use_smart_switch_key && !self.opt.remember_code {
            return;
        }
        let current = self.smart_value();
        match self.smart.get(&self.front_app, current) {
            None => prefs::set_data("smartSwitchKey", &self.smart.save_binary()),
            Some(v) => {
                let lang = (v & 1) as u8;
                if self.opt.use_smart_switch_key && lang != self.engine.s.language {
                    prefs::set_int("InputMethod", lang as i64);
                    self.engine.s.language = lang;
                    self.engine.start_new_session();
                }
                let table = ((v >> 1) as u8).min(4);
                if self.opt.remember_code && table != self.engine.s.code_table {
                    prefs::set_int("CodeTable", table as i64);
                    self.engine.s.code_table = table;
                    self.engine.set_code_table(table);
                }
                ui::defer(ui::refresh);
            }
        }
    }
}

/// Run the app (never returns).
pub fn run() {
    if !ui::acquire_single_instance_lock() {
        return;
    }
    APP.with(|a| *a.borrow_mut() = Some(App::new()));
    ui::run_app();
}
