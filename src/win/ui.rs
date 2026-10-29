//! Tray icon and windows. Same pages and look as the macOS app: monochrome, options grouped
//! in rounded cards (label left, control right). Win32 has no such controls, so cards,
//! switches and buttons are painted here (GDI+ shapes, GDI text); drop-down lists, text
//! fields and the macro table are native controls, themed light / dark with the system.
//!
//! Every switch / drop-down is a row of `TOGGLES` / `POPUPS` (registry value + label). One
//! generic action writes the value, the app re-reads its settings and the windows repaint
//! from the registry, so there is no per-option glue code.

use super::{hook, hotkey, read_settings, reg, reg::wide, with_app, DEFAULT_SWITCH_KEY, EMPTY_HOTKEY};
use crate::convert::convert;
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::GdiPlus::{
    FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill,
    GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDrawPath, GdipFillEllipse, GdipFillPath,
    GdipSetSmoothingMode, GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, GpBrush, GpGraphics, SmoothingModeAntiAlias, UnitPixel,
    GdipDrawLines, GdipFillPolygon, PointF,
};
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------- tags

const T_LANG: isize = 1;
const T_ENGLISH: isize = 2;
const T_VIETNAMESE: isize = 3;
const T_INPUT: isize = 10; // + input type
const T_CODE: isize = 20; // + code table
const T_CONVERT_TOOL: isize = 30;
const T_QUICK_CONVERT: isize = 31;
const T_SETTINGS: isize = 32;
const T_MACROS: isize = 33;
const T_ABOUT: isize = 34;
const T_QUIT: isize = 35;
const T_PAGE: isize = 40; // + page
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
const ID_NAME: isize = 600;
const ID_CONTENT: isize = 601;
const ID_LIST: isize = 602;

const WM_TRAY: u32 = WM_APP + 10;
const WM_REFRESH: u32 = WM_APP + 11;
const WM_QUICK_CONVERT: u32 = WM_APP + 12;
const WM_SHOW_SETTINGS: u32 = WM_APP + 13;
const TIMER_HOOK: usize = 1;

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
    /// 0: the value is a bool; otherwise the bit to flip in an int value
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
    t("vCheckSpelling", true, "Kiểm tra chính tả", P_SPELL),
    t("vRestoreIfWrongSpelling", true, "Tự khôi phục phím gõ nếu từ sai chính tả", P_SPELL),
    t("vTempOffSpelling", false, "Tạm tắt kiểm tra chính tả bằng phím Ctrl", P_SPELL),
    t("vAllowConsonantZFWJ", false, "Cho phép phụ âm đầu Z, F, W, J", P_SPELL),
    t("vUseModernOrthography", false, "Đặt dấu kiểu mới (oà, uý thay vì òa, úy)", P_SPELL),
    t("vQuickTelex", false, "Gõ nhanh: cc→ch gg→gi kk→kh nn→ng qq→qu pp→ph tt→th", P_QUICK),
    t("vQuickStartConsonant", false, "Gõ tắt phụ âm đầu: f→ph j→gi w→qu", P_QUICK),
    t("vQuickEndConsonant", false, "Gõ tắt phụ âm cuối: g→ng h→nh k→ch", P_QUICK),
    t("vUpperCaseFirstChar", false, "Tự viết hoa chữ cái đầu câu", P_QUICK),
    t("vUseMacro", true, "Bật gõ tắt", P_MACRO),
    t("vUseMacroInEnglishMode", false, "Gõ tắt cả khi đang ở chế độ tiếng Anh", P_MACRO),
    t("vAutoCapsMacro", false, "Tự viết hoa theo từ gõ tắt (btw → Btw → BTW)", P_MACRO),
    t("vRunWithWindows", true, "Khởi động cùng Windows", P_STARTUP),
    t("vShowOnStartUp", true, "Hiện bảng điều khiển khi khởi động", P_STARTUP),
    t("vUseSmartSwitchKey", true, "Tự nhớ chế độ gõ Việt / Anh theo từng ứng dụng", P_SMART),
    t("vRememberCode", true, "Tự nhớ bảng mã theo từng ứng dụng", P_SMART),
    t("vTempOffOpenKey", false, "Tạm tắt VKeyEngine bằng phím Alt", P_SMART),
    t("vFixRecommendBrowser", true, "Sửa lỗi gợi ý từ (thanh địa chỉ trình duyệt, Excel...)", P_COMPAT),
    t("vFixChromiumBrowser", false, "Sửa lỗi gợi ý cho Chrome / Edge / Brave (thử nghiệm)", P_COMPAT),
    t("vSendKeyStepByStep", true, "Gửi từng phím một (cho ứng dụng cũ)", P_COMPAT),
    t("convertToolRemoveMark", false, "Bỏ dấu", P_CONVERT),
    t("convertToolDontAlertWhenCompleted", false, "Thông báo khi chuyển mã xong", P_CONVERT),
    hk("vSwitchKeyStatus", hotkey::CONTROL, DEFAULT_SWITCH_KEY, "Ctrl", P_SWITCH_KEY),
    hk("vSwitchKeyStatus", hotkey::ALT, DEFAULT_SWITCH_KEY, "Alt", P_SWITCH_KEY),
    hk("vSwitchKeyStatus", hotkey::WIN, DEFAULT_SWITCH_KEY, "Win", P_SWITCH_KEY),
    hk("vSwitchKeyStatus", hotkey::SHIFT, DEFAULT_SWITCH_KEY, "Shift", P_SWITCH_KEY),
    hk("vSwitchKeyStatus", hotkey::BEEP, DEFAULT_SWITCH_KEY, "Âm báo khi chuyển chế độ", P_SWITCH_KEY),
    hk("convertToolHotKey", hotkey::CONTROL, EMPTY_HOTKEY, "Ctrl", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::ALT, EMPTY_HOTKEY, "Alt", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::WIN, EMPTY_HOTKEY, "Win", P_CONVERT_KEY),
    hk("convertToolHotKey", hotkey::SHIFT, EMPTY_HOTKEY, "Shift", P_CONVERT_KEY),
];

/// Values that need a value even when 0 means "unset".
fn int_pref(key: &str, default: i64) -> i64 {
    match reg::int(key, default) {
        0 if default > 1 => default,
        v => v,
    }
}

fn toggle_on(t: &Toggle) -> bool {
    if t.bit == 0 {
        reg::flag(t.key, t.default != 0)
    } else {
        int_pref(t.key, t.default) & t.bit != 0
    }
}

fn toggle_enabled(t: &Toggle) -> bool {
    match t.key {
        "vRestoreIfWrongSpelling" | "vTempOffSpelling" | "vAllowConsonantZFWJ" => reg::flag("vCheckSpelling", true),
        "vFixChromiumBrowser" => reg::flag("vFixRecommendBrowser", true),
        _ => true,
    }
}

fn toggle_index(key: &str, bit: i64) -> usize {
    TOGGLES.iter().position(|t| t.key == key && t.bit == bit).expect("toggle")
}

struct Popup {
    /// "" = convert case options (4 exclusive bool values)
    key: &'static str,
    label: &'static str,
    items: &'static [&'static str],
    default: i64,
}

const CASE_KEYS: [&str; 4] =
    ["convertToolToAllCaps", "convertToolToAllNonCaps", "convertToolToCapsFirstLetter", "convertToolToCapsEachWord"];

static POPUPS: &[Popup] = &[
    Popup { key: "vLanguage", label: "Chế độ gõ", items: &["Tiếng Anh", "Tiếng Việt"], default: 1 },
    Popup { key: "vInputType", label: "Kiểu gõ", items: &INPUT_TYPES, default: 0 },
    Popup { key: "vCodeTable", label: "Bảng mã", items: &CODE_TABLES, default: 0 },
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
        return CASE_KEYS.iter().position(|k| reg::flag(k, false)).map_or(0, |i| i as i64 + 1);
    }
    reg::int(p.key, p.default)
}

fn set_popup_value(i: usize, v: i64) {
    let p = &POPUPS[i];
    if p.key.is_empty() {
        for (n, k) in CASE_KEYS.iter().enumerate() {
            reg::set_int(k, (v == n as i64 + 1) as i64);
        }
    } else {
        reg::set_int(p.key, v);
    }
}

// ---------------------------------------------------------------- theme

/// 0xRRGGBB colors of the light or dark theme.
#[derive(Clone, Copy)]
struct Theme {
    dark: bool,
    bg: u32,
    card: u32,
    border: u32,
    text: u32,
    secondary: u32,
    accent: u32,
    on_accent: u32,
    track_off: u32,
    knob_off: u32,
    button: u32,
    hover: u32,
}

const LIGHT: Theme = Theme {
    dark: false,
    bg: 0xFFFFFF,
    card: 0xF6F6F6,
    border: 0xE0E0E0,
    text: 0x000000,
    secondary: 0x6E6E73,
    accent: 0x000000,
    on_accent: 0xFFFFFF,
    track_off: 0xD4D4D4,
    knob_off: 0xFFFFFF,
    button: 0xEAEAEA,
    hover: 0xDCDCDC,
};

const DARK: Theme = Theme {
    dark: true,
    bg: 0x000000,
    card: 0x141414,
    border: 0x2E2E2E,
    text: 0xFFFFFF,
    secondary: 0x98989D,
    accent: 0xFFFFFF,
    on_accent: 0x000000,
    track_off: 0x3A3A3A,
    knob_off: 0xE5E5E5,
    button: 0x262626,
    hover: 0x333333,
};

