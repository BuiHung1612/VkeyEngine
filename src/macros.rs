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
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct MacroData {
    pub text: String,
    pub content: String,
    pub content_code: Vec<u32>,
}

#[derive(Default, Clone)]
pub struct Macros {
    // Snapshots for UI edits must not copy a large dictionary while holding the hook lock.
    map: Arc<BTreeMap<Vec<u32>, Arc<MacroData>>>,
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

/// Ready-made macros added with one button (only the names not taken yet).
pub struct Pack {
    pub name: &'static str,
    pub note: &'static str,
    pub items: &'static [(&'static str, &'static str)],
}

pub static PACKS: &[Pack] = &[
    Pack {
        name: "Emoji",
        note: ":) → 😊, <3 → ❤️, (y) → 👍",
        items: &[
            (":)", "😊"),
            (":D", "😄"),
            (":(", "😢"),
            (":'(", "😭"),
            (";)", "😉"),
            (":P", "😛"),
            (":O", "😮"),
            (":*", "😘"),
            ("<3", "❤️"),
            ("(y)", "👍"),
            ("(n)", "👎"),
            ("(ok)", "👌"),
            ("(pray)", "🙏"),
            ("(fire)", "🔥"),
        ],
    },
    Pack {
        name: "Ký hiệu",
        note: "-> → →, (c) → ©, ... → … (lưu ý: gõ code cũng bị đổi, ví dụ \"->\")",
        items: &[
            ("->", "→"),
            ("<-", "←"),
            ("=>", "⇒"),
            ("(c)", "©"),
            ("(r)", "®"),
            ("(tm)", "™"),
            ("+-", "±"),
            ("...", "…"),
            ("1/2", "½"),
            ("deg", "°"),
        ],
    },
    Pack {
        name: "Viết tắt",
        note: "ko → không, dc → được, ntn → như thế nào",
        items: &[
            ("ko", "không"),
            ("dc", "được"),
            ("ntn", "như thế nào"),
            ("bt", "bình thường"),
            ("mn", "mọi người"),
            ("ae", "anh em"),
            ("cty", "công ty"),
            ("nv", "nhân viên"),
            ("kh", "khách hàng"),
            ("sdt", "số điện thoại"),
            ("tp", "thành phố"),
            ("hn", "Hà Nội"),
            ("hcm", "Hồ Chí Minh"),
            ("vn", "Việt Nam"),
        ],
    },
];

impl Macros {
    /// Binary format (NSUserDefaults "macroData"): u16 count, then for each macro:
    /// u8 text length, text bytes, u16 content length, content bytes (all little endian, UTF-8).
    pub fn load_binary(&mut self, data: &[u8], code_table: u8) {
        self.map = Arc::default();
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
            if self.add(&text, &content, code_table).is_err() {
                return;
            }
        }
    }

    pub fn save_binary(&self) -> Vec<u8> {
        let mut out = (self.map.len() as u16).to_le_bytes().to_vec();
        for m in self.map.values() {
            let t = &m.text;
            out.push(t.len() as u8);
            out.extend_from_slice(t.as_bytes());
            let c = &m.content;
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

    /// Add the macros of a pack whose names are free; returns how many were added.
    pub fn add_pack(&mut self, pack: &Pack, code_table: u8) -> Result<usize, &'static str> {
        let mut next = self.clone();
        let mut added = 0;
        for (name, content) in pack.items {
            if !next.has(name, code_table) {
                next.add(name, content, code_table)?;
                added += 1;
            }
        }
        *self = next;
        Ok(added)
    }

    pub fn has(&self, name: &str, code_table: u8) -> bool {
        self.map.contains_key(&convert(name, code_table))
    }

    /// (text, content) pairs, sorted like the original table.
    pub fn all(&self) -> Vec<(String, String)> {
        self.map.values().map(|m| (m.text.clone(), m.content.clone())).collect()
    }

    pub fn add(&mut self, text: &str, content: &str, code_table: u8) -> Result<(), &'static str> {
        if text.is_empty() || text.len() > u8::MAX as usize {
            return Err("Tên gõ tắt phải có từ 1 đến 255 byte UTF-8.");
        }
        if content.len() > u16::MAX as usize {
            return Err("Nội dung gõ tắt không được vượt quá 65.535 byte UTF-8.");
        }
        let key = convert(text, code_table);
        if self.map.len() == u16::MAX as usize && !self.map.contains_key(&key) {
            return Err("Danh sách không được vượt quá 65.535 gõ tắt.");
        }
        let content_code = convert(content, code_table);
        let entry = Arc::make_mut(&mut self.map).entry(key).or_insert_with(|| Arc::new(MacroData {
            text: text.to_string(),
            content: String::new(),
            content_code: Vec::new(),
        }));
        let entry = Arc::make_mut(entry);
        entry.content = content.to_string();
        entry.content_code = content_code;
        Ok(())
    }

    pub fn delete(&mut self, text: &str, code_table: u8) -> bool {
        Arc::make_mut(&mut self.map).remove(&convert(text, code_table)).is_some()
    }

