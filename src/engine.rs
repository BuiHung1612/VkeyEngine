//! Vietnamese typing engine. Port of engine/Engine.cpp.
//!
//! The algorithm is kept 1:1 with the C++ engine so typing behaviour does not change,
//! except for the bugs fixed below (each one has a regression test in tests/engine.rs):
//! - macro key buffer got out of sync whenever backspace count != new char count
//!   (standalone `w` -> `ư`, quick telex, grammar re-check, restore): macros starting
//!   with `ư` never matched (#313) and macros "sometimes" did not fire (#284).
//! - a NUL character was sent when restoring a tone/horn on a vowel that also had a mark
//!   (`oos` + `o`), or when a standalone ư/ơ lost its horn (`[as`, `wx`).
//! - out-of-bounds reads/writes of the 32-slot buffers (UB in C++) are now guarded.
//!
//! Verified against the C++ engine with a differential test over 440k random key
//! sequences (all input types / code tables): identical output except the NUL cases above.

use crate::macros::Macros;
use crate::tables::*;

pub const MAX_BUFF: usize = 32;
const MAXB: i32 = MAX_BUFF as i32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Code {
    #[default]
    DoNothing = 0,
    WillProcess,
    BreakWord,
    Restore,
    ReplaceMacro,
    RestoreAndStartNewSession,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyEvent {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyState {
    KeyDown,
    KeyUp,
    MouseDown,
    MouseUp,
}

pub const TELEX: u8 = 0;
pub const VNI: u8 = 1;
pub const SIMPLE_TELEX_1: u8 = 2;
pub const SIMPLE_TELEX_2: u8 = 3;

/// Result of a key event, read by the platform layer.
#[derive(Default)]
pub struct HookState {
    pub code: Code,
    pub backspace_count: i32,
    pub new_char_count: i32,
    /// 1: word break, 2: delete key, 3: normal key, 4: should not send empty character
    pub ext_code: u8,
    /// New characters, in reverse order (index 0 = last character).
    pub char_data: [u32; MAX_BUFF],
    pub macro_key: Vec<u32>,
    pub macro_data: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    /// 0: English, 1: Vietnamese
    pub language: u8,
    /// TELEX, VNI, SIMPLE_TELEX_1, SIMPLE_TELEX_2
    pub input_type: u8,
    pub free_mark: bool,
    /// 0: Unicode, 1: TCVN3, 2: VNI Windows, 3: Unicode Compound, 4: CP 1258
    pub code_table: u8,
    pub check_spelling: bool,
    pub modern_orthography: bool,
    pub quick_telex: bool,
    pub restore_if_wrong_spelling: bool,
    /// like `restore_if_wrong_spelling`, but only when the keys spell an English word
    pub restore_english: bool,
    pub use_macro: bool,
    pub use_macro_in_english: bool,
    pub auto_caps_macro: bool,
    pub upper_case_first_char: bool,
    pub allow_consonant_zfwj: bool,
    pub quick_start_consonant: bool,
    pub quick_end_consonant: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: 1,
            input_type: TELEX,
            free_mark: false,
            code_table: 0,
            check_spelling: true,
            modern_orthography: true,
            quick_telex: false,
            restore_if_wrong_spelling: false,
            restore_english: false,
            use_macro: true,
            use_macro_in_english: true,
            auto_caps_macro: false,
            upper_case_first_char: false,
            allow_consonant_zfwj: false,
            quick_start_consonant: false,
            quick_end_consonant: false,
        }
    }
}

const CHAR_KEY_CODE: &[u16] = &[
    KEY_BACKQUOTE, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0, KEY_MINUS, KEY_EQUALS,
    KEY_LEFT_BRACKET, KEY_RIGHT_BRACKET, KEY_BACK_SLASH, KEY_SEMICOLON, KEY_QUOTE, KEY_COMMA, KEY_DOT, KEY_SLASH,
];

const BREAK_CODE: &[u16] = &[
    KEY_ESC, KEY_TAB, KEY_ENTER, KEY_RETURN, KEY_LEFT, KEY_RIGHT, KEY_DOWN, KEY_UP, KEY_COMMA, KEY_DOT, KEY_SLASH,
    KEY_SEMICOLON, KEY_QUOTE, KEY_BACK_SLASH, KEY_MINUS, KEY_EQUALS, KEY_BACKQUOTE,
];

const MACRO_BREAK_CODE: &[u16] = &[
    KEY_RETURN, KEY_COMMA, KEY_DOT, KEY_SLASH, KEY_SEMICOLON, KEY_QUOTE, KEY_BACK_SLASH, KEY_MINUS, KEY_EQUALS,
];

const PROCESSING_CHAR: [[u16; 11]; 4] = [
    [KEY_S, KEY_F, KEY_R, KEY_X, KEY_J, KEY_A, KEY_O, KEY_E, KEY_W, KEY_D, KEY_Z],
    [KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0],
    [KEY_S, KEY_F, KEY_R, KEY_X, KEY_J, KEY_A, KEY_O, KEY_E, KEY_W, KEY_D, KEY_Z],
    [KEY_S, KEY_F, KEY_R, KEY_X, KEY_J, KEY_A, KEY_O, KEY_E, KEY_W, KEY_D, KEY_Z],
];

fn is_word_break(event: KeyEvent, data: u16) -> bool {
    event == KeyEvent::Mouse || BREAK_CODE.contains(&data)
}

/// English words that Telex typing changes, sorted: the most frequent 50 000 English words
/// of FrequencyWords (github.com/hermitdave/FrequencyWords, content CC BY-SA 4.0), kept
/// when typing them with Telex does not give them back unchanged.
fn english_words() -> &'static [&'static str] {
    static WORDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| include_str!("english.txt").lines().collect())
}

/// Convert engine character data to the output character code of `code_table`.
pub fn char_code(data: u32, code_table: u8) -> u32 {
    let caps_elem: usize = if data & CAPS_MASK != 0 { 0 } else { 1 };
    let mut key = data & CHAR_MASK;
    if data & MARK_MASK != 0 {
        let mark_elem: usize = match data & MARK_MASK {
            MARK1_MASK => 0,
            MARK2_MASK => 2,
            MARK3_MASK => 4,
            MARK4_MASK => 6,
            MARK5_MASK => 8,
            _ => return data,
        };
        let mut mark_elem = mark_elem + caps_elem;
        if matches!(key as u16, KEY_A | KEY_O | KEY_U | KEY_E) && data & (TONE_MASK | TONEW_MASK) == 0 {
            mark_elem += 4;
        }
        if data & TONE_MASK != 0 {
            key |= TONE_MASK;
        } else if data & TONEW_MASK != 0 {
            key |= TONEW_MASK;
        }
        match code_entry(code_table as usize, key).and_then(|v| v.get(mark_elem)) {
            Some(&c) => c as u32 | CHAR_CODE_MASK,
            None => data,
        }
    } else {
        let Some(v) = code_entry(code_table as usize, key) else { return data };
        let idx = if data & TONE_MASK != 0 {
            caps_elem
        } else if data & TONEW_MASK != 0 {
            caps_elem + 2
        } else {
            return data;
        };
        v.get(idx).map_or(data, |&c| c as u32 | CHAR_CODE_MASK)
    }
}

/// Append the UTF-16 units that represent one output code. Returns the number of units,
/// which the platform layer needs to know how many backspaces delete this character.
pub fn encode_output(code: u32, code_table: u8, out: &mut Vec<u16>) -> u8 {
    if code & PURE_CHARACTER_MASK != 0 {
        let c = char::from_u32(code & !PURE_CHARACTER_MASK).unwrap_or('\u{FFFD}');
        let mut buf = [0u16; 2];
        out.extend_from_slice(c.encode_utf16(&mut buf));
        return 1;
    }
    if code & CHAR_CODE_MASK == 0 {
        out.push(key_to_char(code));
        return 1;
    }
    let c = code as u16;
    match code_table {
        0 => {
            out.push(c);
            1
        }
        3 => {
            let hi = c >> 13;
            out.push(c & 0x1FFF);
            if hi > 0 {
                out.push(UNICODE_COMPOUND_MARK[(hi - 1) as usize]);
                2
            } else {
                1
            }
        }
        _ => {
            let hi = c >> 8;
            out.push(c & 0xFF);
            if hi > 32 {
                out.push(hi);
                2
            } else {
                1
            }
        }
    }
}