fn personalize(value: &str, default: bool) -> bool {
    let mut v = 0u32;
    let mut size = 4u32;
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let r = unsafe {
        windows_sys::Win32::System::Registry::RegGetValueW(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            key.as_ptr(),
            wide(value).as_ptr(),
            windows_sys::Win32::System::Registry::RRF_RT_REG_DWORD,
            null_mut(),
            &mut v as *mut u32 as _,
            &mut size,
        )
    };
    if r == ERROR_SUCCESS {
        v != 0
    } else {
        default
    }
}

fn theme() -> Theme {
    if personalize("AppsUseLightTheme", true) {
        LIGHT
    } else {
        DARK
    }
}

/// 0xRRGGBB -> GDI COLORREF (0x00BBGGRR)
fn colorref(c: u32) -> COLORREF {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

/// 0xRRGGBB -> GDI+ ARGB
fn argb(c: u32, alpha: u8) -> u32 {
    ((alpha as u32) << 24) | c
}

fn blend(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

// ---------------------------------------------------------------- layout model

/// Rectangle in device independent pixels (96 dpi).
#[derive(Clone, Copy, Default)]
struct R {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl R {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

enum Item {
    /// `toggle`: the switch this is the label of (dimmed when the switch is disabled)
    Text { r: R, s: String, size: f32, bold: bool, secondary: bool, center: bool, toggle: Option<usize> },
    Card(R),
    Line(R),
    /// rounded frame behind a native text field
    Field(R),
    Switch { r: R, toggle: usize },
    /// Ctrl / Alt / Win / Shift pill of a hot key
    Mod { r: R, toggle: usize },
    Button { r: R, label: String, tag: isize, primary: bool },
    Segment { r: R, labels: &'static [&'static str], tag: isize },
    Record { r: R, tag: isize },
    Icon(R),
    /// value of ACTIVITY_ROWS[n], filled at paint time
    Stat { r: R, n: usize },
    /// CPU history graph
    Graph(R),
}

impl Item {
    fn rect(&self) -> R {
        match self {
            Item::Text { r, .. } | Item::Switch { r, .. } | Item::Mod { r, .. } | Item::Button { r, .. } => *r,
            Item::Segment { r, .. } | Item::Record { r, .. } => *r,
            Item::Card(r) | Item::Line(r) | Item::Field(r) | Item::Icon(r) | Item::Graph(r) => *r,
            Item::Stat { r, .. } => *r,
        }
    }

    fn interactive(&self) -> bool {
        matches!(self, Item::Switch { .. } | Item::Mod { .. } | Item::Button { .. } | Item::Segment { .. } | Item::Record { .. })
    }
}

enum Native {
    Combo(usize),
    Edit(isize, &'static str),
    List,
}

enum Ctl {
    Switch(usize),
    Popup(usize, f32),
    Hotkey(u8, isize),
    Stat(usize),
    Edit(isize, &'static str, f32),
}

const PAD: f32 = 24.0;
const ROW_H: f32 = 40.0;
const INSET: f32 = 14.0;

/// Top-down layout of a window or settings page.
struct Page {
    items: Vec<Item>,
    natives: Vec<(Native, R)>,
    y: f32,
    w: f32,
}

impl Page {
    fn new(w: f32) -> Page {
        Page { items: Vec::new(), natives: Vec::new(), y: 0.0, w }
    }

    fn text(&mut self, r: R, s: &str, size: f32, bold: bool, secondary: bool) {
        self.items.push(Item::Text { r, s: s.into(), size, bold, secondary, center: false, toggle: None });
    }

    fn header(&mut self, title: &str, subtitle: &str) {
        self.y = 20.0;
        self.text(R { x: PAD, y: self.y, w: self.w - 2.0 * PAD, h: 30.0 }, title, 24.0, true, false);
        self.text(R { x: PAD, y: self.y + 32.0, w: self.w - 2.0 * PAD, h: 18.0 }, subtitle, 12.0, false, true);
        self.y += 58.0;
    }

    fn section(&mut self, title: &str) {
        self.y += 14.0;
        let r = R { x: PAD + INSET, y: self.y, w: self.w - 2.0 * PAD, h: 16.0 };
        self.text(r, &title.to_uppercase(), 11.0, true, true);
        self.y += 22.0;
    }

    /// One row per (label, control), separated by hairlines; controls are right-aligned.
    fn card(&mut self, rows: Vec<(&str, Ctl)>) {
        let (x, w) = (PAD, self.w - 2.0 * PAD);
        let h = ROW_H * rows.len() as f32;
        self.items.push(Item::Card(R { x, y: self.y, w, h }));
        for (i, (label, ctl)) in rows.into_iter().enumerate() {
            let top = self.y + ROW_H * i as f32;
            if i > 0 {
                self.items.push(Item::Line(R { x: x + INSET, y: top, w: w - 2.0 * INSET, h: 1.0 }));
            }
            let right = x + w - INSET;
            let at = |cw: f32, ch: f32| R { x: right - cw, y: top + (ROW_H - ch) / 2.0, w: cw, h: ch };
            let cw = match &ctl {
                Ctl::Switch(t) => {
                    self.items.push(Item::Switch { r: at(40.0, 22.0), toggle: *t });
                    40.0
                }
                Ctl::Popup(p, pw) => {
                    self.natives.push((Native::Combo(*p), at(*pw, 24.0)));
                    *pw
                }
                Ctl::Edit(id, placeholder, ew) => {
                    let r = at(*ew, 28.0);
                    self.items.push(Item::Field(r));
                    self.natives.push((Native::Edit(*id, placeholder), R { x: r.x + 8.0, y: r.y + 5.0, w: r.w - 16.0, h: 18.0 }));
                    *ew
                }
                Ctl::Stat(n) => {
                    self.items.push(Item::Stat { r: at(230.0, ROW_H), n: *n });
                    230.0
                }
                Ctl::Hotkey(page, tag) => {
                    let mods: Vec<usize> = (0..TOGGLES.len())
                        .filter(|&i| TOGGLES[i].page == *page && TOGGLES[i].bit != hotkey::BEEP)
                        .collect();
                    let total = mods.len() as f32 * 48.0 + 6.0 + 110.0;
                    let mut mx = right - total;
                    for m in mods {
                        self.items.push(Item::Mod { r: R { x: mx, y: top + 8.0, w: 44.0, h: 24.0 }, toggle: m });
                        mx += 48.0;
                    }
                    self.items.push(Item::Record { r: R { x: mx + 6.0, y: top + 8.0, w: 110.0, h: 24.0 }, tag: *tag });
                    total
                }
            };
            let r = R { x: x + INSET, y: top, w: w - 3.0 * INSET - cw, h: ROW_H };
            let toggle = match ctl {
                Ctl::Switch(t) => Some(t),
                _ => None,
            };
            self.items.push(Item::Text { r, s: label.into(), size: 13.0, bold: false, secondary: false, center: false, toggle });
        }
        self.y += h;
    }

    fn toggle_card(&mut self, page: u8) {
        let rows = (0..TOGGLES.len()).filter(|&i| TOGGLES[i].page == page).map(|i| (TOGGLES[i].title, Ctl::Switch(i))).collect();
        self.card(rows);
    }

    fn note(&mut self, s: &str) {
        self.y += 10.0;
        let r = R { x: PAD + INSET, y: self.y, w: self.w - 2.0 * (PAD + INSET), h: 18.0 };
        self.text(r, s, 12.0, false, true);
        self.y += 18.0;
    }

    /// Buttons aligned left and right; `primary` is filled with the accent color.
    fn buttons(&mut self, left: &[(&str, isize)], right: &[(&str, isize)], primary: isize) {
        self.y += 18.0;
        let width = |s: &str| (s.chars().count() as f32 * 7.5 + 32.0).max(84.0);
        let mut x = PAD;
        for &(label, tag) in left {
            let w = width(label);
            self.items.push(Item::Button { r: R { x, y: self.y, w, h: 32.0 }, label: label.into(), tag, primary: tag == primary });
            x += w + 8.0;
        }
        let mut x = self.w - PAD;
        for &(label, tag) in right.iter().rev() {
            let w = width(label);
            x -= w;
            self.items.push(Item::Button { r: R { x, y: self.y, w, h: 32.0 }, label: label.into(), tag, primary: tag == primary });
            x -= 8.0;
        }
        self.y += 32.0;
    }

    fn finish(&mut self) -> f32 {
        self.y += PAD;
        self.y
    }
}

// ---------------------------------------------------------------- windows state

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Settings,
    Macros,
    Convert,
    About,
}

struct Win {
    hwnd: HWND,
    kind: Kind,
    page: usize,
    items: Vec<Item>,
    /// native child windows and their rectangles
    natives: Vec<(HWND, R)>,
    /// client size in DIPs
    width: f32,
    height: f32,
    hover: Option<usize>,
    focus: Option<usize>,
    /// hot key being recorded (T_RECORD_*)
    recording: Option<isize>,
}

thread_local! {
    static WINS: RefCell<Vec<Win>> = const { RefCell::new(Vec::new()) };
    static FONTS: RefCell<Vec<(i32, bool, HFONT)>> = const { RefCell::new(Vec::new()) };
    static BRUSH: RefCell<(u32, HBRUSH)> = const { RefCell::new((u32::MAX, null_mut())) };
    static TRAY_ICON: RefCell<HICON> = const { RefCell::new(null_mut()) };
}

static MAIN: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicIsize = AtomicIsize::new(0);
const MAIN_CLASS: &str = "VKeyEngineMain";
const WIN_CLASS: &str = "VKeyEngineWindow";

fn main_hwnd() -> HWND {
    MAIN.load(Ordering::SeqCst) as HWND
}

/// Run `f` on the window, unless its state is already borrowed (re-entrant message).
fn with_win<R>(hwnd: HWND, f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WINS.with(|w| {
        let mut w = w.try_borrow_mut().ok()?;
        w.iter_mut().find(|x| x.hwnd == hwnd).map(f)
    })
}

fn find_kind(kind: Kind) -> Option<HWND> {
    WINS.with(|w| w.borrow().iter().find(|x| x.kind == kind).map(|x| x.hwnd))
}

fn all_windows() -> Vec<HWND> {
    WINS.with(|w| w.borrow().iter().map(|x| x.hwnd).collect())
}

fn scale(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd).max(96) as f32 / 96.0 }
}

fn font(px: f32, bold: bool, s: f32) -> HFONT {
    let h = -(px * s).round() as i32;
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        if let Some(&(_, _, font)) = f.iter().find(|(fh, fb, _)| *fh == h && *fb == bold) {
            return font;
        }
        let face = wide("Segoe UI");
        let weight = if bold { 600 } else { 400 };
        let font = unsafe { CreateFontW(h, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr()) };
        f.push((h, bold, font));
        font
    })
}

