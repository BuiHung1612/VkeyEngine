//! Test helper: applies engine output to a fake screen like the macOS event tap does.

use crate::engine::*;
use crate::tables::*;

/// Feeds `input` to the engine and applies its output to a fake screen, the same way the
/// macOS event tap does. `\x08` is backspace, `\n` is return.
pub fn type_with(e: &mut Engine, input: &str) -> String {
    // one entry per character: legacy code tables use 2 UTF-16 units for some characters
    let mut screen: Vec<Vec<u16>> = Vec::new();
    for ch in input.chars() {
        let (key, shift) = match ch {
            '\x08' => (KEY_DELETE, false),
            '\n' => (KEY_RETURN, false),
            _ => {
                let k = char_to_key(ch as u32).unwrap_or_else(|| panic!("no key for {ch:?}"));
                ((k & CHAR_MASK) as u16, k & CAPS_MASK != 0)
            }
        };
        let typed = if shift { key as u32 | CAPS_MASK } else { key as u32 };
        let table = e.s.code_table;
        if e.s.language == 0 {
            e.english_mode(KeyState::KeyDown, key, shift, false);
        } else {
            e.handle_event(KeyEvent::Keyboard, KeyState::KeyDown, key, shift as u8, false);
        }
        let h = &e.hook;
        let pass_through = |screen: &mut Vec<Vec<u16>>| {
            if key == KEY_DELETE {
                screen.pop();
            } else {
                screen.push(vec![key_to_char(typed)]);
            }
        };
        let put = |screen: &mut Vec<Vec<u16>>, c: u32| {
            let mut v = Vec::new();
            encode_output(c, table, &mut v);
            screen.push(v);
        };
        match h.code {
            Code::DoNothing => pass_through(&mut screen),
            Code::ReplaceMacro => {
                for _ in 0..h.backspace_count {
                    screen.pop();
                }
                for &c in &h.macro_data {
                    put(&mut screen, c);
                }
                screen.push(vec![key_to_char(typed)]);
            }
            _ => {
                for _ in 0..h.backspace_count {
                    screen.pop();
                }
                for i in (0..h.new_char_count.max(0) as usize).rev() {
                    put(&mut screen, h.char_data[i]);
                }
                if matches!(h.code, Code::Restore | Code::RestoreAndStartNewSession) {
                    screen.push(vec![key_to_char(typed)]);
                }
                if h.code == Code::RestoreAndStartNewSession {
                    e.start_new_session();
                }
            }
        }
    }
    String::from_utf16(&screen.concat()).unwrap()
}