/// Tables whose characters can take 2 UTF-16 units (needs per-char length tracking).
pub fn is_double_code(code_table: u8) -> bool {
    matches!(code_table, 2..=4)
}

pub struct Engine {
    pub s: Settings,
    pub hook: HookState,
    pub macros: Macros,
    /// name of the last macro replaced (for usage statistics)
    pub last_macro: Option<String>,

    typing_word: [u32; MAX_BUFF],
    index: i32,
    long_word_helper: Vec<u32>,
    typing_states: Vec<Vec<u32>>,
    key_states: [u32; MAX_BUFF],
    /// every key of the current word, also the repeated mark key that `key_states` drops
    /// ("boss": b o s s, while key_states keeps b o s like the C++ engine)
    raw_keys: Vec<u32>,
    state_index: i32,

    temp_disable_key: bool,
    is_correct: bool,
    is_changed: bool,
    vowel_count: i32,
    vsi: i32,
    vei: i32,
    vwsm: i32,
    is_restored_w: bool,
    is_checked_grammar: bool,
    is_caps: bool,
    space_count: i32,
    has_handled_macro: bool,
    upper_case_status: u8,
    special_char: Vec<u32>,
    /// Runtime spell checking flag (may be temporarily toggled with a hot key).
    spelling_on: bool,
    has_handle_quick_consonant: bool,
    will_temp_off_engine: bool,
    spelling_ok: bool,
    spelling_vowel_ok: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new(Settings::default())
    }
}

impl Engine {
    pub fn new(s: Settings) -> Self {
        let spelling_on = s.check_spelling;
        Engine {
            s,
            hook: HookState::default(),
            macros: Macros::default(),
            last_macro: None,
            typing_word: [0; MAX_BUFF],
            index: 0,
            long_word_helper: Vec::new(),
            typing_states: Vec::new(),
            key_states: [0; MAX_BUFF],
            raw_keys: Vec::new(),
            state_index: 0,
            temp_disable_key: false,
            is_correct: false,
            is_changed: false,
            vowel_count: 0,
            vsi: 0,
            vei: 0,
            vwsm: 0,
            is_restored_w: false,
            is_checked_grammar: false,
            is_caps: false,
            space_count: 0,
            has_handled_macro: false,
            upper_case_status: 0,
            special_char: Vec::new(),
            spelling_on,
            has_handle_quick_consonant: false,
            will_temp_off_engine: false,
            spelling_ok: false,
            spelling_vowel_ok: false,
        }
    }

    // ---------- buffer helpers (bounds-checked; the C++ code read garbage / UB here) ----------

    fn tw(&self, i: i32) -> u32 {
        if (0..MAXB).contains(&i) {
            self.typing_word[i as usize]
        } else {
            0
        }
    }
    fn chr(&self, i: i32) -> u16 {
        self.tw(i) as u16
    }
    /// Data without caps / mark bits, for "is this exactly X" comparisons.
    fn bare(&self, i: i32) -> u32 {
        self.tw(i) & !(CAPS_MASK | MARK_MASK)
    }
    fn set_tw(&mut self, i: i32, v: u32) {
        if (0..MAXB).contains(&i) {
            self.typing_word[i as usize] = v;
        }
    }
    fn or_tw(&mut self, i: i32, m: u32) {
        let v = self.tw(i) | m;
        self.set_tw(i, v);
    }
    fn and_tw(&mut self, i: i32, m: u32) {
        let v = self.tw(i) & m;
        self.set_tw(i, v);
    }
    fn set_hd(&mut self, i: i32, v: u32) {
        if (0..MAXB).contains(&i) {
            self.hook.char_data[i as usize] = v;
        }
    }
    /// Output code for engine data. The standalone flag is internal: C++ leaked it to the
    /// output when a standalone ư/ơ lost its horn, which was sent as a NUL character.
    fn get(&self, data: u32) -> u32 {
        char_code(data, self.s.code_table) & !STANDALONE_MASK
    }

    fn pc(&self, i: usize) -> u16 {
        PROCESSING_CHAR[(self.s.input_type as usize).min(3)][i]
    }
    fn is_vni(&self) -> bool {
        self.s.input_type == VNI
    }
    fn is_key_z(&self, k: u16) -> bool {
        self.pc(10) == k
    }
    fn is_key_d(&self, k: u16) -> bool {
        self.pc(9) == k
    }
    fn is_key_w(&self, k: u16) -> bool {
        self.pc(8) == k || (self.is_vni() && self.pc(7) == k)
    }
    fn is_key_double(&self, k: u16) -> bool {
        if self.is_vni() {
            self.pc(6) == k
        } else {
            self.pc(5) == k || self.pc(6) == k || self.pc(7) == k
        }
    }
    fn is_key_s(&self, k: u16) -> bool {
        self.pc(0) == k
    }
    fn is_key_f(&self, k: u16) -> bool {
        self.pc(1) == k
    }
    fn is_key_r(&self, k: u16) -> bool {
        self.pc(2) == k
    }
    fn is_key_x(&self, k: u16) -> bool {
        self.pc(3) == k
    }
    fn is_key_j(&self, k: u16) -> bool {
        self.pc(4) == k
    }
    fn is_mark_key(&self, k: u16) -> bool {
        if self.is_vni() {
            matches!(k, KEY_1 | KEY_2 | KEY_3 | KEY_5 | KEY_4)
        } else {
            matches!(k, KEY_S | KEY_F | KEY_R | KEY_J | KEY_X)
        }
    }
    fn is_special_key(&self, k: u16) -> bool {
        match self.s.input_type {
            TELEX => matches!(
                k,
                KEY_W | KEY_E | KEY_R | KEY_O | KEY_LEFT_BRACKET | KEY_RIGHT_BRACKET | KEY_A | KEY_S | KEY_D | KEY_F
                    | KEY_J | KEY_Z | KEY_X
            ),
            VNI => is_number_key(k),
            SIMPLE_TELEX_1 | SIMPLE_TELEX_2 => matches!(
                k,
                KEY_W | KEY_E | KEY_R | KEY_O | KEY_A | KEY_S | KEY_D | KEY_F | KEY_J | KEY_Z | KEY_X
            ),
            _ => false,
        }
    }
    fn is_quick_telex_key(&self, k: u16) -> bool {
        self.index > 0
            && matches!(k, KEY_C | KEY_G | KEY_K | KEY_N | KEY_Q | KEY_P | KEY_T)
            && self.chr(self.index - 1) == k
    }

    // ---------- public API ----------

    /// Reset per-word state (call on focus change, mouse click, ...).
    pub fn start_new_session(&mut self) {
        self.index = 0;
        self.hook.backspace_count = 0;
        self.hook.new_char_count = 0;
        self.temp_disable_key = false;
        self.state_index = 0;
        self.raw_keys.clear();
        self.has_handled_macro = false;
        self.has_handle_quick_consonant = false;
        self.long_word_helper.clear();
    }

    /// Toggle spell checking temporarily (hot key).
    pub fn temp_off_spell_checking(&mut self) {
        if self.s.check_spelling {
            self.spelling_on = !self.spelling_on;
        }
    }

    pub fn set_check_spelling(&mut self, on: bool) {
        self.s.check_spelling = on;
        self.spelling_on = on;
    }

    pub fn temp_off_engine(&mut self, off: bool) {
        self.will_temp_off_engine = off;
    }

    pub fn set_code_table(&mut self, table: u8) {
        self.s.code_table = table.min(4);
        self.macros.on_code_table_change(self.s.code_table);
    }

    // ---------- spelling ----------