/// Brush for native control backgrounds (card color).
fn card_brush() -> HBRUSH {
    let c = theme().card;
    BRUSH.with(|b| {
        let mut b = b.borrow_mut();
        if b.0 != c {
            if !b.1.is_null() {
                unsafe { DeleteObject(b.1) };
            }
            *b = (c, unsafe { CreateSolidBrush(colorref(c)) });
        }
        b.1
    })
}

// ---------------------------------------------------------------- startup

const RESTARTED_ARG: &str = "--restarted";

/// One VKeyEngine per user session; a second launch opens the settings of the first. A copy
/// started after a crash waits for the crashed one to be gone.
pub fn acquire_single_instance() -> bool {
    let tries = if std::env::args().any(|a| a == RESTARTED_ARG) { 50 } else { 1 };
    for i in 0..tries {
        unsafe {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let mutex = CreateMutexW(null(), 0, wide("Local\\VKeyEngine").as_ptr());
            if GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            CloseHandle(mutex);
        }
    }
    let other = unsafe { FindWindowW(wide(MAIN_CLASS).as_ptr(), null()) };
    if !other.is_null() {
        unsafe { PostMessageW(other, WM_SHOW_SETTINGS, 0, 0) };
    }
    false
}

/// Start again after a crash: a panic starts a new copy before the process aborts, and
/// Windows Error Reporting restarts us after other crashes or a hang. Not when the crash
/// comes within 10 s of starting, which would loop.
fn restart_after_crash() {
    let started = std::time::Instant::now();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if started.elapsed().as_secs() >= 10 {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).arg(RESTARTED_ARG).spawn();
            }
        }
    }));
    let arg = wide(RESTARTED_ARG);
    unsafe {
        windows_sys::Win32::System::Recovery::RegisterApplicationRestart(
            arg.as_ptr(),
            windows_sys::Win32::System::Recovery::RESTART_NO_PATCH | windows_sys::Win32::System::Recovery::RESTART_NO_REBOOT,
        )
    };
}

/// Dark context menus follow the system theme (uxtheme ordinals 135 / 136, used by Explorer).
unsafe fn allow_dark_menus() {
    let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
    if lib.is_null() {
        return;
    }
    if let Some(f) = GetProcAddress(lib, 135 as _) {
        let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
        set_preferred_app_mode(1); // allow dark
    }
    if let Some(f) = GetProcAddress(lib, 136 as _) {
        let flush_menu_themes: unsafe extern "system" fn() = std::mem::transmute(f);
        flush_menu_themes();
    }
}

pub fn run_app() {
    restart_after_crash();
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..std::mem::zeroed() };
        GdiplusStartup(&mut token, &input, null_mut());
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&icc);
        allow_dark_menus();

        let instance = GetModuleHandleW(null());
        let icon = app_icon(32);
        for (name, proc_) in [(MAIN_CLASS, main_proc as WndProc), (WIN_CLASS, win_proc as WndProc)] {
            let name = wide(name);
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_),
                hInstance: instance,
                hIcon: icon,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                lpszClassName: name.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassExW(&class);
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) as isize, Ordering::SeqCst);
        let main = CreateWindowExW(0, wide(MAIN_CLASS).as_ptr(), wide("VKeyEngine").as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), instance, null());
        MAIN.store(main as isize, Ordering::SeqCst);
        // every key press, independently of our hooks (see hook::check_alive)
        let keyboard = windows_sys::Win32::UI::Input::RAWINPUTDEVICE {
            usUsagePage: 1,
            usUsage: 6,
            dwFlags: windows_sys::Win32::UI::Input::RIDEV_INPUTSINK,
            hwndTarget: main,
        };
        windows_sys::Win32::UI::Input::RegisterRawInputDevices(&keyboard, 1, size_of::<windows_sys::Win32::UI::Input::RAWINPUTDEVICE>() as u32);
        tray(NIM_ADD);
        hook::start();
        SetTimer(main, TIMER_HOOK, 2000, None);
        reg::set_run_on_startup(reg::flag("vRunWithWindows", true));
        if reg::flag("vShowOnStartUp", true) {
            show(Kind::Settings);
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        GdiplusShutdown(token);
    }
}

type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

pub fn post_refresh() {
    unsafe { PostMessageW(main_hwnd(), WM_REFRESH, 0, 0) };
}

pub fn post_quick_convert() {
    unsafe { PostMessageW(main_hwnd(), WM_QUICK_CONVERT, 0, 0) };
}

// ---------------------------------------------------------------- icons

/// Icon of `size` px from an embedded .ico file (frames are PNG).
fn icon_from_ico(ico: &[u8], size: i32) -> HICON {
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let entry = |i: usize| {
        let e = &ico[6 + 16 * i..22 + 16 * i];
        let w = if e[0] == 0 { 256 } else { e[0] as i32 };
        let len = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        (w, off, len)
    };
    // smallest frame at least `size` wide, else the largest
    let best = (0..count)
        .map(entry)
        .filter(|e| e.0 >= size)
        .min_by_key(|e| e.0)
        .or_else(|| (0..count).map(entry).max_by_key(|e| e.0));
    let Some((_, off, len)) = best else { return null_mut() };
    unsafe { CreateIconFromResourceEx(ico[off..off + len].as_ptr(), len as u32, 1, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }
}

fn app_icon(size: i32) -> HICON {
    icon_from_ico(include_bytes!("../../windows/app.ico"), size)
}

/// Black glyph on a light taskbar, white on a dark one.
fn tray_icon(vietnamese: bool, size: i32) -> HICON {
    let light = personalize("SystemUsesLightTheme", false);
    let ico: &[u8] = match (vietnamese, light) {
        (true, true) => include_bytes!("../../windows/tray-v-black.ico"),
        (true, false) => include_bytes!("../../windows/tray-v-white.ico"),
        (false, true) => include_bytes!("../../windows/tray-e-black.ico"),
        (false, false) => include_bytes!("../../windows/tray-e-white.ico"),
    };
    icon_from_ico(ico, size)
}

fn copy_wide(dst: &mut [u16], s: &str) {
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().chain([0])) {
        *d = c;
    }
    if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

fn tray(message: NOTIFY_ICON_MESSAGE) {
    let (s, _) = read_settings();
    let vietnamese = s.language == 1;
    unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(main_hwnd()).max(96));
        let icon = if message == NIM_DELETE { null_mut() } else { tray_icon(vietnamese, size) };
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: main_hwnd(),
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            ..std::mem::zeroed()
        };
        let mode = if vietnamese { "Tiếng Việt" } else { "Tiếng Anh" };
        let tip = if hook::FRONT_ELEVATED.load(Ordering::SeqCst) {
            "VKeyEngine · ⚠ Ứng dụng này chạy quyền Admin: chạy VKeyEngine bằng quyền Admin để gõ được".to_string()
        } else {
            format!("VKeyEngine · {mode} · {}", INPUT_TYPES[s.input_type as usize])
        };
        copy_wide(&mut data.szTip, &tip);
        if Shell_NotifyIconW(message, &data) == 0 && message == NIM_MODIFY {
            Shell_NotifyIconW(NIM_ADD, &data);
        }
        TRAY_ICON.with(|t| {
            let old = t.replace(icon);
            if !old.is_null() {
                DestroyIcon(old);
            }
        });
    }
}

