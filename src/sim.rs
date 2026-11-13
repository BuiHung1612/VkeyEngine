//! Test helper: applies engine output to a fake screen through the same `output` code the
//! macOS tap and the Windows hook use. The screen holds UTF-16 units and one Backspace
//! deletes one unit, so a wrong count of deletes for 2-unit characters shows up in tests.

use crate::engine::*;
use crate::output::{Ctx, Output, Step, Typed};
use crate::tables::*;

/// Feeds `input` to the engine and applies its output to a fake screen. `\x08` is
/// backspace, `\n` is return, `\x01` a mouse click as the engine sees it, `\x02` the caret
/// moving away (focus change: what the platform layers do on a click or app switch).
pub fn type_with(e: &mut Engine, input: &str) -> String {
    type_with_ctx(e, input, &Ctx::default())
}

/// `type_with` for an app that needs a workaround (`ctx`). Shift+Left selects units at the
/// end of the screen; the next Backspace deletes the selection, the next text replaces it.
pub fn type_with_ctx(e: &mut Engine, input: &str, ctx: &Ctx) -> String {
    let mut out = Output::default();
    let mut screen: Vec<u16> = Vec::new();
    let mut selected = 0usize;
    for ch in input.chars() {
        if ch == '\x01' {
            e.handle_event(KeyEvent::Mouse, KeyState::MouseDown, 0, 0, false);
            assert_eq!(e.hook.code, Code::DoNothing, "a click must not rewrite text at the old caret");
            out.clear();
            continue;
        }
        if ch == '\x02' {
            e.reset();
            out.clear();
            continue;
        }
        let (key, shift) = match ch {
            '\x08' => (KEY_DELETE, false),
            '\n' => (KEY_RETURN, false),
            _ => {
                let k = char_to_key(ch as u32).unwrap_or_else(|| panic!("no key for {ch:?}"));
                ((k & CHAR_MASK) as u16, k & CAPS_MASK != 0)
            }
        };
        let typed_char = key_to_char(if shift { key as u32 | CAPS_MASK } else { key as u32 });
        let typed = if typed_char != 0 { Typed::Printable } else { Typed::Control };
        if e.s.language == 0 {
            e.english_mode(KeyState::KeyDown, key, shift, false);
        } else {
            e.handle_event(KeyEvent::Keyboard, KeyState::KeyDown, key, shift as u8, false);
        }
        let pass = e.hook.code == Code::DoNothing;
        let steps = out.plan(e, ctx, typed);
        // a key press: Backspace deletes the selection or one unit, a character replaces it
        let press = |screen: &mut Vec<u16>, selected: &mut usize, text: &[u16]| {
            if *selected > 0 {
                screen.truncate(screen.len() - std::mem::take(selected));
                if text.is_empty() {
                    return;
                }
            } else if text.is_empty() {
                screen.pop();
                return;
            }
            screen.extend_from_slice(text);
        };
        let typed_text: &[u16] = if key == KEY_DELETE || typed_char == 0 { &[] } else { std::slice::from_ref(&typed_char) };
        for s in steps {
            match s {
                Step::Delete => press(&mut screen, &mut selected, &[]),
                Step::Text(u) => press(&mut screen, &mut selected, &u),
                Step::Key if typed_text.is_empty() && key != KEY_DELETE => {} // a control key
                Step::Key => press(&mut screen, &mut selected, typed_text),
                Step::SelectLeft => selected = (selected + 1).min(screen.len()),
            }
        }
        if pass && !(typed_text.is_empty() && key != KEY_DELETE) {
            press(&mut screen, &mut selected, typed_text);
        }
    }
    String::from_utf16(&screen).unwrap()
}
