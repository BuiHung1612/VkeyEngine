//! What to send to the screen for one engine result. Shared by the macOS event tap, the
//! Windows hook and the test simulator, so the three cannot drift apart again (before, each
//! had its own copy, and fixes made to one were missing from the others).

use crate::engine::{encode_output, is_double_code, Code, Engine, MAX_BUFF};

/// Some apps truncate long Unicode strings sent in one event.
const MAX_UNITS_PER_EVENT: usize = 16;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Step {
    /// one Backspace key press
    Delete,
    /// one Shift + Left key press
    SelectLeft,
    /// characters, sent as Unicode key events
    Text(Vec<u16>),
    /// the key just typed, sent again as a key press with the modifiers it had
    Key,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BrowserFix {
    #[default]
    Off,
    /// type an invisible character first, so the address bar's autocomplete closes
    EmptyChar,
    /// select the last character instead (Chromium)
    SelectLeft,
}

/// The key that was typed, when the engine gives it back (restore, macro trigger).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typed {
    /// send this character
    Char(u16),
    /// send the key again; it types one character
    Printable,
    /// send the key again; it types nothing (Tab, arrows...)
    Control,
}

#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub browser_fix: BrowserFix,
    /// select instead of deleting (Spotlight ignores synthetic backspaces)
    pub select_instead_of_delete: bool,
    /// one Backspace deletes a base letter together with its combining mark (Apple apps,
    /// Chromium): a 2-unit Unicode Compound / CP 1258 character needs one Backspace there
    pub grapheme_delete: bool,
    /// one event per character
    pub step_by_step: bool,
    /// the invisible character of `BrowserFix::EmptyChar`
    pub empty_char: u16,
}

impl Default for Ctx {
    fn default() -> Ctx {
        Ctx {
            browser_fix: BrowserFix::Off,
            select_instead_of_delete: false,
            grapheme_delete: false,
            step_by_step: false,
            empty_char: 0x202F,
        }
    }
}

/// Screen side state: UTF-16 length of each character of the current word. Only used by the
/// 2-unit code tables (VNI Windows, Unicode Compound, CP 1258), to know how many key presses
/// delete one character.
#[derive(Default)]
pub struct Output {
    sync_key: Vec<u8>,
}

impl Output {
    pub fn clear(&mut self) {
        self.sync_key.clear();
    }

    /// Steps for the engine's last result. `DoNothing`: the caller passes the key through
    /// after these steps (at most one extra Backspace).
    pub fn plan(&mut self, e: &mut Engine, ctx: &Ctx, typed: Typed) -> Vec<Step> {
        let mut steps = Vec::new();
        let table = e.s.code_table;
        let double = is_double_code(table);
        match e.hook.code {
            Code::DoNothing => {
                if double {
                    match e.hook.ext_code {
                        1 => self.sync_key.clear(),
                        2 => {
                            // the user's Backspace deletes one unit, this one the second
                            if let Some(l) = self.sync_key.pop() {
                                if two_presses(l, table, ctx) {
                                    steps.push(Step::Delete);
                                }
                            }
                        }
                        3 => self.sync_key.push(1),
                        _ => {}
                    }
                }
            }
            Code::WillProcess | Code::Restore | Code::RestoreAndStartNewSession => {
                let mut bs = e.hook.backspace_count;
                if e.hook.ext_code != 4 {
                    match ctx.browser_fix {
                        BrowserFix::Off => {}
                        BrowserFix::EmptyChar => {
                            self.empty_char(double, ctx, &mut steps);
                            bs += 1;
                        }
                        BrowserFix::SelectLeft if bs > 0 => {
                            // the selection is replaced by what comes next, or deleted by
                            // the first Backspace (which then deletes nothing else)
                            self.select_char(double, table, ctx, &mut steps);
                            if bs > 1 {
                                steps.push(Step::Delete);
                            }
                            bs -= 1;
                        }
                        BrowserFix::SelectLeft => {}
                    }
                }
                if bs > 0 && ctx.select_instead_of_delete {
                    for _ in 0..bs {
                        self.select_char(double, table, ctx, &mut steps);
                    }
                    bs = 0;
                }
                if bs > 0 && bs < MAX_BUFF as i32 {
                    for _ in 0..bs {
                        self.delete_char(double, table, ctx, &mut steps);
                    }
                }
                let h = &e.hook;
                let mut text = self.text(
                    (0..h.new_char_count.clamp(0, MAX_BUFF as i32) as usize).rev().map(|i| h.char_data[i]),
                    table, ctx, &mut steps,
                );
                let restore = matches!(h.code, Code::Restore | Code::RestoreAndStartNewSession);
                if h.code == Code::RestoreAndStartNewSession {
                    e.start_new_session();
                }
                if restore {
                    self.typed(typed, double, &mut text, ctx, &mut steps);
                } else {
                    flush_text(&mut text, &mut steps);
                }
            }
            Code::ReplaceMacro => {
                let mut bs = e.hook.backspace_count;
                if ctx.browser_fix != BrowserFix::Off {
                    self.empty_char(double, ctx, &mut steps);
                    bs += 1;
                }
                for _ in 0..bs.max(0) {
                    self.delete_char(double, table, ctx, &mut steps);
                }
                let mut text = self.text(e.hook.macro_data.iter().copied(), table, ctx, &mut steps);
                // the key that triggered the macro (space, punctuation, Enter)
                self.typed(typed, double, &mut text, ctx, &mut steps);
            }
        }
        steps
    }