fn tray_menu() {
    let (s, o) = read_settings();
    unsafe {
        let menu = CreatePopupMenu();
        let add = |m: HMENU, tag: isize, s: &str, flags: MENU_ITEM_FLAGS| {
            AppendMenuW(m, MF_STRING | flags, tag as usize, wide(s).as_ptr());
        };
        add(menu, T_VIETNAMESE, "Tiếng Việt", 0);
        add(menu, T_ENGLISH, "Tiếng Anh", 0);
        let current = if s.language == 1 { T_VIETNAMESE } else { T_ENGLISH };
        CheckMenuRadioItem(menu, T_ENGLISH as u32, T_VIETNAMESE as u32, current as u32, MF_BYCOMMAND);
        SetMenuDefaultItem(menu, current as u32, 0); // bold
        AppendMenuW(menu, MF_SEPARATOR, 0, null());

        let sub = |items: &[&str], base: isize, selected: usize| {
            let m = CreatePopupMenu();
            for (i, s) in items.iter().enumerate() {
                add(m, base + i as isize, s, 0);
            }
            CheckMenuRadioItem(m, base as u32, (base + items.len() as isize - 1) as u32, (base + selected as isize) as u32, MF_BYCOMMAND);
            m
        };
        let inputs = sub(&INPUT_TYPES, T_INPUT, s.input_type as usize);
        AppendMenuW(menu, MF_POPUP, inputs as usize, wide(&format!("Kiểu gõ: {}", INPUT_TYPES[s.input_type as usize])).as_ptr());
        let codes = sub(&CODE_TABLES, T_CODE, s.code_table as usize);
        AppendMenuW(menu, MF_POPUP, codes as usize, wide(&format!("Bảng mã: {}", CODE_TABLES[s.code_table as usize])).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(menu, T_CONVERT_TOOL, "Công cụ chuyển mã…", 0);
        let key = hotkey::describe(o.convert_hotkey);
        let quick = if key.is_empty() { "Chuyển mã nhanh".to_string() } else { format!("Chuyển mã nhanh\t{key}") };
        add(menu, T_QUICK_CONVERT, &quick, 0);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(menu, T_SETTINGS, "Bảng điều khiển…", 0);
        add(menu, T_MACROS, "Gõ tắt…", 0);
        add(menu, T_ABOUT, "Giới thiệu", 0);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(menu, T_QUIT, "Thoát VKeyEngine", 0);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(main_hwnd());
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, main_hwnd(), null());
        PostMessageW(main_hwnd(), WM_NULL, 0, 0);
        DestroyMenu(menu);
        if cmd != 0 {
            action(null_mut(), cmd as isize);
        }
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP => action(null_mut(), T_LANG),
                WM_LBUTTONDBLCLK => show(Kind::Settings),
                WM_RBUTTONUP | WM_CONTEXTMENU => tray_menu(),
                _ => {}
            }
            0
        }
        WM_REFRESH => {
            refresh();
            0
        }
        WM_QUICK_CONVERT => {
            quick_convert(false);
            0
        }
        WM_SHOW_SETTINGS => {
            show(Kind::Settings);
            0
        }
        WM_INPUT => {
            hook::raw_key_seen();
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_TIMER if wparam == TIMER_HOOK => {
            hook::check_alive();
            let (cpu, memory) = super::process_usage();
            crate::stats::sample(cpu, memory);
            for hwnd in all_windows() {
                if with_win(hwnd, |w| w.kind == Kind::Settings && w.page == ACTIVITY_PAGE) == Some(true) {
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_SETTINGCHANGE => {
            theme_changed();
            0
        }
        WM_DESTROY => {
            tray(NIM_DELETE);
            PostQuitMessage(0);
            0
        }
        _ if msg as isize == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            tray(NIM_ADD); // Explorer restarted
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ---------------------------------------------------------------- actions

fn reload() {
    with_app(|a| a.reload_settings());
    refresh();
}

/// Update the tray icon and every open window from the registry.
fn refresh() {
    tray(NIM_MODIFY);
    for hwnd in all_windows() {
        let combos: Vec<(HWND, usize)> = with_win(hwnd, |w| {
            w.natives.iter().filter_map(|&(h, _)| {
                let id = unsafe { GetDlgCtrlID(h) } as isize;
                (T_POPUP..T_RECORD_SWITCH).contains(&id).then(|| (h, (id - T_POPUP) as usize))
            }).collect()
        })
        .unwrap_or_default();
        for (h, i) in combos {
            unsafe { SendMessageW(h, CB_SETCURSEL, popup_value(i) as usize, 0) };
        }
        unsafe { InvalidateRect(hwnd, null(), 0) };
    }
}

fn theme_changed() {
    for hwnd in all_windows() {
        style_window(hwnd);
        let natives: Vec<HWND> = with_win(hwnd, |w| w.natives.iter().map(|n| n.0).collect()).unwrap_or_default();
        for h in natives {
            style_native(h);
        }
        unsafe { InvalidateRect(hwnd, null(), 1) };
    }
    refresh();
}

fn action(hwnd: HWND, tag: isize) {
    match tag {
        T_LANG | T_ENGLISH | T_VIETNAMESE => {
            with_app(|a| {
                let lang = match tag {
                    T_ENGLISH => 0,
                    T_VIETNAMESE => 1,
                    _ => 1 - a.engine.s.language.min(1),
                };
                a.set_language(lang);
            });
            refresh();
        }
        T_INPUT..=13 => {
            reg::set_int("vInputType", (tag - T_INPUT) as i64);
            reload();
        }
        T_CODE..=24 => {
            let t = (tag - T_CODE) as u8;
            with_app(|a| a.set_code_table(t));
            refresh();
        }
        T_CONVERT_TOOL => show(Kind::Convert),
        T_QUICK_CONVERT | T_CONVERT_RUN => quick_convert(tag == T_CONVERT_RUN),
        T_SETTINGS => show(Kind::Settings),
        T_MACROS => show(Kind::Macros),
        T_ABOUT => show(Kind::About),
        T_QUIT => unsafe {
            DestroyWindow(main_hwnd());
        },
        T_DEFAULTS => reset_defaults(hwnd),
        T_RECORD_SWITCH | T_RECORD_CONVERT => {
            with_win(hwnd, |w| w.recording = Some(tag));
            unsafe {
                SetFocus(hwnd);
                InvalidateRect(hwnd, null(), 0);
            }
        }
        T_MACRO_ADD => macro_add(hwnd),
        T_MACRO_DEL => macro_delete(hwnd),
        T_MACRO_IMPORT => macro_import(hwnd),
        T_MACRO_EXPORT => macro_export(hwnd),
        T_CONVERT_SWAP => {
            let (from, to) = (popup_value(POP_FROM), popup_value(POP_TO));
            set_popup_value(POP_FROM, to);
            set_popup_value(POP_TO, from);
            refresh();
        }
        T_CLOSE => unsafe {
            DestroyWindow(hwnd);
        },
        _ if (T_PAGE..T_PAGE + 4).contains(&tag) => show_page(hwnd, (tag - T_PAGE) as usize),
        _ if (T_TOGGLE..T_POPUP).contains(&tag) => {
            let Some(tg) = TOGGLES.get((tag - T_TOGGLE) as usize) else { return };
            if !toggle_enabled(tg) {
                return;
            }
            let on = !toggle_on(tg);
            if tg.bit == 0 {
                reg::set_int(tg.key, on as i64);
            } else {
                let v = int_pref(tg.key, tg.default);
                reg::set_int(tg.key, if on { v | tg.bit } else { v & !tg.bit });
            }
            if tg.key == "vRunWithWindows" {
                reg::set_run_on_startup(on);
            }
            reload();
        }
        _ => {}
    }
}

fn reset_defaults(hwnd: HWND) {
    if !confirm(hwnd, "Bạn có chắc chắn muốn thiết lập lại cấu hình mặc định?") {
        return;
    }
    for tg in TOGGLES.iter().filter(|t| t.bit == 0) {
        reg::set_int(tg.key, tg.default);
    }
    reg::set_int("vSwitchKeyStatus", DEFAULT_SWITCH_KEY);
    reg::set_int("convertToolHotKey", EMPTY_HOTKEY);
    for p in POPUPS.iter().filter(|p| !p.key.is_empty()) {
        reg::set_int(p.key, p.default);
    }
    set_popup_value(POP_CASE, 0);
    reg::set_run_on_startup(true);
    reload();
}

fn record_key(hwnd: HWND, tag: isize, vk: u16) {
    if modifier_key(vk) {
        return; // wait for the key itself
    }
    let (key, default) = if tag == T_RECORD_SWITCH { ("vSwitchKeyStatus", DEFAULT_SWITCH_KEY) } else { ("convertToolHotKey", EMPTY_HOTKEY) };
    let (k, c): (i64, i64) = match vk {
        VK_ESCAPE => (-1, -1),
        VK_BACK | VK_DELETE => (hotkey::NO_KEY, hotkey::NO_KEY),
        VK_SPACE => (0x20, 0x20),
        _ => {
            let ch = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_CHAR) } & 0xFFFF;
            (vk as i64, (ch as u8).to_ascii_lowercase() as i64)
        }
    };
    if k >= 0 {
        let v = int_pref(key, default);
        reg::set_int(key, (v & 0x00FF_FF00) | k | (c << 24));
    }
    with_win(hwnd, |w| w.recording = None);
    reload();
}

fn modifier_key(vk: u16) -> bool {
    matches!(vk, VK_SHIFT | VK_CONTROL | VK_MENU | VK_LSHIFT | VK_RSHIFT | VK_LCONTROL | VK_RCONTROL | VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN)
}

// ---------------------------------------------------------------- alerts

fn alert(hwnd: HWND, msg: &str) {
    unsafe { MessageBoxW(hwnd, wide(msg).as_ptr(), wide("VKeyEngine").as_ptr(), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND) };
}

fn confirm(hwnd: HWND, msg: &str) -> bool {
    let r = unsafe { MessageBoxW(hwnd, wide(msg).as_ptr(), wide("VKeyEngine").as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND) };
    r == IDYES
}

// ---------------------------------------------------------------- quick convert

/// Clipboard text as UTF-16 (without the trailing NUL) or bytes, for one format.
unsafe fn clipboard_get(format: u32) -> Option<Vec<u8>> {
    let h = GetClipboardData(format);
    if h.is_null() {
        return None;
    }
    let p = GlobalLock(h) as *const u8;
    if p.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(p, GlobalSize(h)).to_vec();
    GlobalUnlock(h);
    Some(bytes)
}

unsafe fn clipboard_set(format: u32, bytes: &[u8]) {
    let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
    if mem.is_null() {
        return;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), GlobalLock(mem) as *mut u8, bytes.len());
    GlobalUnlock(mem);
    SetClipboardData(format, mem);
}

/// "HTML Format" stores byte offsets in its header; convert the body and recompute them.
fn convert_html(html: &str, opt: &crate::convert::ConvertOptions) -> Option<String> {
    let start = html.find('<')?;
    let (header, body) = html.split_at(start);
    let mut out = format!("{header}{}", convert(body, opt));
    let pos = |s: &str, m: &str, after: bool| s.find(m).map(|p| if after { p + m.len() } else { p });
    let values = [
        ("StartHTML:", Some(start)),
        ("EndHTML:", Some(out.len())),
        ("StartFragment:", pos(&out, "<!--StartFragment-->", true)),
        ("EndFragment:", pos(&out, "<!--EndFragment-->", false)),
    ];
    for (key, value) in values {
        let (Some(k), Some(v)) = (out.find(key), value) else { continue };
        let from = k + key.len();
        let digits = out[from..].bytes().take_while(u8::is_ascii_digit).count();
        out.replace_range(from..from + digits, &format!("{v:0digits$}"));
    }
    Some(out)
}

/// Convert the clipboard (plain text and HTML) with the convert tool options.
fn quick_convert(always_alert: bool) {
    let (_, o) = read_settings();
    let html_format = unsafe { RegisterClipboardFormatW(wide("HTML Format").as_ptr()) };
    let (text, html) = unsafe {
        if OpenClipboard(main_hwnd()) == 0 {
            return;
        }
        let text = clipboard_get(CF_UNICODETEXT as u32).map(|b| {
            let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).take_while(|&u| u != 0).collect();
            convert(&String::from_utf16_lossy(&units), &o.convert)
        });
        let html = clipboard_get(html_format).and_then(|b| {
            let s = String::from_utf8_lossy(&b);
            convert_html(s.trim_end_matches('\0'), &o.convert)
        });
        if text.is_some() || html.is_some() {
            EmptyClipboard();
            if let Some(t) = &text {
                let bytes: Vec<u8> = t.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
                clipboard_set(CF_UNICODETEXT as u32, &bytes);
            }
            if let Some(h) = &html {
                clipboard_set(html_format, &[h.as_bytes(), &[0]].concat());
            }
        }
        CloseClipboard();
        (text, html)
    };
    if text.is_none() && html.is_none() {
        alert(null_mut(), "Không có dữ liệu trong clipboard!\nHãy sao chép một đoạn văn bản để chuyển mã.");
    } else if o.convert_alert || always_alert {
        alert(null_mut(), "Chuyển mã thành công!\nKết quả đã được lưu trong clipboard.");
    }
}

