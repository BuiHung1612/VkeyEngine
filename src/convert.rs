//! Code table conversion tool. Port of engine/ConvertTool.cpp.
//!
//! Fixes vs C++:
//! - with no case option selected, everything was lower-cased ("Hà Nội" -> "hà nội");
//!   case is now preserved (TCVN3 marked letters have no upper case, they stay lower).
//! - the last character of the text was emitted without encoding, so a trailing
//!   Vietnamese letter came out as garbage for VNI / Unicode Compound / CP1258.
//! - "remove mark" lost the case of the letter ("Á" -> "a").

use crate::tables::*;

#[derive(Clone, Debug, Default)]
pub struct ConvertOptions {
    pub to_all_caps: bool,
    pub to_all_non_caps: bool,
    pub caps_first_letter: bool,
    pub caps_each_word: bool,
    pub remove_mark: bool,
    pub from_code: u8,
    pub to_code: u8,
}

fn find_key_code(ch: u32, table: u8) -> Option<(u32, usize)> {
    CODE_TABLES[table.min(4) as usize]
        .iter()
        .find_map(|(key, values)| values.iter().position(|&v| v as u32 == ch).map(|k| (*key, k)))
}

fn compound_mark_index(mark: u32) -> u32 {
    UNICODE_COMPOUND_MARK
        .iter()
        .position(|&m| m as u32 == mark)
        .map_or(0, |i| ((i + 1) as u32) << 13)
}

fn push_char(out: &mut String, c: u32) {
    if let Some(ch) = char::from_u32(c) {
        out.push(ch);
    }
}

pub fn convert(src: &str, o: &ConvertOptions) -> String {
    let data: Vec<u32> = src.chars().map(|c| c as u32).collect();
    let caps_mode = o.caps_first_letter || o.caps_each_word;
    let mut should_upper = caps_mode && !o.to_all_non_caps;
    let mut has_break = false;
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < data.len() {
        // a Vietnamese character may span 2 units in VNI / CP1258 / Unicode Compound
        let mut found = None;
        if i + 1 < data.len() {
            let (t, p) = match o.from_code {
                2 | 4 => ((data[i] & 0xFF) | ((data[i + 1] & 0xFF) << 8), 1),
                3 => match compound_mark_index(data[i + 1]) {
                    0 => (data[i], 0),
                    m => (data[i] | m, 1),
                },
                _ => (data[i], 0),
            };
            if data[i] <= 0xFF || o.from_code == 0 || o.from_code == 3 {
                found = find_key_code(t, o.from_code).map(|f| (f, p));
            }
        }
        if found.is_none() {
            found = find_key_code(data[i], o.from_code).map(|f| (f, 0));
        }

        if let Some(((key, mut k), p)) = found {
            i += p + 1;
            let from = code_entry(o.from_code as usize, key).unwrap_or(&[]);
            if k % 2 == 0 && from.get(k + 1) == from.get(k) {
                k += 1; // TCVN3 has no upper case marked letters
            }
            let upper = o.to_all_caps || should_upper || (k % 2 == 0 && !o.to_all_non_caps && !caps_mode);
            let k = if upper { k & !1 } else { k | 1 };
            let to = code_entry(o.to_code as usize, key).unwrap_or(&[]);
            let mut target = to.get(k).copied().unwrap_or(0) as u32;
            if o.remove_mark {
                target = key_to_char(key & 0xFF) as u32;
                if upper {
                    target = (target as u8).to_ascii_uppercase() as u32;
                }
            }
            match o.to_code {
                2 | 4 if target >> 8 > 32 => {
                    push_char(&mut out, target & 0xFF);
                    push_char(&mut out, target >> 8);
                }
                2 | 4 => push_char(&mut out, target & 0xFF),
                3 if target >> 13 > 0 => {
                    push_char(&mut out, target & 0x1FFF);
                    push_char(&mut out, UNICODE_COMPOUND_MARK[(target >> 13) as usize - 1] as u32);
                }
                _ => push_char(&mut out, target),
            }
            should_upper = false;
            has_break = false;
            continue;
        }

        let c = data[i];
        let ch = char::from_u32(c).unwrap_or('\u{FFFD}');
        if o.to_all_caps || should_upper {
            out.extend(ch.to_uppercase());
        } else if o.to_all_non_caps || caps_mode {
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
        if ch == '\n' || (has_break && ch == ' ') {
            if caps_mode {
                should_upper = true;
            }
        } else if ch == ' ' && o.caps_each_word {
            should_upper = true;
        } else if matches!(ch, '.' | '?' | '!') {
            has_break = true;
        } else {
            should_upper = false;
            has_break = false;
        }
        i += 1;
    }
    out
}
