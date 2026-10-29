//! Macro (text expansion). Port of engine/Macro.cpp.
//!
//! Fixes vs C++:
//! - keys were only converted with the code table active at load time, so after switching
//!   the code table macros containing Vietnamese characters (e.g. "ưd") never matched.
//! - auto-caps on a non-Vietnamese character (e.g. "€", "ñ") zeroed it (`code &= CAPS_MASK`).
//! - characters outside the BMP (emoji) were truncated to 16 bits.

use crate::engine::char_code;
use crate::tables::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct MacroData {
    pub text: String,
    pub content: String,
    pub content_code: Vec<u32>,
}

#[derive(Default)]
pub struct Macros {
    map: BTreeMap<Vec<u32>, MacroData>,
}

fn convert(s: &str, code_table: u8) -> Vec<u32> {
    s.chars()
        .map(|ch| {
            let t = ch as u32;
            if let Some(k) = char_to_key(t) {
                return k;
            }
            if t <= 0xFFFF {
                for (key, values) in CODE_TABLES[0].iter() {
                    if let Some(pos) = values.iter().position(|&v| v as u32 == t) {
                        let target = code_entry(code_table as usize, *key).map_or(0, |v| v[pos]);
                        return target as u32 | CHAR_CODE_MASK;
                    }
                }
            }
            t | PURE_CHARACTER_MASK
        })
        .collect()
}

