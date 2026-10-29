//! Remember input language / code table per application. Port of engine/SmartSwitchKey.cpp.

use std::collections::BTreeMap;

#[derive(Default)]
pub struct SmartSwitch {
    data: BTreeMap<String, i8>,
}

impl SmartSwitch {
    /// Binary format (NSUserDefaults "smartSwitchKey"): u16 count, then u8 id length, id bytes, i8 value.
    pub fn load_binary(&mut self, data: &[u8]) {
        self.data.clear();
        if data.len() < 2 {
            return;
        }
        let count = u16::from_le_bytes([data[0], data[1]]);
        let mut cur = 2usize;
        for _ in 0..count {
            let Some(&len) = data.get(cur) else { return };
            cur += 1;
            let (Some(id), Some(&value)) = (data.get(cur..cur + len as usize), data.get(cur + len as usize)) else {
                return;
            };
            self.data.insert(String::from_utf8_lossy(id).into_owned(), value as i8);
            cur += len as usize + 1;
        }
    }

    pub fn save_binary(&self) -> Vec<u8> {
        let mut out = (self.data.len() as u16).to_le_bytes().to_vec();
        for (id, v) in &self.data {
            let id = &id.as_bytes()[..id.len().min(255)];
            out.push(id.len() as u8);
            out.extend_from_slice(id);
            out.push(*v as u8);
        }
        out
    }

    /// Stored status for `app` (bit 0: language, bits 1..: code table), or None if the
    /// app is new — in that case `current` is stored for it.
    pub fn get(&mut self, app: &str, current: i8) -> Option<i8> {
        if let Some(&v) = self.data.get(app) {
            return Some(v);
        }
        self.data.insert(app.to_string(), current);
        None
    }

    pub fn set(&mut self, app: &str, value: i8) {
        self.data.insert(app.to_string(), value);
    }
}