    /// Encode into event chunks without allocating a buffer for every character.
    fn text(&mut self, codes: impl Iterator<Item = u32>, table: u8, ctx: &Ctx, steps: &mut Vec<Step>) -> Vec<u16> {
        let mut text = Vec::new();
        let mut units = Vec::with_capacity(2);
        for code in codes {
            units.clear();
            let len = encode_output(code, table, &mut units);
            if is_double_code(table) {
                self.sync_key.push(len);
            }
            push_text(&units, &mut text, ctx, steps);
        }
        text
    }

    /// Output text followed by the typed key.
    fn typed(&mut self, typed: Typed, double: bool, text: &mut Vec<u16>, ctx: &Ctx, steps: &mut Vec<Step>) {
        if let Typed::Char(c) = typed {
            push_text(&[c], text, ctx, steps);
        }
        flush_text(text, steps);
        match typed {
            Typed::Char(_) | Typed::Printable if double => self.sync_key.push(1),
            Typed::Control => {
                // Tab, Enter, arrows: the caret left the word
                self.sync_key.clear();
            }
            _ => {}
        }
        if !matches!(typed, Typed::Char(_)) {
            steps.push(Step::Key);
        }
    }

    fn delete_char(&mut self, double: bool, table: u8, ctx: &Ctx, steps: &mut Vec<Step>) {
        steps.push(Step::Delete);
        if double {
            if self.sync_key.last().is_some_and(|&l| two_presses(l, table, ctx)) {
                steps.push(Step::Delete);
            }
            self.sync_key.pop();
        }
    }

    fn select_char(&mut self, double: bool, table: u8, ctx: &Ctx, steps: &mut Vec<Step>) {
        steps.push(Step::SelectLeft);
        if double {
            if self.sync_key.last().is_some_and(|&l| two_presses(l, table, ctx)) {
                steps.push(Step::SelectLeft);
            }
            self.sync_key.pop();
        }
    }

    fn empty_char(&mut self, double: bool, ctx: &Ctx, steps: &mut Vec<Step>) {
        if double {
            self.sync_key.push(1);
        }
        steps.push(Step::Text(vec![ctx.empty_char]));
    }
}

/// A character of `len` UTF-16 units takes two Backspace / Shift+Left presses.
fn two_presses(len: u8, table: u8, ctx: &Ctx) -> bool {
    len > 1 && (table == 2 || !ctx.grapheme_delete)
}

/// Append a whole character, flushing before it would cross an event boundary.
fn push_text(units: &[u16], text: &mut Vec<u16>, ctx: &Ctx, steps: &mut Vec<Step>) {
    if ctx.step_by_step || text.len() + units.len() > MAX_UNITS_PER_EVENT {
        flush_text(text, steps);
    }
    if text.is_empty() {
        text.reserve_exact(if ctx.step_by_step { units.len() } else { MAX_UNITS_PER_EVENT });
    }
    text.extend_from_slice(units);
}

fn flush_text(text: &mut Vec<u16>, steps: &mut Vec<Step>) {
    if !text.is_empty() {
        steps.push(Step::Text(std::mem::take(text)));
    }
}