// ---------------------------------------------------------------- building windows

fn build(kind: Kind, page: usize) -> Page {
    let mut p = Page::new(580.0);
    match kind {
        Kind::Settings => {
            p.header("VKeyEngine", &format!("Bộ gõ tiếng Việt · phiên bản {}", env!("CARGO_PKG_VERSION")));
            let labels: &[&str] = &["Bộ gõ", "Gõ tắt", "Hệ thống", "Hoạt động"];
            p.items.push(Item::Segment { r: R { x: 580.0 - PAD - 340.0, y: 28.0, w: 340.0, h: 30.0 }, labels, tag: T_PAGE });
            match page {
                0 => {
                    let beep = toggle_index("vSwitchKeyStatus", hotkey::BEEP);
                    p.section("Bộ gõ");
                    p.card(vec![
                        (POPUPS[POP_LANG].label, Ctl::Popup(POP_LANG, 200.0)),
                        (POPUPS[POP_INPUT].label, Ctl::Popup(POP_INPUT, 200.0)),
                        (POPUPS[POP_CODE].label, Ctl::Popup(POP_CODE, 200.0)),
                        ("Phím chuyển Việt / Anh", Ctl::Hotkey(P_SWITCH_KEY, T_RECORD_SWITCH)),
                        (TOGGLES[beep].title, Ctl::Switch(beep)),
                    ]);
                    p.section("Chính tả");
                    p.toggle_card(P_SPELL);
                    p.section("Gõ nhanh");
                    p.toggle_card(P_QUICK);
                }
                1 => {
                    p.section("Gõ tắt");
                    p.toggle_card(P_MACRO);
                    p.note("Ví dụ: gõ \"vn\" rồi nhấn phím cách để được \"Việt Nam\".");
                    p.buttons(&[], &[("Bảng gõ tắt…", T_MACROS)], 0);
                }
                ACTIVITY_PAGE => {
                    p.section("CPU · 2 phút gần nhất");
                    let r = R { x: PAD, y: p.y, w: p.w - 2.0 * PAD, h: 96.0 };
                    p.items.push(Item::Card(r));
                    p.items.push(Item::Graph(R { x: r.x + INSET, y: r.y + 8.0, w: r.w - 2.0 * INSET, h: r.h - 16.0 }));
                    p.y += r.h;
                    p.section("Tài nguyên");
                    p.card((0..3).map(|n| (ACTIVITY_ROWS[n], Ctl::Stat(n))).collect());
                    p.section("Bộ gõ");
                    p.card((3..7).map(|n| (ACTIVITY_ROWS[n], Ctl::Stat(n))).collect());
                    p.note("Cập nhật mỗi 2 giây. Khi để không, VKeyEngine gần như không dùng CPU.");
                }
                _ => {
                    p.section("Khởi động");
                    p.toggle_card(P_STARTUP);
                    p.section("Tự động chuyển");
                    p.toggle_card(P_SMART);
                    p.section("Tương thích");
                    p.toggle_card(P_COMPAT);
                    p.buttons(&[("Khôi phục mặc định", T_DEFAULTS)], &[("Thoát VKeyEngine", T_QUIT)], 0);
                }
            }
        }
        Kind::Convert => {
            p.w = 520.0;
            p.header("Chuyển mã", "Chuyển văn bản trong clipboard giữa các bảng mã tiếng Việt.");
            p.section("Bảng mã");
            p.card(vec![
                (POPUPS[POP_FROM].label, Ctl::Popup(POP_FROM, 220.0)),
                (POPUPS[POP_TO].label, Ctl::Popup(POP_TO, 220.0)),
                (POPUPS[POP_CASE].label, Ctl::Popup(POP_CASE, 220.0)),
            ]);
            p.section("Tuỳ chọn");
            let mut rows: Vec<_> =
                (0..TOGGLES.len()).filter(|&i| TOGGLES[i].page == P_CONVERT).map(|i| (TOGGLES[i].title, Ctl::Switch(i))).collect();
            rows.push(("Phím tắt chuyển mã nhanh", Ctl::Hotkey(P_CONVERT_KEY, T_RECORD_CONVERT)));
            p.card(rows);
            p.note("Sao chép văn bản, bấm \"Chuyển mã\", rồi dán kết quả.");
            p.buttons(&[("⇅ Đảo bảng mã", T_CONVERT_SWAP)], &[("Đóng", T_CLOSE), ("Chuyển mã", T_CONVERT_RUN)], T_CONVERT_RUN);
        }
        Kind::Macros => {
            p.header("Gõ tắt", "Gõ từ viết tắt rồi nhấn phím cách, VKeyEngine sẽ thay bằng nội dung đầy đủ.");
            p.y += 14.0;
            let r = R { x: PAD, y: p.y, w: p.w - 2.0 * PAD, h: 250.0 };
            p.items.push(Item::Card(r));
            p.natives.push((Native::List, R { x: r.x + 1.0, y: r.y + 6.0, w: r.w - 2.0, h: r.h - 12.0 }));
            p.y += r.h;
            p.section("Thêm / sửa");
            let caps = toggle_index("vAutoCapsMacro", 0);
            p.card(vec![
                ("Từ gõ tắt", Ctl::Edit(ID_NAME, "ví dụ: vn", 340.0)),
                ("Nội dung", Ctl::Edit(ID_CONTENT, "ví dụ: Việt Nam", 340.0)),
                (TOGGLES[caps].title, Ctl::Switch(caps)),
            ]);
            p.buttons(
                &[("Nhập từ file…", T_MACRO_IMPORT), ("Xuất ra file…", T_MACRO_EXPORT)],
                &[("Xoá", T_MACRO_DEL), ("Thêm", T_MACRO_ADD)],
                T_MACRO_ADD,
            );
        }
        Kind::About => {
            p.w = 380.0;
            p.y = 24.0;
            p.items.push(Item::Icon(R { x: (p.w - 80.0) / 2.0, y: p.y, w: 80.0, h: 80.0 }));
            p.y += 92.0;
            let lines = [
                ("VKeyEngine".to_string(), 20.0, true, false, 28.0),
                (format!("Phiên bản {}", env!("CARGO_PKG_VERSION")), 12.0, false, true, 22.0),
                ("Bộ gõ tiếng Việt mã nguồn mở".to_string(), 13.0, false, false, 20.0),
                ("Dựa trên OpenKey của Mai Vũ Tuyên · GPL v3".to_string(), 12.0, false, true, 20.0),
            ];
            for (s, size, bold, secondary, h) in lines {
                let r = R { x: PAD, y: p.y, w: p.w - 2.0 * PAD, h };
                p.items.push(Item::Text { r, s, size, bold, secondary, center: true, toggle: None });
                p.y += h;
            }
        }
    }
    p.finish();
    p
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

fn activity_texts() -> [String; 7] {
    use crate::stats;
    let s = stats::snapshot();
    let status = if hook::FRONT_ELEVATED.load(Ordering::SeqCst) {
        "⚠ Ứng dụng quyền Admin đang mở".to_string()
    } else if read_settings().0.language == 1 {
        "Đang hoạt động · Tiếng Việt".to_string()
    } else {
        "Đang hoạt động · Tiếng Anh".to_string()
    };
    [
        stats::memory(s.memory),
        stats::percent(s.cpu),
        stats::duration(s.uptime),
        status,
        stats::thousands(s.keys),
        stats::latency(s.avg_key),
        stats::latency(s.max_key),
    ]
}

fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Settings => "VKeyEngine",
        Kind::Macros => "Gõ tắt",
        Kind::Convert => "Công cụ chuyển mã",
        Kind::About => "Giới thiệu VKeyEngine",
    }
}