    fn check_spelling(&mut self, force_check_vowel: bool) {
        self.spelling_ok = false;
        self.spelling_vowel_ok = true;
        let idx = self.index;
        let mut end = idx;
        if idx > 0 && self.chr(idx - 1) == KEY_RIGHT_BRACKET {
            end = idx - 1;
        }
        let qsc = if self.s.quick_start_consonant { END_CONSONANT_MASK } else { 0 };
        let qec = if self.s.quick_end_consonant { END_CONSONANT_MASK } else { 0 };
        let zfwj = if self.s.allow_consonant_zfwj { CONSONANT_ALLOW_MASK } else { 0 };

        if end > 0 {
            let mut j: i32 = 0;
            if is_consonant(self.chr(0)) {
                for entry in CONSONANT_TABLE {
                    let mut flag = end < entry.len() as i32;
                    j = 0;
                    while (j as usize) < entry.len() {
                        let e = entry[j as usize];
                        if end > j && (e & !qsc) != self.chr(j) && (e & !zfwj) != self.chr(j) {
                            flag = true;
                            break;
                        }
                        j += 1;
                    }
                    if flag {
                        continue;
                    }
                    break;
                }
            }
            if j == end {
                self.spelling_ok = true;
            }

            let mut k = j;
            self.vsi = k;
            if self.chr(self.vsi) == KEY_U && k > 0 && k < end - 1 && self.chr(self.vsi - 1) == KEY_Q {
                k += 1;
                j = k;
                self.vsi = k;
            } else if idx >= 2 && self.chr(0) == KEY_G && self.chr(1) == KEY_I && is_consonant(self.chr(2)) {
                k = 1;
                j = 1;
                self.vsi = 1;
            }
            for _ in 0..3 {
                if k < end && !is_consonant(self.chr(k)) {
                    k += 1;
                    self.vei = k;
                }
            }
            if k > j {
                self.spelling_vowel_ok = false;
                if k - j > 1 && force_check_vowel {
                    for entry in vowel_combine(self.chr(j)) {
                        let mut flag = false;
                        let mut ii: i32 = 1;
                        while (ii as usize) < entry.len() {
                            let p = j + ii - 1;
                            let cur = self.chr(p) as u32 | (self.tw(p) & TONEW_MASK) | (self.tw(p) & TONE_MASK);
                            if p < end && entry[ii as usize] != cur {
                                flag = true;
                                break;
                            }
                            ii += 1;
                        }
                        if flag
                            || (k < end && entry[0] == 0)
                            || (j + ii - 1 < end && !is_consonant(self.chr(j + ii - 1)))
                        {
                            continue;
                        }
                        self.spelling_vowel_ok = true;
                        break;
                    }
                } else if !is_consonant(self.chr(j)) {
                    self.spelling_vowel_ok = true;
                }

                for entry in END_CONSONANT_TABLE {
                    let mut flag = false;
                    j = 0;
                    while (j as usize) < entry.len() {
                        if end > k + j && (entry[j as usize] & !qec) != self.chr(k + j) {
                            flag = true;
                            break;
                        }
                        j += 1;
                    }
                    if flag {
                        continue;
                    }
                    if k + j >= end {
                        self.spelling_ok = true;
                        break;
                    }
                }

                // end consonant "ch", "t" can not go with "~", "`", "?"
                if self.spelling_ok {
                    let bad_mark = |w: u32| !((w & MARK1_MASK) != 0 || (w & MARK5_MASK) != 0 || (w & MARK_MASK) == 0);
                    let ch_end = idx >= 3
                        && self.chr(idx - 1) == KEY_H
                        && self.chr(idx - 2) == KEY_C
                        && bad_mark(self.tw(idx - 3));
                    let t_end = idx >= 2 && self.chr(idx - 1) == KEY_T && bad_mark(self.tw(idx - 2));
                    if ch_end || t_end {
                        self.spelling_ok = false;
                    }
                }
            }
        } else {
            self.spelling_ok = true;
        }
        self.temp_disable_key = !(self.spelling_ok && self.spelling_vowel_ok);
    }

    fn check_grammar(&mut self, delta_back_space: i32) {
        let idx = self.index;
        if idx <= 1 || idx >= MAXB {
            return;
        }
        self.find_and_calculate_vowel(true);
        if self.vowel_count == 0 {
            return;
        }
        self.is_checked_grammar = false;
        let l = self.vsi;

        // "thuơn", "ưoi", "ưom", "ưoc"
        if idx >= 3 {
            let mut i = idx - 1;
            while i >= 0 {
                if matches!(self.chr(i), KEY_N | KEY_C | KEY_I | KEY_M | KEY_P | KEY_T)
                    && i - 2 >= 0
                    && self.chr(i - 1) == KEY_O
                    && self.chr(i - 2) == KEY_U
                    && ((self.tw(i - 1) & TONEW_MASK) ^ (self.tw(i - 2) & TONEW_MASK)) != 0
                {
                    self.or_tw(i - 2, TONEW_MASK);
                    self.or_tw(i - 1, TONEW_MASK);
                    self.is_checked_grammar = true;
                    break;
                }
                i -= 1;
            }
        }

        // re-place the mark
        if idx >= 2 {
            let mut i = l;
            while i <= self.vei {
                if self.tw(i) & MARK_MASK != 0 {
                    let mark = self.tw(i) & MARK_MASK;
                    self.and_tw(i, !MARK_MASK);
                    self.insert_mark(mark, false);
                    if i != self.vwsm {
                        self.is_checked_grammar = true;
                    }
                    break;
                }
                i += 1;
            }
        }

        if self.is_checked_grammar {
            if self.hook.code == Code::DoNothing {
                self.hook.code = Code::WillProcess;
            }
            self.hook.backspace_count = 0;
            let mut i = idx - 1;
            while i >= l {
                self.hook.backspace_count += 1;
                let v = self.get(self.tw(i));
                self.set_hd(idx - 1 - i, v);
                i -= 1;
            }
            self.hook.new_char_count = self.hook.backspace_count;
            self.hook.backspace_count += delta_back_space;
            self.hook.ext_code = 4;
        }
    }

    fn insert_key(&mut self, key_code: u16, is_caps: bool, check_spelling: bool) {
        let v = key_code as u32 | if is_caps { CAPS_MASK } else { 0 };
        if self.index >= MAXB {
            self.long_word_helper.push(self.typing_word[0]);
            self.typing_word.copy_within(1.., 0);
            self.set_tw(self.index - 1, v);
        } else {
            self.set_tw(self.index, v);
            self.index += 1;
        }
        if self.spelling_on && check_spelling {
            self.check_spelling(false);
        }
        // allow d after consonant
        if key_code == KEY_D && self.index - 2 >= 0 && is_consonant(self.chr(self.index - 2)) {
            self.temp_disable_key = false;
        }
    }

    fn insert_state(&mut self, key_code: u16, is_caps: bool) {
        let v = key_code as u32 | if is_caps { CAPS_MASK } else { 0 };
        if self.raw_keys.len() < MAX_BUFF {
            self.raw_keys.push(v);
        }
        if self.state_index >= MAXB {
            self.key_states.copy_within(1.., 0);
            self.key_states[MAX_BUFF - 1] = v;
        } else {
            self.key_states[self.state_index as usize] = v;
            self.state_index += 1;
        }
    }

    fn push_chunked(&mut self, data: &[u32]) {
        for chunk in data.chunks(MAX_BUFF) {
            self.typing_states.push(chunk.to_vec());
        }
        if data.is_empty() {
            self.typing_states.push(Vec::new());
        }
    }

    fn save_word(&mut self) {
        if self.hook.code != Code::ReplaceMacro {
            if self.index > 0 {
                if !self.long_word_helper.is_empty() {
                    let long = std::mem::take(&mut self.long_word_helper);
                    self.push_chunked(&long);
                }
                let word = self.typing_word[..self.index as usize].to_vec();
                self.typing_states.push(word);
            }
        } else {
            let data = self.hook.macro_data.clone();
            self.push_chunked(&data);
        }
    }