    pub fn on_code_table_change(&mut self, code_table: u8) {
        let old = std::mem::take(&mut self.map);
        for m in old.values() {
            self.add(&m.text, &m.content, code_table).expect("validated macro");
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

    pub fn import_text(&mut self, text: &str, append: bool, code_table: u8) -> Result<(), &'static str> {
        let mut imported = Self::default();
        let mut header = false;
        for line in text.trim_start_matches('\u{feff}').lines() {
            if line.starts_with(';') && !line.contains(':') {
                header |= line.contains("Macro") && line.contains("version=1");
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }
            let Some(pos) = line.find(':') else { return Err("File gõ tắt không hợp lệ.") };
            let (mut name, mut content) = (line[..pos].to_string(), line[pos + 1..].to_string());
            // names starting with ':' (e.g. "::)")
            while name.is_empty() && !content.is_empty() {
                let Some(p) = content.find(':') else { break };
                name = format!(":{}", &content[..p]);
                content = content[p + 1..].to_string();
            }
            if name.is_empty() {
                return Err("File gõ tắt có tên trống.");
            }
            if !imported.has(&name, code_table) {
                imported.add(&name, &content, code_table)?;
            }
        }
        if imported.map.is_empty() && !header {
            return Err("File không chứa dữ liệu gõ tắt hợp lệ.");
        }
        if append {
            let extra = imported.map.keys().filter(|k| !self.map.contains_key(*k)).count();
            if self.map.len() + extra > u16::MAX as usize {
                return Err("Danh sách không được vượt quá 65.535 gõ tắt.");
            }
            let map = Arc::make_mut(&mut self.map);
            for (key, value) in std::mem::take(Arc::make_mut(&mut imported.map)) {
                map.entry(key).or_insert(value);
            }
        } else {
            *self = imported;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_import_keeps_existing_macros() {
        let mut macros = Macros::default();
        macros.add("vn", "Việt Nam", 0).unwrap();
        let before = macros.save_binary();
        for append in [false, true] {
            for invalid in ["", "not a macro file", "new:valid\ninvalid line", ":empty name"] {
                assert!(macros.import_text(invalid, append, 0).is_err());
                assert_eq!(macros.save_binary(), before);
            }
            let oversized = format!("new:valid\nlong:{}", "x".repeat(65_536));
            assert!(macros.import_text(&oversized, append, 0).is_err());
            assert_eq!(macros.save_binary(), before);
        }
    }

    #[test]
    fn valid_import_preserves_existing_names_and_accepts_empty_exports() {
        let mut macros = Macros::default();
        macros.add("vn", "Việt Nam", 0).unwrap();
        macros.import_text("\u{feff}; comment\r\nvn:other\r\nbtw:by the way\r\n", true, 0).unwrap();
        assert!(macros.all().contains(&("vn".into(), "Việt Nam".into())));
        assert!(macros.has("btw", 0));
        macros.import_text(&Macros::default().export_text(), false, 0).unwrap();
        assert!(macros.all().is_empty());
    }

    #[test]
    fn exported_punctuation_macros_roundtrip() {
        let mut macros = Macros::default();
        for pack in PACKS {
            macros.add_pack(pack, 0).unwrap();
        }
        macros.add(";date", "today", 0).unwrap();
        let mut loaded = Macros::default();
        loaded.import_text(&macros.export_text(), false, 0).unwrap();
        assert_eq!(loaded.all(), macros.all());
    }

    #[test]
    fn macro_limits_reject_changes_without_truncating_utf8() {
        let mut macros = Macros::default();
        let name = format!("{}a", "á".repeat(127));
        let content = format!("{}a", "á".repeat(32_767));
        macros.add(&name, &content, 0).unwrap();
        let before = macros.save_binary();
        assert!(macros.add(&format!("{name}a"), "x", 0).is_err());
        assert!(macros.add(&name, &format!("{content}a"), 0).is_err());
        assert!(macros.add("", "x", 0).is_err());
        assert_eq!(macros.save_binary(), before);
        let mut loaded = Macros::default();
        loaded.load_binary(&before, 0);
        assert_eq!(loaded.all(), macros.all());
    }

    #[test]
    fn macro_count_cannot_overflow_binary_header() {
        let mut macros = Macros::default();
        for i in 0..u16::MAX {
            macros.add(&format!("m{i}"), "x", 0).unwrap();
        }
        assert!(macros.add("extra", "x", 0).is_err());
        macros.add("m0", "updated", 0).unwrap();
        assert!(macros.import_text("extra:x", true, 0).is_err());
        assert_eq!(&macros.save_binary()[..2], &u16::MAX.to_le_bytes());
    }

    #[test]
    fn editing_a_snapshot_does_not_change_live_macros() {
        let mut live = Macros::default();
        live.add("vn", "Việt Nam", 0).unwrap();
        let mut snapshot = live.clone();
        snapshot.add("vn", "changed", 0).unwrap();
        snapshot.add("new", "new", 0).unwrap();
        assert_eq!(live.all(), vec![("vn".into(), "Việt Nam".into())]);
    }
}