/// Change case of one converted character; returns true if it changed.
fn modify_case(code: &mut u32, upper: bool, code_table: u8) -> bool {
    let before = *code;
    if *code & CHAR_CODE_MASK == 0 {
        if *code & PURE_CHARACTER_MASK != 0 {
            let Some(c) = char::from_u32(*code & !PURE_CHARACTER_MASK) else { return false };
            let m: Vec<char> = if upper { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
            if let [n] = m[..] {
                *code = n as u32 | PURE_CHARACTER_MASK;
            }
        } else if upper {
            *code |= CAPS_MASK;
        } else {
            *code &= !CAPS_MASK;
        }
        return *code != before;
    }
    for (_, values) in CODE_TABLES[code_table.min(4) as usize].iter() {
        if let Some(mut k) = values.iter().position(|&v| v == *code as u16) {
            if k % 2 == 0 && !upper {
                k += 1;
            } else if k % 2 != 0 && upper {
                k -= 1;
            }
            *code = values[k] as u32 | CHAR_CODE_MASK;
            return *code != before;
        }
    }
    false
}

impl Macros {
    /// Binary format (NSUserDefaults "macroData"): u16 count, then for each macro:
    /// u8 text length, text bytes, u16 content length, content bytes (all little endian, UTF-8).
    pub fn load_binary(&mut self, data: &[u8], code_table: u8) {
        self.map.clear();
        let mut cur = 0usize;
        let take = |cur: &mut usize, n: usize| -> Option<&[u8]> {
            let s = data.get(*cur..*cur + n)?;
            *cur += n;
            Some(s)
        };
        let Some(c) = take(&mut cur, 2) else { return };
        let count = u16::from_le_bytes([c[0], c[1]]);
        for _ in 0..count {
            let Some(&[tl]) = take(&mut cur, 1) else { return };
            let Some(text) = take(&mut cur, tl as usize).map(|b| String::from_utf8_lossy(b).into_owned()) else {
                return;
            };
            let Some(cl) = take(&mut cur, 2).map(|b| u16::from_le_bytes([b[0], b[1]])) else { return };
            let Some(content) = take(&mut cur, cl as usize).map(|b| String::from_utf8_lossy(b).into_owned()) else {
                return;
            };
            self.add(&text, &content, code_table);
        }
    }

    pub fn save_binary(&self) -> Vec<u8> {
        let mut out = (self.map.len() as u16).to_le_bytes().to_vec();
        for m in self.map.values() {
            let t = truncate_utf8(&m.text, u8::MAX as usize);
            out.push(t.len() as u8);
            out.extend_from_slice(t.as_bytes());
            let c = truncate_utf8(&m.content, u16::MAX as usize);
            out.extend_from_slice(&(c.len() as u16).to_le_bytes());
            out.extend_from_slice(c.as_bytes());
        }
        out
    }

    /// Look up the macro for the typed key (engine data). `key` is converted in place.
    pub fn find(&self, key: &mut [u32], code_table: u8, auto_caps: bool) -> Option<Vec<u32>> {
        for k in key.iter_mut() {
            *k = char_code(*k, code_table);
        }
        if let Some(m) = self.map.get(key) {
            return Some(m.content_code.clone());
        }
        if !auto_caps || key.is_empty() {
            return None;
        }
        let mut all_caps = false;
        if key.len() > 1 && modify_case(&mut key[1], false, code_table) {
            all_caps = true;
            for k in key[2..].iter_mut() {
                modify_case(k, false, code_table);
            }
        }
        if !modify_case(&mut key[0], false, code_table) {
            return None;
        }
        let m = self.map.get(key)?;
        let mut content = m.content_code.clone();
        for (c, code) in content.iter_mut().enumerate() {
            if c == 0 || all_caps {
                let ch = key_to_char(*code);
                if ch != 0 {
                    if let Some(k) = char_to_key((ch as u8).to_ascii_uppercase() as u32) {
                        *code = k;
                    }
                    continue;
                }
                modify_case(code, true, code_table);
            }
        }
        Some(content)
    }

    /// Name (text) of the macro whose key is `key`, as `find` left it.
    pub fn text_of(&self, key: &[u32]) -> Option<&str> {
        self.map.get(key).map(|m| m.text.as_str())
    }

    pub fn has(&self, name: &str, code_table: u8) -> bool {
        self.map.contains_key(&convert(name, code_table))
    }

    /// (text, content) pairs, sorted like the original table.
    pub fn all(&self) -> Vec<(String, String)> {
        self.map.values().map(|m| (m.text.clone(), m.content.clone())).collect()
    }

    pub fn add(&mut self, text: &str, content: &str, code_table: u8) {
        let key = convert(text, code_table);
        let content_code = convert(content, code_table);
        let entry = self.map.entry(key).or_insert_with(|| MacroData {
            text: text.to_string(),
            content: String::new(),
            content_code: Vec::new(),
        });
        entry.content = content.to_string();
        entry.content_code = content_code;
    }

    pub fn delete(&mut self, text: &str, code_table: u8) -> bool {
        self.map.remove(&convert(text, code_table)).is_some()
    }

    pub fn on_code_table_change(&mut self, code_table: u8) {
        let old = std::mem::take(&mut self.map);
        for m in old.into_values() {
            self.add(&m.text, &m.content, code_table);
        }
    }

    /// UniKey compatible text format.
    pub fn export_text(&self) -> String {
        let mut s = String::from(";Compatible VKeyEngine Macro Data file for UniKey*** version=1 ***\n");
        for m in self.map.values() {
            s.push_str(&format!("{}:{}\n", m.text, m.content));
        }
        s
    }

    pub fn import_text(&mut self, text: &str, append: bool, code_table: u8) {
        if !append {
            self.map.clear();
        }
        for line in text.lines().skip(1) {
            let line = line.trim_end_matches('\r');
            let Some(pos) = line.find(':') else { continue };
            let (mut name, mut content) = (line[..pos].to_string(), line[pos + 1..].to_string());
            // names starting with ':' (e.g. "::)")
            while name.is_empty() && !content.is_empty() {
                let Some(p) = content.find(':') else { break };
                name = format!(":{}", &content[..p]);
                content = content[p + 1..].to_string();
            }
            if !name.is_empty() && !self.has(&name, code_table) {
                self.add(&name, &content, code_table);
            }
        }
    }
}

fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