    fn save_word_repeat(&mut self, key_code: u32, count: i32) {
        self.typing_states.push(vec![key_code; count.max(0) as usize]);
    }

    fn save_special_char(&mut self) {
        let sc = std::mem::take(&mut self.special_char);
        self.typing_states.push(sc);
    }

    fn restore_last_typing_state(&mut self) {
        let Some(data) = self.typing_states.pop() else { return };
        if data.is_empty() {
            return;
        }
        if data[0] == KEY_SPACE as u32 {
            self.space_count = data.len() as i32;
            self.index = 0;
        } else if CHAR_KEY_CODE.contains(&(data[0] as u16)) {
            self.index = 0;
            self.special_char = data;
            self.check_spelling(false);
        } else {
            let n = data.len().min(MAX_BUFF);
            self.typing_word[..n].copy_from_slice(&data[..n]);
            self.index = n as i32;
        }
    }

    fn check_correct_vowel(&mut self, entry: &[u16], mark_key: u16) {
        let idx = self.index;
        // ignore "qu"
        if idx >= 2 && self.chr(idx - 1) == KEY_U && self.chr(idx - 2) == KEY_Q {
            self.is_correct = false;
            return;
        }
        let qec = if self.s.quick_end_consonant { END_CONSONANT_MASK } else { 0 };
        let mut k = idx - 1;
        for &e in entry.iter().rev() {
            if (e & !qec) != self.chr(k) {
                self.is_correct = false;
                return;
            }
            k -= 1;
            if k < 0 {
                break;
            }
        }
        // limit mark for end consonant "c", "t"
        if self.is_correct
            && entry.len() > 1
            && (self.is_key_f(mark_key) || self.is_key_x(mark_key) || self.is_key_r(mark_key))
            && (entry[1] == KEY_C || entry[1] == KEY_T || (entry.len() > 2 && entry[2] == KEY_T))
        {
            self.is_correct = false;
        }
        if self.is_correct && k >= 0 && self.chr(k) == self.chr(k + 1) {
            self.is_correct = false;
        }
    }

    fn find_and_calculate_vowel(&mut self, for_grammar: bool) {
        self.vowel_count = 0;
        self.vsi = 0;
        self.vei = 0;
        let mut i = self.index - 1;
        while i >= 0 {
            if is_consonant(self.chr(i)) {
                if self.vowel_count > 0 {
                    break;
                }
            } else {
                if self.vowel_count == 0 {
                    self.vei = i;
                }
                if !for_grammar
                    && i > 0
                    && ((self.chr(i) == KEY_I && self.chr(i - 1) == KEY_G)
                        || (self.chr(i) == KEY_U && self.chr(i - 1) == KEY_Q))
                {
                    break;
                }
                self.vsi = i;
                self.vowel_count += 1;
            }
            i -= 1;
        }
        // don't count "u" in "qu" as a vowel
        if self.vsi > 0 && self.chr(self.vsi) == KEY_U && self.chr(self.vsi - 1) == KEY_Q {
            self.vsi += 1;
            self.vowel_count -= 1;
        }
    }

    fn remove_mark(&mut self) {
        self.find_and_calculate_vowel(true);
        self.is_changed = false;
        if self.index > 0 {
            for i in self.vsi..=self.vei {
                if self.tw(i) & MARK_MASK != 0 {
                    self.and_tw(i, !MARK_MASK);
                    self.is_changed = true;
                }
            }
        }
        if self.is_changed {
            self.hook.code = Code::WillProcess;
            self.hook.backspace_count = 0;
            let mut i = self.index - 1;
            while i >= self.vsi {
                self.hook.backspace_count += 1;
                let v = self.get(self.tw(i));
                self.set_hd(self.index - 1 - i, v);
                i -= 1;
            }
            self.hook.new_char_count = self.hook.backspace_count;
        } else {
            self.hook.code = Code::DoNothing;
        }
    }

    fn can_has_end_consonant(&self) -> bool {
        for entry in vowel_combine(self.chr(self.vsi)) {
            let mut kk = self.vsi;
            let mut iii = 1;
            while iii < entry.len() {
                let cur = self.chr(kk) as u32 | (self.tw(kk) & TONE_MASK) | (self.tw(kk) & TONEW_MASK);
                if kk > self.vei || cur != entry[iii] {
                    break;
                }
                kk += 1;
                iii += 1;
            }
            if iii >= entry.len() {
                return entry[0] == 1;
            }
        }
        false
    }

    fn set_mark_pos(&mut self, pos: i32) {
        self.vwsm = pos;
        self.hook.backspace_count = self.index - pos;
    }

    fn handle_modern_mark(&mut self) {
        let (vsi, vei, idx) = (self.vsi, self.vei, self.index);
        let word = self.typing_word;
        let c = |i: i32| if (0..MAXB).contains(&i) { word[i as usize] as u16 } else { 0 };
        self.set_mark_pos(vei);

        // rule 2
        if self.vowel_count == 3
            && ((c(vsi) == KEY_O && c(vsi + 1) == KEY_A && c(vsi + 2) == KEY_I)
                || (c(vsi) == KEY_U && c(vsi + 1) == KEY_Y && c(vsi + 2) == KEY_U)
                || (c(vsi) == KEY_O && c(vsi + 1) == KEY_E && c(vsi + 2) == KEY_O)
                || (c(vsi) == KEY_U && c(vsi + 1) == KEY_Y && c(vsi + 2) == KEY_A))
        {
            self.set_mark_pos(vsi + 1);
        } else if (c(vsi) == KEY_O && c(vsi + 1) == KEY_I)
            || (c(vsi) == KEY_A && c(vsi + 1) == KEY_I)
            || (c(vsi) == KEY_U && c(vsi + 1) == KEY_I)
        {
            self.set_mark_pos(vsi);
        } else if c(vei - 1) == KEY_A && c(vei) == KEY_Y {
            self.set_mark_pos(vei - 1);
        } else if c(vsi) == KEY_U && c(vsi + 1) == KEY_O {
            self.set_mark_pos(vsi + 1);
        } else if c(vsi + 1) == KEY_O || c(vsi + 1) == KEY_U {
            self.set_mark_pos(vei - 1);
        } else if c(vsi) == KEY_O || c(vsi) == KEY_U {
            self.set_mark_pos(vei);
        }

        // rule 3.1
        let next = self.tw(vsi + 1);
        if (c(vsi) == KEY_I && next & (KEY_E as u32 | TONE_MASK) != 0)
            || (c(vsi) == KEY_Y && next & (KEY_E as u32 | TONE_MASK) != 0)
            || (c(vsi) == KEY_U && self.bare(vsi + 1) == (KEY_O as u32 | TONE_MASK))
            || (self.bare(vsi) == (KEY_U as u32 | TONEW_MASK) && self.bare(vsi + 1) == (KEY_O as u32 | TONEW_MASK))
        {
            if vsi + 2 < idx {
                if matches!(c(vsi + 2), KEY_P | KEY_T | KEY_M | KEY_N | KEY_O | KEY_U | KEY_I | KEY_C) {
                    self.set_mark_pos(vsi + 1);
                } else {
                    self.set_mark_pos(vsi);
                }
            } else {
                self.set_mark_pos(vsi);
            }
        }
        // rule 3.2
        else if c(vsi) == KEY_U && self.bare(vsi + 1) == (KEY_U as u32 | TONEW_MASK) {
            self.set_mark_pos(vsi);
        }

        // rule 4
        if self.vowel_count == 2 {
            if c(vsi) == KEY_I && matches!(c(vsi + 1), KEY_A | KEY_U | KEY_O) {
                if vsi == 0 || c(vsi - 1) != KEY_G {
                    self.set_mark_pos(vsi);
                } else {
                    self.set_mark_pos(vsi + 1);
                }
            } else if c(vsi) == KEY_U && c(vsi + 1) == KEY_A {
                if vsi == 0 || c(vsi - 1) != KEY_Q {
                    if vei + 1 >= idx || !self.can_has_end_consonant() {
                        self.set_mark_pos(vsi);
                    }
                } else {
                    self.set_mark_pos(vsi + 1);
                }
            } else if c(vsi) == KEY_O && c(vsi + 1) == KEY_O {
                self.set_mark_pos(vei);
            }
        }
    }