const WINDOW_STYLE: WINDOW_STYLE = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;

fn show(kind: Kind) {
    if let Some(hwnd) = find_kind(kind) {
        if kind == Kind::Macros {
            reload_macro_rows(hwnd);
        }
        refresh();
        unsafe {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
        }
        return;
    }
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            wide(WIN_CLASS).as_ptr(),
            wide(title(kind)).as_ptr(),
            WINDOW_STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        )
    };
    if hwnd.is_null() {
        return;
    }
    WINS.with(|w| {
        w.borrow_mut().push(Win {
            hwnd,
            kind,
            page: 0,
            items: Vec::new(),
            natives: Vec::new(),
            width: 0.0,
            height: 0.0,
            hover: None,
            focus: None,
            recording: None,
        })
    });
    style_window(hwnd);
    load_page(hwnd, 0);
    center(hwnd);
    if kind == Kind::Macros {
        reload_macro_rows(hwnd);
    }
    refresh();
    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
    }
}

fn show_page(hwnd: HWND, page: usize) {
    load_page(hwnd, page);
    refresh();
}

/// (Re)build the window content: painted items, native controls and the window size.
fn load_page(hwnd: HWND, page: usize) {
    let Some(kind) = with_win(hwnd, |w| w.kind) else { return };
    let p = build(kind, page);
    let old: Vec<HWND> = with_win(hwnd, |w| w.natives.drain(..).map(|n| n.0).collect()).unwrap_or_default();
    for h in old {
        unsafe { DestroyWindow(h) };
    }
    let natives: Vec<(HWND, R)> = p.natives.iter().map(|(n, r)| (create_native(hwnd, n), *r)).collect();
    let (w, h) = (p.w, p.y);
    with_win(hwnd, |win| {
        win.page = page;
        win.items = p.items;
        win.natives = natives;
        win.width = w;
        win.height = h;
        win.hover = None;
        win.focus = None;
        win.recording = None;
    });
    resize(hwnd, w, h);
}