    fn handle_old_mark(&mut self) {
        if self.vowel_count == 0 && self.chr(self.vei) == KEY_I {
            self.set_mark_pos(self.vei);
        } else {
            self.set_mark_pos(self.vsi);
        }
        // rule 2
        if self.vowel_count == 3
            || (self.vei + 1 < self.index && is_consonant(self.chr(self.vei + 1)) && self.can_has_end_consonant())
        {
            self.set_mark_pos(self.vsi + 1);
        }
        // rule 3
        for ii in self.vsi..=self.vei {
            if (self.chr(ii) == KEY_E && self.tw(ii) & TONE_MASK != 0)
                || (self.chr(ii) == KEY_O && self.tw(ii) & TONEW_MASK != 0)
            {
                self.set_mark_pos(ii);
                break;
            }
        }
        self.hook.new_char_count = self.hook.backspace_count;
    }

    fn insert_mark(&mut self, mark_mask: u32, can_modify_flag: bool) {
        self.vowel_count = 0;
        if can_modify_flag {
            self.hook.code = Code::WillProcess;
        }
        self.hook.backspace_count = 0;
        self.hook.new_char_count = 0;
        self.find_and_calculate_vowel(false);
        self.vwsm = 0;

        if self.vowel_count == 1 {
            self.set_mark_pos(self.vei);
        } else {
            if self.s.modern_orthography {
                self.handle_modern_mark();
            } else {
                self.handle_old_mark();
            }
            if self.tw(self.vei) & (TONE_MASK | TONEW_MASK) != 0 {
                self.vwsm = self.vei;
            }
        }

        let mut kk = self.index - 1 - self.vsi;
        if self.tw(self.vwsm) & mark_mask != 0 {
            // same mark twice -> restore
            self.and_tw(self.vwsm, !MARK_MASK);
            if can_modify_flag {
                self.hook.code = Code::Restore;
            }
            for ii in self.vsi..self.index {
                self.and_tw(ii, !MARK_MASK);
                let v = self.get(self.tw(ii));
                self.set_hd(kk, v);
                kk -= 1;
            }
            self.temp_disable_key = true;
        } else {
            self.and_tw(self.vwsm, !MARK_MASK);
            self.or_tw(self.vwsm, mark_mask);
            for ii in self.vsi..self.index {
                if ii != self.vwsm {
                    self.and_tw(ii, !MARK_MASK);
                }
                let v = self.get(self.tw(ii));
                self.set_hd(kk, v);
                kk -= 1;
            }
            self.hook.backspace_count = self.index - self.vsi;
        }
        self.hook.new_char_count = self.hook.backspace_count;
    }

    fn insert_d(&mut self) {
        self.hook.code = Code::WillProcess;
        self.hook.backspace_count = 0;
        let mut ii = self.index - 1;
        while ii >= 0 {
            self.hook.backspace_count += 1;
            let p = self.index - 1 - ii;
            if self.chr(ii) == KEY_D {
                if self.tw(ii) & TONE_MASK != 0 {
                    self.hook.code = Code::Restore;
                    self.and_tw(ii, !TONE_MASK);
                    self.set_hd(p, self.tw(ii));
                    self.temp_disable_key = true;
                } else {
                    self.or_tw(ii, TONE_MASK);
                    let v = self.get(self.tw(ii));
                    self.set_hd(p, v);
                }
                break;
            }
            let v = self.get(self.tw(ii));
            self.set_hd(p, v);
            ii -= 1;
        }
        self.hook.new_char_count = self.hook.backspace_count;
    }

    fn insert_aoe(&mut self, data: u16) {
        self.find_and_calculate_vowel(false);
        for ii in self.vsi..=self.vei {
            self.and_tw(ii, !TONEW_MASK);
        }
        self.hook.code = Code::WillProcess;
        self.hook.backspace_count = 0;
        let mut ii = self.index - 1;
        while ii >= 0 {
            self.hook.backspace_count += 1;
            let p = self.index - 1 - ii;
            if self.chr(ii) == data {
                if self.tw(ii) & TONE_MASK != 0 {
                    self.hook.code = Code::Restore;
                    self.and_tw(ii, !TONE_MASK);
                    // fix: C++ sent the raw data here, which is a NUL char when the vowel has a mark
                    let v = self.get(self.tw(ii));
                    self.set_hd(p, v);
                    if data != KEY_O {
                        // case thoòng
                        self.temp_disable_key = true;
                    }
                } else {
                    self.or_tw(ii, TONE_MASK);
                    if !self.is_key_d(data) {
                        self.and_tw(ii, !TONEW_MASK);
                    }
                    let v = self.get(self.tw(ii));
                    self.set_hd(p, v);
                }
                break;
            }
            let v = self.get(self.tw(ii));
            self.set_hd(p, v);
            ii -= 1;
        }
        self.hook.new_char_count = self.hook.backspace_count;
    }

    fn insert_w(&mut self) {
        self.is_restored_w = false;
        self.find_and_calculate_vowel(false);
        for ii in self.vsi..=self.vei {
            self.and_tw(ii, !TONE_MASK);
        }
        let (vsi, idx) = (self.vsi, self.index);

        if self.vowel_count > 1 {
            self.hook.backspace_count = idx - vsi;
            self.hook.new_char_count = self.hook.backspace_count;
            let w0 = self.tw(vsi) & TONEW_MASK != 0;
            if (w0 && self.tw(vsi + 1) & TONEW_MASK != 0)
                || (w0 && self.chr(vsi + 1) == KEY_I)
                || (w0 && self.chr(vsi + 1) == KEY_A)
            {
                self.hook.code = Code::Restore;
                for ii in vsi..idx {
                    self.and_tw(ii, !TONEW_MASK);
                    let v = self.get(self.tw(ii)) & !STANDALONE_MASK;
                    self.set_hd(idx - 1 - ii, v);
                }
                self.is_restored_w = true;
                self.temp_disable_key = true;
            } else {
                self.hook.code = Code::WillProcess;
                let (c0, c1) = (self.chr(vsi), self.chr(vsi + 1));
                if c0 == KEY_U && c1 == KEY_O {
                    if vsi - 2 >= 0 && self.chr(vsi - 2) == KEY_T && self.chr(vsi - 1) == KEY_H {
                        self.or_tw(vsi + 1, TONEW_MASK);
                        if vsi + 2 < idx && self.chr(vsi + 2) == KEY_N {
                            self.or_tw(vsi, TONEW_MASK);
                        }
                    } else if vsi > 0 && self.chr(vsi - 1) == KEY_Q {
                        self.or_tw(vsi + 1, TONEW_MASK);
                    } else {
                        self.or_tw(vsi, TONEW_MASK);
                        self.or_tw(vsi + 1, TONEW_MASK);
                    }
                } else if (c0 == KEY_U && c1 == KEY_A)
                    || (c0 == KEY_U && c1 == KEY_I)
                    || (c0 == KEY_U && c1 == KEY_U)
                    || (c0 == KEY_O && c1 == KEY_I)
                {
                    self.or_tw(vsi, TONEW_MASK);
                } else if (c0 == KEY_I && c1 == KEY_O) || (c0 == KEY_O && c1 == KEY_A) {
                    self.or_tw(vsi + 1, TONEW_MASK);
                } else {
                    self.temp_disable_key = true;
                    self.is_changed = false;
                    self.hook.code = Code::DoNothing;
                }
                for ii in vsi..idx {
                    let v = self.get(self.tw(ii));
                    self.set_hd(idx - 1 - ii, v);
                }
            }
            return;
        }

        self.hook.code = Code::WillProcess;
        self.hook.backspace_count = 0;
        let mut ii = idx - 1;
        while ii >= 0 && ii >= vsi {
            self.hook.backspace_count += 1;
            let p = idx - 1 - ii;
            match self.chr(ii) {
                KEY_A | KEY_U | KEY_O => {
                    if self.tw(ii) & TONEW_MASK != 0 {
                        let caps = self.tw(ii) & CAPS_MASK;
                        if self.tw(ii) & STANDALONE_MASK != 0 {
                            self.hook.code = Code::WillProcess;
                            if self.chr(ii) == KEY_U {
                                self.set_tw(ii, KEY_W as u32 | caps);
                            } else if self.chr(ii) == KEY_O {
                                self.hook.code = Code::Restore;
                                self.set_tw(ii, KEY_O as u32 | caps);
                                self.is_restored_w = true;
                            }
                            self.set_hd(p, self.tw(ii));
                        } else {
                            self.hook.code = Code::Restore;
                            self.and_tw(ii, !TONEW_MASK);
                            let v = self.get(self.tw(ii));
                            self.set_hd(p, v);
                            self.is_restored_w = true;
                        }
                        self.temp_disable_key = true;
                    } else {
                        self.or_tw(ii, TONEW_MASK);
                        self.and_tw(ii, !TONE_MASK);
                        let v = self.get(self.tw(ii));
                        self.set_hd(p, v);
                    }
                }
                _ => {
                    let v = self.get(self.tw(ii));
                    self.set_hd(p, v);
                }
            }
            ii -= 1;
        }
        self.hook.new_char_count = self.hook.backspace_count;
    }

    fn reverse_last_standalone_char(&mut self, key_code: u16, is_caps: bool) {
        self.hook.code = Code::WillProcess;
        self.hook.backspace_count = 0;
        self.hook.new_char_count = 1;
        self.hook.ext_code = 4;
        let v = key_code as u32 | TONEW_MASK | STANDALONE_MASK | if is_caps { CAPS_MASK } else { 0 };
        self.set_tw(self.index - 1, v);
        self.hook.char_data[0] = self.get(v);
    }

    fn check_for_standalone_char(&mut self, data: u16, is_caps: bool, key_will_reverse: u16) {
        let idx = self.index;
        if idx > 0 && self.chr(idx - 1) == key_will_reverse && self.tw(idx - 1) & TONEW_MASK != 0 {
            self.hook.code = Code::WillProcess;
            self.hook.backspace_count = 1;
            self.hook.new_char_count = 1;
            let v = data as u32 | if is_caps { CAPS_MASK } else { 0 };
            self.set_tw(idx - 1, v);
            self.hook.char_data[0] = self.get(v);
            return;
        }
        // standalone w -> ư
        if idx > 0 && self.chr(idx - 1) == KEY_U && key_will_reverse == KEY_O {
            self.insert_key(key_will_reverse, is_caps, true);
            self.reverse_last_standalone_char(key_will_reverse, is_caps);
            return;
        }
        match idx {
            0 => {
                self.insert_key(data, is_caps, false);
                self.reverse_last_standalone_char(key_will_reverse, is_caps);
            }
            1 => {
                if STANDALONE_W_BAD.contains(&self.chr(0)) {
                    self.insert_key(data, is_caps, true);
                } else {
                    self.insert_key(data, is_caps, false);
                    self.reverse_last_standalone_char(key_will_reverse, is_caps);
                }
            }
            2 => {
                let first = [self.chr(0), self.chr(1)];
                if DOUBLE_W_ALLOWED.contains(&first) {
                    self.insert_key(data, is_caps, false);
                    self.reverse_last_standalone_char(key_will_reverse, is_caps);
                } else {
                    self.insert_key(data, is_caps, true);
                }
            }
            _ => self.insert_key(data, is_caps, true),
        }
    }

    fn upper_case_first_character(&mut self) {
        if self.typing_word[0] & CAPS_MASK == 0 {
            self.hook.code = Code::WillProcess;
            self.hook.backspace_count = 0;
            self.hook.new_char_count = 1;
            self.typing_word[0] |= CAPS_MASK;
            self.hook.char_data[0] = self.get(self.typing_word[0]);
            self.upper_case_status = 0;
            if self.s.use_macro {
                if let Some(first) = self.hook.macro_key.first_mut() {
                    *first |= CAPS_MASK;
                }
            }
        }
    }

    fn handle_main_key(&mut self, data: u16, is_caps: bool) {
        if self.is_key_z(data) {
            self.remove_mark();
            if !self.is_changed {
                self.insert_key(data, is_caps, true);
            }
            return;
        }
        if data == KEY_LEFT_BRACKET {
            self.check_for_standalone_char(data, is_caps, KEY_O);
            return;
        }
        if data == KEY_RIGHT_BRACKET {
            self.check_for_standalone_char(data, is_caps, KEY_U);
            return;
        }

        if self.is_key_d(data) {
            self.is_correct = false;
            self.is_changed = false;
            for entry in CONSONANT_D {
                if self.index < entry.len() as i32 {
                    continue;
                }
                self.is_correct = true;
                self.check_correct_vowel(entry, data);
                // allow d after consonant
                if !self.is_correct
                    && self.index - 2 >= 0
                    && self.chr(self.index - 1) == KEY_D
                    && is_consonant(self.chr(self.index - 2))
                {
                    self.is_correct = true;
                }
                if self.is_correct {
                    self.is_changed = true;
                    self.insert_d();
                    break;
                }
            }
            if !self.is_changed {
                self.insert_key(data, is_caps, true);
            }
            return;
        }

        if self.is_mark_key(data) {
            'sets: for charset in VOWEL_FOR_MARK {
                self.is_correct = false;
                self.is_changed = false;
                for entry in charset.iter() {
                    if self.index < entry.len() as i32 {
                        continue;
                    }
                    self.is_correct = true;
                    self.check_correct_vowel(entry, data);
                    if self.is_correct {
                        self.is_changed = true;
                        let mark = if self.is_key_s(data) {
                            MARK1_MASK
                        } else if self.is_key_f(data) {
                            MARK2_MASK
                        } else if self.is_key_r(data) {
                            MARK3_MASK
                        } else if self.is_key_x(data) {
                            MARK4_MASK
                        } else if self.is_key_j(data) {
                            MARK5_MASK
                        } else {
                            0
                        };
                        if mark != 0 {
                            self.insert_mark(mark, true);
                        }
                        break 'sets;
                    }
                }
            }
            if !self.is_changed {
                self.insert_key(data, is_caps, true);
            }
            return;
        }

        if self.is_vni() {
            let mut i = self.index - 1;
            while i >= 0 {
                if matches!(self.chr(i), KEY_O | KEY_A | KEY_E) {
                    self.vei = i;
                    break;
                }
                i -= 1;
            }
        }

        let key_for_aeo: u16 = if !self.is_vni() {
            data
        } else if data == KEY_7 || data == KEY_8 {
            KEY_W
        } else if data == KEY_6 {
            self.tw(self.vei) as u16
        } else {
            data
        };
        self.is_correct = false;
        self.is_changed = false;
        for entry in vowel(key_for_aeo) {
            if self.index < entry.len() as i32 {
                continue;
            }
            self.is_correct = true;
            self.check_correct_vowel(entry, data);
            if self.is_correct {
                self.is_changed = true;
                if self.is_key_double(data) {
                    self.insert_aoe(key_for_aeo);
                } else if self.is_key_w(data) {
                    if self.is_vni() {
                        let mut j = self.index - 1;
                        while j >= 0 {
                            if matches!(self.chr(j), KEY_O | KEY_U | KEY_A | KEY_E) {
                                self.vei = j;
                                break;
                            }
                            j -= 1;
                        }
                        let vei = self.vei;
                        if (data == KEY_7
                            && self.chr(vei) == KEY_A
                            && (vei - 1 < 0 || self.chr(vei - 1) != KEY_U))
                            || (data == KEY_8 && matches!(self.chr(vei), KEY_O | KEY_U))
                        {
                            break;
                        }
                    }
                    self.insert_w();
                }
                break;
            }
        }