/// Client size in DIPs -> window size at the window's DPI; native controls follow.
fn resize(hwnd: HWND, w: f32, h: f32) {
    let s = scale(hwnd);
    let mut rc = RECT { left: 0, top: 0, right: (w * s).round() as i32, bottom: (h * s).round() as i32 };
    unsafe {
        AdjustWindowRectExForDpi(&mut rc, WINDOW_STYLE, 0, 0, GetDpiForWindow(hwnd));
        SetWindowPos(hwnd, null_mut(), 0, 0, rc.right - rc.left, rc.bottom - rc.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    layout_natives(hwnd);
}

fn layout_natives(hwnd: HWND) {
    let s = scale(hwnd);
    let natives: Vec<(HWND, R)> = with_win(hwnd, |w| w.natives.clone()).unwrap_or_default();
    for (h, r) in natives {
        let id = unsafe { GetDlgCtrlID(h) } as isize;
        // a combo box's height includes its drop-down list
        let extra = if (T_POPUP..T_RECORD_SWITCH).contains(&id) { 240.0 } else { 0.0 };
        unsafe {
            SetWindowPos(h, null_mut(), (r.x * s) as i32, (r.y * s) as i32, (r.w * s) as i32, ((r.h + extra) * s) as i32, SWP_NOZORDER | SWP_NOACTIVATE);
            SendMessageW(h, WM_SETFONT, font(13.0, false, s) as usize, 1);
        }
        if id == ID_LIST {
            let cw = r.w * s;
            for (col, width) in [(0, 150.0 * s), (1, cw - 170.0 * s)] {
                unsafe { SendMessageW(h, LVM_SETCOLUMNWIDTH, col, width as isize) };
            }
        }
    }
}

fn center(hwnd: HWND) {
    unsafe {
        let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut rc);
        let mut work = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut RECT as _, 0);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let x = work.left + ((work.right - work.left) - w) / 2;
        let y = work.top + ((work.bottom - work.top) - h) / 2;
        SetWindowPos(hwnd, null_mut(), x, y.max(work.top), 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Title bar matching the window (dark mode, and on Windows 11 the exact colors).
fn style_window(hwnd: HWND) {
    let t = theme();
    unsafe {
        let dark: BOOL = t.dark as BOOL;
        DwmSetWindowAttribute(hwnd, 20, &dark as *const BOOL as _, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
        for (attr, c) in [(34, t.bg), (35, t.bg), (36, t.text)] {
            // DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR
            let c = colorref(c);
            DwmSetWindowAttribute(hwnd, attr, &c as *const u32 as _, 4);
        }
    }
}

fn style_native(h: HWND) {
    let t = theme();
    let id = unsafe { GetDlgCtrlID(h) } as isize;
    unsafe {
        if id == ID_LIST {
            SetWindowTheme(h, wide(if t.dark { "DarkMode_Explorer" } else { "Explorer" }).as_ptr(), null());
            let header = SendMessageW(h, LVM_GETHEADER, 0, 0) as HWND;
            SetWindowTheme(header, wide(if t.dark { "DarkMode_ItemsView" } else { "ItemsView" }).as_ptr(), null());
            for m in [LVM_SETBKCOLOR, LVM_SETTEXTBKCOLOR] {
                SendMessageW(h, m, 0, colorref(t.card) as isize);
            }
            SendMessageW(h, LVM_SETTEXTCOLOR, 0, colorref(t.text) as isize);
        } else {
            let dark = wide("DarkMode_CFD");
            SetWindowTheme(h, if t.dark { dark.as_ptr() } else { null() }, null());
        }
    }
}

fn create_native(parent: HWND, n: &Native) -> HWND {
    let instance = unsafe { GetModuleHandleW(null()) };
    let make = |class: &str, style: u32, ex: u32, id: isize| unsafe {
        CreateWindowExW(ex, wide(class).as_ptr(), null(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | style, 0, 0, 10, 10, parent, id as HMENU, instance, null())
    };
    let h = match n {
        Native::Combo(i) => {
            let h = make("COMBOBOX", (CBS_DROPDOWNLIST | WS_VSCROLL as i32) as u32, 0, T_POPUP + *i as isize);
            for item in POPUPS[*i].items {
                unsafe { SendMessageW(h, CB_ADDSTRING, 0, wide(item).as_ptr() as isize) };
            }
            h
        }
        Native::Edit(id, placeholder) => {
            let h = make("EDIT", ES_AUTOHSCROLL as u32, 0, *id);
            unsafe { SendMessageW(h, EM_SETCUEBANNER, 1, wide(placeholder).as_ptr() as isize) };
            h
        }
        Native::List => {
            let style = LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER;
            let h = make("SysListView32", style, 0, ID_LIST);
            unsafe {
                SendMessageW(h, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize);
                for (i, title) in ["Từ gõ tắt", "Nội dung"].iter().enumerate() {
                    let mut text = wide(title);
                    let col = LVCOLUMNW { mask: LVCF_TEXT | LVCF_WIDTH, cx: 150, pszText: text.as_mut_ptr(), ..std::mem::zeroed() };
                    SendMessageW(h, LVM_INSERTCOLUMNW, i, &col as *const LVCOLUMNW as isize);
                }
            }
            h
        }
    };
    style_native(h);
    h
}

// ---------------------------------------------------------------- painting

struct Gdip(*mut GpGraphics);

impl Gdip {
    fn rounded(&self, r: R, radius: f32, s: f32, fill: Option<u32>, stroke: Option<u32>) {
        let (x, y, w, h) = (r.x * s, r.y * s, r.w * s, r.h * s);
        let d = (radius * 2.0 * s).min(h).min(w);
        unsafe {
            let mut path = null_mut();
            GdipCreatePath(FillModeAlternate, &mut path);
            GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(path);
            if let Some(c) = fill {
                let mut brush = null_mut();
                GdipCreateSolidFill(c, &mut brush);
                GdipFillPath(self.0, brush as *mut GpBrush, path);
                GdipDeleteBrush(brush as *mut GpBrush);
            }
            if let Some(c) = stroke {
                let mut pen = null_mut();
                GdipCreatePen1(c, s.max(1.0), UnitPixel, &mut pen);
                GdipDrawPath(self.0, pen, path);
                GdipDeletePen(pen);
            }
            GdipDeletePath(path);
        }
    }

    fn circle(&self, cx: f32, cy: f32, radius: f32, s: f32, c: u32) {
        unsafe {
            let mut brush = null_mut();
            GdipCreateSolidFill(c, &mut brush);
            let r = radius * s;
            GdipFillEllipse(self.0, brush as *mut GpBrush, cx * s - r, cy * s - r, 2.0 * r, 2.0 * r);
            GdipDeleteBrush(brush as *mut GpBrush);
        }
    }
}

fn button_label(hwnd: HWND, tag: isize, label: &str) -> String {
    if tag != T_MACRO_ADD {
        return label.into();
    }
    let name = get_text(hwnd, ID_NAME);
    let exists = !name.is_empty() && with_app(|a| a.engine.macros.has(&name, a.engine.s.code_table)).unwrap_or(false);
    if exists { "Sửa".into() } else { label.into() }
}

fn paint(hwnd: HWND) {
    let t = theme();
    let s = scale(hwnd);
    // the items are taken out while painting, so nothing below borrows the window state
    let Some((items, page, hover, focus, recording)) =
        with_win(hwnd, |w| (std::mem::take(&mut w.items), w.page, w.hover, w.focus, w.recording))
    else {
        return;
    };
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut rc);
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, rc.right.max(1), rc.bottom.max(1));
        let old_bmp = SelectObject(mem, bmp);
        let bg = CreateSolidBrush(colorref(t.bg));
        FillRect(mem, &rc, bg);
        DeleteObject(bg);

        // shapes
        let mut g = null_mut();
        GdipCreateFromHDC(mem, &mut g);
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        let g = Gdip(g);
        for (i, item) in items.iter().enumerate() {
            let hot = hover == Some(i);
            match item {
                Item::Card(r) => g.rounded(*r, 10.0, s, Some(argb(t.card, 255)), Some(argb(t.border, 255))),
                Item::Line(r) => g.rounded(*r, 0.0, s, Some(argb(t.border, 255)), None),
                Item::Field(r) => g.rounded(*r, 6.0, s, Some(argb(t.card, 255)), Some(argb(t.border, 255))),
                Item::Switch { r, toggle } => {
                    let tg = &TOGGLES[*toggle];
                    let (on, enabled) = (toggle_on(tg), toggle_enabled(tg));
                    let alpha = if enabled { 255 } else { 90 };
                    let track = if on { t.accent } else { t.track_off };
                    g.rounded(*r, r.h / 2.0, s, Some(argb(track, alpha)), None);
                    let knob = if on { t.on_accent } else { t.knob_off };
                    let cx = if on { r.x + r.w - r.h / 2.0 } else { r.x + r.h / 2.0 };
                    g.circle(cx, r.y + r.h / 2.0, r.h / 2.0 - 3.0, s, argb(knob, alpha));
                }
                Item::Mod { r, toggle } => {
                    let on = toggle_on(&TOGGLES[*toggle]);
                    let fill = if on { t.accent } else if hot { t.hover } else { t.button };
                    g.rounded(*r, 6.0, s, Some(argb(fill, 255)), None);
                }
                Item::Button { r, primary, .. } => {
                    let fill = if *primary { t.accent } else if hot { t.hover } else { t.button };
                    g.rounded(*r, 8.0, s, Some(argb(fill, if *primary && hot { 220 } else { 255 })), None);
                }
                Item::Record { r, tag } => {
                    let fill = if recording == Some(*tag) || hot { t.hover } else { t.button };
                    g.rounded(*r, 6.0, s, Some(argb(fill, 255)), None);
                }
                Item::Graph(r) => {
                    let values = crate::stats::snapshot().cpu_history;
                    if values.len() >= 2 {
                        let max = values.iter().fold(5.0f32, |m, &v| m.max(v)); // at least 5 % full scale
                        let step = r.w / (crate::stats::HISTORY - 1) as f32;
                        let x0 = r.x + r.w - step * (values.len() - 1) as f32; // newest on the right
                        let bottom = r.y + r.h;
                        let mut pts: Vec<PointF> = values
                            .iter()
                            .enumerate()
                            .map(|(i, &v)| PointF { X: (x0 + step * i as f32) * s, Y: (bottom - 4.0 - (r.h - 8.0) * v / max) * s })
                            .collect();
                        let line = pts.clone();
                        pts.push(PointF { X: (r.x + r.w) * s, Y: bottom * s });
                        pts.push(PointF { X: x0 * s, Y: bottom * s });
                        let mut brush = null_mut();
                        GdipCreateSolidFill(argb(t.text, 26), &mut brush);
                        GdipFillPolygon(g.0, brush as *mut GpBrush, pts.as_ptr(), pts.len() as i32, FillModeAlternate);
                        GdipDeleteBrush(brush as *mut GpBrush);
                        let mut pen = null_mut();
                        GdipCreatePen1(argb(t.text, 255), 1.5 * s, UnitPixel, &mut pen);
                        GdipDrawLines(g.0, pen, line.as_ptr(), line.len() as i32);
                        GdipDeletePen(pen);
                    }
                }
                Item::Segment { r, labels, .. } => {
                    g.rounded(*r, 8.0, s, Some(argb(t.button, 255)), None);
                    let sw = r.w / labels.len() as f32;
                    let sel = R { x: r.x + sw * page as f32 + 2.0, y: r.y + 2.0, w: sw - 4.0, h: r.h - 4.0 };
                    g.rounded(sel, 6.0, s, Some(argb(t.accent, 255)), None);
                }
                _ => {}
            }
            if focus == Some(i) {
                let r = item.rect();
                let ring = R { x: r.x - 3.0, y: r.y - 3.0, w: r.w + 6.0, h: r.h + 6.0 };
                g.rounded(ring, 8.0, s, None, Some(argb(t.accent, 160)));
            }
        }
        GdipDeleteGraphics(g.0);

        // text and icons
        SetBkMode(mem, TRANSPARENT as i32);
        let draw = |r: R, text: &str, px: f32, bold: bool, color: u32, align: DRAW_TEXT_FORMAT| {
            let mut rect = RECT {
                left: (r.x * s) as i32,
                top: (r.y * s) as i32,
                right: ((r.x + r.w) * s) as i32,
                bottom: ((r.y + r.h) * s) as i32,
            };
            SelectObject(mem, font(px, bold, s));
            SetTextColor(mem, colorref(color));
            let mut w: Vec<u16> = text.encode_utf16().collect();
            DrawTextW(mem, w.as_mut_ptr(), w.len() as i32, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX | align);
        };
        let mut activity: Option<[String; 7]> = None; // computed once per paint
        for item in &items {
            match item {
                Item::Text { r, s: text, size, bold, secondary, center, toggle } => {
                    let mut color = if *secondary { t.secondary } else { t.text };
                    if toggle.is_some_and(|i| !toggle_enabled(&TOGGLES[i])) {
                        color = blend(t.text, t.card, 0.6);
                    }
                    draw(*r, text, *size, *bold, color, if *center { DT_CENTER } else { DT_LEFT })
                }
                Item::Mod { r, toggle } => {
                    let tg = &TOGGLES[*toggle];
                    let color = if toggle_on(tg) { t.on_accent } else { t.text };
                    draw(*r, tg.title, 12.0, false, color, DT_CENTER);
                }
                Item::Button { r, label, tag, primary } => {
                    let color = if *primary { t.on_accent } else { t.text };
                    draw(*r, &button_label(hwnd, *tag, label), 13.0, *primary, color, DT_CENTER);
                }
                Item::Record { r, tag } => {
                    let key = if *tag == T_RECORD_SWITCH { "vSwitchKeyStatus" } else { "convertToolHotKey" };
                    let default = if *tag == T_RECORD_SWITCH { DEFAULT_SWITCH_KEY } else { EMPTY_HOTKEY };
                    let label = if recording == Some(*tag) { "Nhấn một phím…".to_string() } else { hotkey::key_title(int_pref(key, default)) };
                    draw(*r, &label, 12.0, false, t.text, DT_CENTER);
                }
                Item::Segment { r, labels, .. } => {
                    let sw = r.w / labels.len() as f32;
                    for (n, label) in labels.iter().enumerate() {
                        let seg = R { x: r.x + sw * n as f32, y: r.y, w: sw, h: r.h };
                        let color = if n == page { t.on_accent } else { t.text };
                        draw(seg, label, 13.0, n == page, color, DT_CENTER);
                    }
                }
                Item::Stat { r, n } => {
                    let texts = activity.get_or_insert_with(activity_texts);
                    draw(*r, &texts[*n], 13.0, false, t.secondary, DT_RIGHT);
                }
                Item::Icon(r) => {
                    let size = (r.w * s) as i32;
                    let icon = app_icon(size);
                    DrawIconEx(mem, (r.x * s) as i32, (r.y * s) as i32, icon, size, size, 0, null_mut(), DI_NORMAL);
                    DestroyIcon(icon);
                }
                _ => {}
            }
        }
        BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old_bmp);
        DeleteObject(bmp);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
    with_win(hwnd, |w| w.items = items);
}

// ---------------------------------------------------------------- input

fn dip_point(hwnd: HWND, lparam: LPARAM) -> (f32, f32) {
    let s = scale(hwnd);
    let x = (lparam & 0xFFFF) as i16 as f32 / s;
    let y = ((lparam >> 16) & 0xFFFF) as i16 as f32 / s;
    (x, y)
}

fn hit(hwnd: HWND, x: f32, y: f32) -> Option<usize> {
    with_win(hwnd, |w| w.items.iter().position(|i| i.interactive() && i.rect().contains(x, y))).flatten()
}

/// Tag an item triggers when clicked at (x, y).
fn item_tag(hwnd: HWND, i: usize, x: Option<f32>) -> Option<isize> {
    with_win(hwnd, |w| match &w.items[i] {
        Item::Switch { toggle, .. } | Item::Mod { toggle, .. } => Some(T_TOGGLE + *toggle as isize),
        Item::Button { tag, .. } | Item::Record { tag, .. } => Some(*tag),
        Item::Segment { r, labels, tag } => {
            let n = match x {
                Some(x) => (((x - r.x) / (r.w / labels.len() as f32)) as usize).min(labels.len() - 1),
                None => (w.page + 1) % labels.len(), // keyboard: next page
            };
            Some(tag + n as isize)
        }
        _ => None,
    })
    .flatten()
}

fn get_text(hwnd: HWND, id: isize) -> String {
    unsafe {
        let h = GetDlgItem(hwnd, id as i32);
        let len = GetWindowTextLengthW(h);
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

fn set_text(hwnd: HWND, id: isize, s: &str) {
    unsafe { SetWindowTextW(GetDlgItem(hwnd, id as i32), wide(s).as_ptr()) };
}

unsafe extern "system" fn win_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let (x, y) = dip_point(hwnd, lparam);
            let h = hit(hwnd, x, y);
            if with_win(hwnd, |w| std::mem::replace(&mut w.hover, h) != h) == Some(true) {
                let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut tme);
                SetCursor(LoadCursorW(null_mut(), if h.is_some() { IDC_HAND } else { IDC_ARROW }));
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE => {
            with_win(hwnd, |w| w.hover = None);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_SETCURSOR if (lparam & 0xFFFF) as u32 == HTCLIENT && with_win(hwnd, |w| w.hover.is_some()) == Some(true) => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            let (x, y) = dip_point(hwnd, lparam);
            if let Some(tag) = hit(hwnd, x, y).and_then(|i| item_tag(hwnd, i, Some(x))) {
                action(hwnd, tag);
            }
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam as u16;
            if let Some(Some(tag)) = with_win(hwnd, |w| w.recording) {
                record_key(hwnd, tag, vk);
                return 0;
            }
            match vk {
                VK_TAB => {
                    let back = GetKeyState(VK_SHIFT as i32) < 0;
                    with_win(hwnd, |w| {
                        let idx: Vec<usize> = (0..w.items.len()).filter(|&i| w.items[i].interactive()).collect();
                        if idx.is_empty() {
                            return;
                        }
                        let pos = w.focus.and_then(|f| idx.iter().position(|&i| i == f));
                        let next = match (pos, back) {
                            (None, false) => 0,
                            (None, true) => idx.len() - 1,
                            (Some(p), false) => (p + 1) % idx.len(),
                            (Some(p), true) => (p + idx.len() - 1) % idx.len(),
                        };
                        w.focus = Some(idx[next]);
                    });
                    InvalidateRect(hwnd, null(), 0);
                }
                VK_SPACE | VK_RETURN => {
                    if let Some(Some(i)) = with_win(hwnd, |w| w.focus) {
                        if let Some(tag) = item_tag(hwnd, i, None) {
                            action(hwnd, tag);
                        }
                    }
                }
                VK_ESCAPE => {
                    DestroyWindow(hwnd);
                }
                _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as isize;
            let code = ((wparam >> 16) & 0xFFFF) as u32;
            if code == CBN_SELCHANGE && (T_POPUP..T_RECORD_SWITCH).contains(&id) {
                let sel = SendMessageW(lparam as HWND, CB_GETCURSEL, 0, 0);
                let i = (id - T_POPUP) as usize;
                if i == POP_LANG {
                    action(hwnd, if sel == 1 { T_VIETNAMESE } else { T_ENGLISH });
                } else if i == POP_CODE {
                    action(hwnd, T_CODE + sel);
                } else {
                    set_popup_value(i, sel as i64);
                    reload();
                }
            } else if code == EN_CHANGE && id == ID_NAME {
                InvalidateRect(hwnd, null(), 0); // "Thêm" / "Sửa"
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lparam as *const NMHDR);
            if hdr.idFrom as isize == ID_LIST && hdr.code == LVN_ITEMCHANGED {
                let nm = &*(lparam as *const NMLISTVIEW);
                if nm.uNewState & LVIS_SELECTED != 0 {
                    let rows = with_app(|a| a.engine.macros.all()).unwrap_or_default();
                    if let Some((text, content)) = rows.get(nm.iItem as usize) {
                        set_text(hwnd, ID_NAME, text);
                        set_text(hwnd, ID_CONTENT, content);
                    }
                }
            }
            0
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSTATIC => {
            let t = theme();
            SetTextColor(wparam as HDC, colorref(t.text));
            SetBkColor(wparam as HDC, colorref(t.card));
            card_brush() as LRESULT
        }
        WM_DPICHANGED => {
            let r = &*(lparam as *const RECT);
            SetWindowPos(hwnd, null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            if let Some((w, h)) = with_win(hwnd, |w| (w.width, w.height)) {
                resize(hwnd, w, h);
            }
            0
        }
        WM_DESTROY => {
            WINS.with(|w| w.borrow_mut().retain(|x| x.hwnd != hwnd));
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ---------------------------------------------------------------- macros

fn reload_macro_rows(hwnd: HWND) {
    let rows = with_app(|a| a.engine.macros.all()).unwrap_or_default();
    unsafe {
        let list = GetDlgItem(hwnd, ID_LIST as i32);
        SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
        for (i, (text, content)) in rows.iter().enumerate() {
            let mut t = wide(text);
            let mut item = LVITEMW { mask: LVIF_TEXT, iItem: i as i32, pszText: t.as_mut_ptr(), ..std::mem::zeroed() };
            SendMessageW(list, LVM_INSERTITEMW, 0, &item as *const LVITEMW as isize);
            let mut c = wide(content);
            item.iSubItem = 1;
            item.pszText = c.as_mut_ptr();
            SendMessageW(list, LVM_SETITEMTEXTW, i, &item as *const LVITEMW as isize);
        }
        InvalidateRect(hwnd, null(), 0);
    }
}

fn clear_macro_fields(hwnd: HWND) {
    set_text(hwnd, ID_NAME, "");
    set_text(hwnd, ID_CONTENT, "");
    unsafe { SetFocus(GetDlgItem(hwnd, ID_NAME as i32)) };
}

fn macro_add(hwnd: HWND) {
    let (name, content) = (get_text(hwnd, ID_NAME), get_text(hwnd, ID_CONTENT));
    if name.is_empty() || content.is_empty() {
        alert(hwnd, "Bạn hãy nhập từ gõ tắt và nội dung!");
        return;
    }
    with_app(|a| {
        let t = a.engine.s.code_table;
        a.engine.macros.add(&name, &content, t);
        a.save_macros();
    });
    clear_macro_fields(hwnd);
    reload_macro_rows(hwnd);
}

fn macro_delete(hwnd: HWND) {
    let name = get_text(hwnd, ID_NAME);
    if name.is_empty() {
        alert(hwnd, "Bạn hãy chọn từ cần xoá!");
        return;
    }
    with_app(|a| {
        let t = a.engine.s.code_table;
        if a.engine.macros.delete(&name, t) {
            a.save_macros();
        }
    });
    clear_macro_fields(hwnd);
    reload_macro_rows(hwnd);
}

/// Open / save dialog for a .txt file; returns the chosen path.
fn file_dialog(hwnd: HWND, save: bool) -> Option<String> {
    let mut file = [0u16; 1024];
    if save {
        copy_wide(&mut file, "VKeyEngineMacro.txt");
    }
    let filter: Vec<u16> = "Văn bản (*.txt)\0*.txt\0Tất cả\0*.*\0\0".encode_utf16().collect();
    let ext = wide("txt");
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: file.as_mut_ptr(),
        nMaxFile: file.len() as u32,
        lpstrDefExt: ext.as_ptr(),
        Flags: if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST } | OFN_PATHMUSTEXIST,
        ..unsafe { std::mem::zeroed() }
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } } != 0;
    let len = file.iter().position(|&c| c == 0).unwrap_or(0);
    ok.then(|| String::from_utf16_lossy(&file[..len]))
}

fn macro_import(hwnd: HWND) {
    let Some(path) = file_dialog(hwnd, false) else { return };
    let text = match std::fs::read(&path) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return alert(hwnd, &format!("Không đọc được file\n{e}")),
    };
    let keep = confirm(hwnd, "Bạn có muốn giữ lại các dữ liệu gõ tắt hiện tại không?");
    with_app(|a| {
        let t = a.engine.s.code_table;
        a.engine.macros.import_text(&text, keep, t);
        a.save_macros();
    });
    reload_macro_rows(hwnd);
}

fn macro_export(hwnd: HWND) {
    let Some(path) = file_dialog(hwnd, true) else { return };
    let text = with_app(|a| a.engine.macros.export_text()).unwrap_or_default();
    if let Err(e) = std::fs::write(&path, text) {
        alert(hwnd, &format!("Không ghi được file\n{e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_offsets_follow_the_converted_body() {
        let body = "<html><body><!--StartFragment-->Hà Nội<!--EndFragment--></body></html>";
        let header = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
        let opt = crate::convert::ConvertOptions { to_code: 1, ..Default::default() }; // Unicode -> TCVN3
        let out = convert_html(&format!("{header}{body}"), &opt).unwrap();
        let num = |key: &str| {
            let from = out.find(key).unwrap() + key.len();
            out[from..from + 10].parse::<usize>().unwrap()
        };
        assert_eq!(num("StartHTML:"), header.len());
        assert_eq!(num("EndHTML:"), out.len());
        assert!(out[num("StartFragment:")..].starts_with('H'));
        assert!(out[num("EndFragment:")..].starts_with("<!--EndFragment-->"));
    }
}