        if !self.is_changed {
            if data == KEY_W && self.s.input_type != SIMPLE_TELEX_1 {
                self.check_for_standalone_char(data, is_caps, KEY_U);
            } else {
                self.insert_key(data, is_caps, true);
            }
        }
    }

    fn handle_quick_telex(&mut self, data: u16, is_caps: bool) {
        let Some(qt) = quick_telex(data) else { return };
        let caps = if is_caps { CAPS_MASK } else { 0 };
        self.hook.code = Code::WillProcess;
        self.hook.backspace_count = 1;
        self.hook.new_char_count = 2;
        self.hook.char_data[1] = qt[0] as u32 | caps;
        self.hook.char_data[0] = qt[1] as u32 | caps;
        self.insert_key(qt[1], is_caps, false);
    }

    fn check_restore_if_wrong_spelling(&mut self, handle_code: Code) -> bool {
        // Changed by a mark, or keys were consumed: "boss" shows "bos" (the second s removed
        // the first one's mark), which has no mark left (the C++ version missed that).
        let consumed = self.index != self.state_index;
        for ii in 0..self.index {
            if consumed || (!is_consonant(self.chr(ii)) && self.tw(ii) & (MARK_MASK | TONE_MASK | TONEW_MASK) != 0) {
                self.hook.code = handle_code;
                self.hook.backspace_count = self.index;
                self.hook.new_char_count = self.state_index;
                for i in 0..self.state_index {
                    self.typing_word[i as usize] = self.key_states[i as usize];
                    self.set_hd(self.state_index - 1 - i, self.key_states[i as usize]);
                }
                self.index = self.state_index;
                return true;
            }
        }
        false
    }

    /// The current word is not Vietnamese: give back the keys as typed, if an option asks
    /// for it. An English word gets all its keys back ("boss", not "bos").
    fn restore_word(&mut self, code: Code) -> bool {
        if self.s.restore_english && self.typed_english_word() {
            let n = self.raw_keys.len().min(MAX_BUFF);
            self.key_states[..n].copy_from_slice(&self.raw_keys[..n]);
            self.state_index = n as i32;
        } else if !self.s.restore_if_wrong_spelling {
            return false;
        }
        self.check_restore_if_wrong_spelling(code)
    }

    /// The keys of the current word spell an English word that Telex would change
    /// ("boss" -> "bos", "good" -> "gôd").
    fn typed_english_word(&self) -> bool {
        let mut word = String::new();
        for &k in &self.raw_keys {
            match key_to_char(k & !CAPS_MASK) as u8 {
                c @ b'a'..=b'z' => word.push(c as char),
                _ => return false,
            }
        }
        word.len() >= 3 && english_words().binary_search(&word.as_str()).is_ok()
    }

    /// The current word failed the spelling check (used to build the English word list).
    #[doc(hidden)]
    pub fn spelling_failed(&self) -> bool {
        self.temp_disable_key
    }

    /// Esc right after a word was changed: put back the keys exactly as typed, and keep the
    /// rest of the word as typed. False (nothing to undo) if the word shows as typed.
    pub fn undo_word(&mut self) -> bool {
        let n = self.raw_keys.len().min(MAX_BUFF) as i32;
        if self.index == 0 || n == 0 {
            return false;
        }
        let raw = self.raw_keys.clone();
        self.key_states[..raw.len()].copy_from_slice(&raw);
        self.state_index = n;
        let changed = self.index != n || (0..self.index).any(|i| self.tw(i) & (MARK_MASK | TONE_MASK | TONEW_MASK) != 0);
        if !changed {
            return false;
        }
        self.hook.code = Code::WillProcess;
        self.hook.ext_code = 0;
        self.hook.backspace_count = self.index;
        self.hook.new_char_count = n;
        for i in 0..n {
            self.typing_word[i as usize] = self.key_states[i as usize];
            self.set_hd(n - 1 - i, self.key_states[i as usize]);
        }
        self.index = n;
        self.temp_disable_key = true;
        true
    }

    fn check_quick_consonant(&mut self) -> bool {
        if self.index <= 1 {
            return false;
        }
        let mut changed = false;
        if self.s.quick_start_consonant {
            if let Some(q) = quick_start_consonant(self.chr(0)) {
                self.hook.code = Code::Restore;
                self.hook.backspace_count = self.index;
                self.hook.new_char_count = self.index + 1;
                if self.index < MAXB - 1 {
                    self.index += 1;
                }
                let mut i = self.index - 1;
                while i >= 2 {
                    let v = self.tw(i - 1);
                    self.set_tw(i, v);
                    i -= 1;
                }
                let caps0 = self.tw(0) & CAPS_MASK;
                let both_caps = if caps0 != 0 && self.tw(2) & CAPS_MASK != 0 { CAPS_MASK } else { 0 };
                self.set_tw(1, q[1] as u32 | both_caps);
                self.set_tw(0, q[0] as u32 | caps0);
                changed = true;
            }
        }
        if self.s.quick_end_consonant && self.index - 2 >= 0 && !is_consonant(self.chr(self.index - 2)) {
            if let Some(q) = quick_end_consonant(self.chr(self.index - 1)) {
                self.hook.code = Code::Restore;
                if changed {
                    self.hook.new_char_count += 1;
                } else {
                    self.hook.backspace_count = 1;
                    self.hook.new_char_count = 2;
                }
                if self.index < MAXB - 1 {
                    self.index += 1;
                }
                let caps = self.tw(self.index - 2) & CAPS_MASK;
                self.set_tw(self.index - 1, q[1] as u32 | caps);
                self.set_tw(self.index - 2, q[0] as u32 | caps);
                changed = true;
            }
        }
        if changed {
            self.has_handle_quick_consonant = true;
            let mut i = self.index - 1;
            while i >= 0 {
                let v = self.get(self.tw(i));
                self.set_hd(self.index - 1 - i, v);
                i -= 1;
            }
        }
        changed
    }

    fn find_macro(&mut self) -> bool {
        let table = self.s.code_table;
        let auto_caps = self.s.auto_caps_macro;
        match self.macros.find(&mut self.hook.macro_key, table, auto_caps) {
            Some(data) => {
                self.hook.macro_data = data;
                self.last_macro = self.macros.text_of(&self.hook.macro_key).map(str::to_string);
                true
            }
            None => false,
        }
    }

    /// Key handling while in English mode (only macros are processed).
    pub fn english_mode(&mut self, state: KeyState, data: u16, is_caps: bool, other_control_key: bool) {
        self.hook.code = Code::DoNothing;
        if state == KeyState::MouseDown || (other_control_key && !is_caps) {
            self.hook.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if data == KEY_SPACE {
            if !self.has_handled_macro && self.find_macro() {
                self.hook.code = Code::ReplaceMacro;
                self.hook.backspace_count = self.hook.macro_key.len() as i32;
            }
            self.hook.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if data == KEY_DELETE {
            if self.hook.macro_key.pop().is_none() {
                self.will_temp_off_engine = false;
            }
        } else if is_word_break(KeyEvent::Keyboard, data) && !CHAR_KEY_CODE.contains(&data) {
            self.hook.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if !self.will_temp_off_engine {
            self.hook.macro_key.push(data as u32 | if is_caps { CAPS_MASK } else { 0 });
        }
    }

    /// Main entry point for each key.
    /// `caps_status`: 0 none, 1 shift, 2 caps lock (letter will be upper case).
    pub fn handle_event(&mut self, event: KeyEvent, state: KeyState, data: u16, caps_status: u8, other_control_key: bool) {
        self.is_caps = caps_status == 1 || caps_status == 2;
        let is_caps = self.is_caps;
        let caps_bit = if is_caps { CAPS_MASK } else { 0 };

        if (is_number_key(data) && caps_status == 1)
            || other_control_key
            || is_word_break(event, data)
            || (self.index == 0 && is_number_key(data))
        {
            self.hook.code = Code::DoNothing;
            self.hook.backspace_count = 0;
            self.hook.new_char_count = 0;
            self.hook.ext_code = 1;

            let macro_break = MACRO_BREAK_CODE.contains(&data);
            if self.s.use_macro && macro_break && !self.has_handled_macro && self.find_macro() {
                self.hook.code = Code::ReplaceMacro;
                self.hook.backspace_count = self.hook.macro_key.len() as i32;
                self.has_handled_macro = true;
            } else if (self.s.quick_start_consonant || self.s.quick_end_consonant)
                && !self.temp_disable_key
                && macro_break
            {
                self.check_quick_consonant();
            } else if (self.s.restore_if_wrong_spelling || self.s.restore_english) && is_word_break(event, data) {
                if !self.temp_disable_key && self.spelling_on {
                    self.check_spelling(true);
                }
                if self.temp_disable_key && !self.restore_word(Code::RestoreAndStartNewSession) {
                    self.hook.code = Code::DoNothing;
                }
            }

            let is_char_key_code = state == KeyState::KeyDown && CHAR_KEY_CODE.contains(&data);
            if !is_char_key_code {
                self.special_char.clear();
                self.typing_states.clear();
            } else {
                if self.space_count > 0 {
                    self.save_word_repeat(KEY_SPACE as u32, self.space_count);
                    self.space_count = 0;
                } else {
                    self.save_word();
                }
                self.special_char.push(data as u32 | caps_bit);
                self.hook.ext_code = 3;
            }

            if self.hook.code == Code::DoNothing {
                self.start_new_session();
                self.spelling_on = self.s.check_spelling;
                self.will_temp_off_engine = false;
            } else if self.hook.code == Code::ReplaceMacro || self.has_handle_quick_consonant {
                self.index = 0;
            }

            if self.s.use_macro {
                if is_char_key_code {
                    self.hook.macro_key.push(data as u32 | caps_bit);
                } else {
                    self.hook.macro_key.clear();
                }
            }

            if self.s.upper_case_first_char {
                self.upper_case_status = match data {
                    KEY_DOT => 1,
                    KEY_ENTER | KEY_RETURN => 2,
                    _ => 0,
                };
            }
        } else if data == KEY_SPACE {
            if !self.temp_disable_key && self.spelling_on {
                self.check_spelling(true);
            }
            if self.s.use_macro && !self.has_handled_macro && self.find_macro() {
                self.hook.code = Code::ReplaceMacro;
                self.hook.backspace_count = self.hook.macro_key.len() as i32;
                self.space_count += 1;
                self.has_handled_macro = true;
            } else if (self.s.quick_start_consonant || self.s.quick_end_consonant)
                && !self.temp_disable_key
                && self.check_quick_consonant()
            {
                self.space_count += 1;
            } else if self.temp_disable_key
                && !self.has_handled_macro
                && (self.s.restore_if_wrong_spelling || self.s.restore_english)
            {
                if !self.restore_word(Code::Restore) {
                    self.hook.code = Code::DoNothing;
                }
                self.space_count += 1;
            } else {
                self.hook.code = Code::DoNothing;
                self.space_count += 1;
            }
            if self.s.use_macro {
                self.hook.macro_key.clear();
            }
            if self.s.upper_case_first_char && self.upper_case_status == 1 {
                self.upper_case_status = 2;
            }
            if self.space_count == 1 {
                if !self.special_char.is_empty() {
                    self.save_special_char();
                } else {
                    self.save_word();
                }
            }
            self.spelling_on = self.s.check_spelling;
            self.will_temp_off_engine = false;
        } else if data == KEY_DELETE {
            self.hook.code = Code::DoNothing;
            self.hook.ext_code = 2;
            if !self.special_char.is_empty() {
                self.special_char.pop();
                if self.special_char.is_empty() {
                    self.restore_last_typing_state();
                }
            } else if self.space_count > 0 {
                self.space_count -= 1;
                if self.space_count == 0 {
                    self.restore_last_typing_state();
                }
            } else {
                if self.state_index > 0 {
                    self.state_index -= 1;
                }
                self.raw_keys.pop();
                if self.index > 0 {
                    self.index -= 1;
                    if let Some(prev) = self.long_word_helper.pop() {
                        self.typing_word.copy_within(..MAX_BUFF - 1, 1);
                        self.typing_word[0] = prev;
                        self.index += 1;
                    }
                    if self.spelling_on {
                        self.check_spelling(false);
                    }
                }
                if self.s.use_macro {
                    self.hook.macro_key.pop();
                }
                self.hook.backspace_count = 0;
                self.hook.new_char_count = 0;
                self.hook.ext_code = 2;
                if self.index == 0 {
                    self.start_new_session();
                    self.special_char.clear();
                    self.restore_last_typing_state();
                } else {
                    self.check_grammar(1);
                }
            }
        } else {
            if self.will_temp_off_engine {
                self.hook.code = Code::DoNothing;
                self.hook.ext_code = 3;
                return;
            }
            if self.space_count > 0 {
                self.hook.backspace_count = 0;
                self.hook.new_char_count = 0;
                self.hook.ext_code = 0;
                self.start_new_session();
                self.save_word_repeat(KEY_SPACE as u32, self.space_count);
                self.space_count = 0;
            } else if !self.special_char.is_empty() {
                self.save_special_char();
            }

            self.insert_state(data, is_caps);

            if !self.is_special_key(data) || self.temp_disable_key {
                if self.s.quick_telex && self.is_quick_telex_key(data) {
                    self.handle_quick_telex(data, is_caps);
                    self.update_macro_key(data, caps_bit);
                    return;
                }
                self.hook.code = Code::DoNothing;
                self.hook.backspace_count = 0;
                self.hook.new_char_count = 0;
                self.hook.ext_code = 3;
                self.insert_key(data, is_caps, true);
            } else {
                self.hook.code = Code::DoNothing;
                self.hook.ext_code = 3;
                self.handle_main_key(data, is_caps);
            }

            if !self.s.free_mark && !self.is_key_d(data) {
                if self.hook.code == Code::DoNothing {
                    self.check_grammar(-1);
                } else {
                    self.check_grammar(0);
                }
            }

            if self.hook.code == Code::Restore {
                self.insert_key(data, is_caps, true);
                self.state_index -= 1;
            }

            self.update_macro_key(data, caps_bit);

            if self.s.upper_case_first_char {
                if self.index == 1 && self.upper_case_status == 2 {
                    self.upper_case_first_character();
                }
                self.upper_case_status = 0;
            }

            // case [ ]
            if (data == KEY_LEFT_BRACKET || data == KEY_RIGHT_BRACKET)
                && (matches!(self.hook.char_data[0] as u16, KEY_LEFT_BRACKET | KEY_RIGHT_BRACKET)
                    || self.s.input_type == SIMPLE_TELEX_1
                    || self.s.input_type == SIMPLE_TELEX_2)
            {
                let processed = if self.hook.code == Code::WillProcess { self.hook.backspace_count } else { 0 };
                if self.index - processed > 0 {
                    self.index -= 1;
                    self.save_word();
                }
                self.index = 0;
                self.temp_disable_key = false;
                self.state_index = 0;
                self.raw_keys.clear();
                self.hook.ext_code = 3;
                self.special_char.push(data as u32 | caps_bit);
            }
        }
    }

    /// Keep the macro key buffer equal to what is on screen for the current word.
    /// Fix: C++ copied `new_char_count` items starting at `index - backspace_count`, which is
    /// wrong whenever the two counts differ (standalone ư, quick telex, grammar re-check, restore).
    fn update_macro_key(&mut self, data: u16, caps_bit: u32) {
        if !self.s.use_macro {
            return;
        }
        match self.hook.code {
            Code::DoNothing => self.hook.macro_key.push(data as u32 | caps_bit),
            Code::WillProcess | Code::Restore => {
                for _ in 0..self.hook.backspace_count.max(0) {
                    self.hook.macro_key.pop();
                }
                // on restore the typed key is appended to the screen after the new chars
                let shown = self.hook.new_char_count + i32::from(self.hook.code == Code::Restore);
                let start = (self.index - shown).max(0);
                for i in start..self.index {
                    let v = self.tw(i);
                    self.hook.macro_key.push(v);
                }
            }
            _ => {}
        }
    }
}
