//! Key codes (macOS virtual key codes) and Vietnamese lookup tables.
//! Port of engine/DataType.h, engine/platforms/mac.h and engine/Vietnamese.cpp.

pub const KEY_ESC: u16 = 53;
pub const KEY_DELETE: u16 = 51;
pub const KEY_TAB: u16 = 48;
pub const KEY_ENTER: u16 = 76;
pub const KEY_RETURN: u16 = 36;
pub const KEY_SPACE: u16 = 49;
pub const KEY_LEFT: u16 = 123;
pub const KEY_RIGHT: u16 = 124;
pub const KEY_DOWN: u16 = 125;
pub const KEY_UP: u16 = 126;

pub const KEY_A: u16 = 0;
pub const KEY_B: u16 = 11;
pub const KEY_C: u16 = 8;
pub const KEY_D: u16 = 2;
pub const KEY_E: u16 = 14;
pub const KEY_F: u16 = 3;
pub const KEY_G: u16 = 5;
pub const KEY_H: u16 = 4;
pub const KEY_I: u16 = 34;
pub const KEY_J: u16 = 38;
pub const KEY_K: u16 = 40;
pub const KEY_L: u16 = 37;
pub const KEY_M: u16 = 46;
pub const KEY_N: u16 = 45;
pub const KEY_O: u16 = 31;
pub const KEY_P: u16 = 35;
pub const KEY_Q: u16 = 12;
pub const KEY_R: u16 = 15;
pub const KEY_S: u16 = 1;
pub const KEY_T: u16 = 17;
pub const KEY_U: u16 = 32;
pub const KEY_V: u16 = 9;
pub const KEY_W: u16 = 13;
pub const KEY_X: u16 = 7;
pub const KEY_Y: u16 = 16;
pub const KEY_Z: u16 = 6;

pub const KEY_1: u16 = 18;
pub const KEY_2: u16 = 19;
pub const KEY_3: u16 = 20;
pub const KEY_4: u16 = 21;
pub const KEY_5: u16 = 23;
pub const KEY_6: u16 = 22;
pub const KEY_7: u16 = 26;
pub const KEY_8: u16 = 28;
pub const KEY_9: u16 = 25;
pub const KEY_0: u16 = 29;

pub const KEY_LEFT_BRACKET: u16 = 33;
pub const KEY_RIGHT_BRACKET: u16 = 30;
pub const KEY_DOT: u16 = 47;
pub const KEY_BACKQUOTE: u16 = 50;
pub const KEY_MINUS: u16 = 27;
pub const KEY_EQUALS: u16 = 24;
pub const KEY_BACK_SLASH: u16 = 42;
pub const KEY_SEMICOLON: u16 = 41;
pub const KEY_QUOTE: u16 = 39;
pub const KEY_COMMA: u16 = 43;
pub const KEY_SLASH: u16 = 44;

// Engine character bit layout
pub const CAPS_MASK: u32 = 0x10000;
pub const TONE_MASK: u32 = 0x20000;
pub const TONEW_MASK: u32 = 0x40000;
pub const MARK1_MASK: u32 = 0x80000; // sắc
pub const MARK2_MASK: u32 = 0x100000; // huyền
pub const MARK3_MASK: u32 = 0x200000; // hỏi
pub const MARK4_MASK: u32 = 0x400000; // ngã
pub const MARK5_MASK: u32 = 0x800000; // nặng
pub const MARK_MASK: u32 = 0xF80000;
pub const CHAR_MASK: u32 = 0xFFFF;
pub const STANDALONE_MASK: u32 = 0x1000000;
pub const CHAR_CODE_MASK: u32 = 0x2000000;
pub const PURE_CHARACTER_MASK: u32 = 0x80000000;
pub const END_CONSONANT_MASK: u16 = 0x4000;
pub const CONSONANT_ALLOW_MASK: u16 = 0x8000;

const G_END: u16 = KEY_G | END_CONSONANT_MASK;
const H_END: u16 = KEY_H | END_CONSONANT_MASK;
const K_END: u16 = KEY_K | END_CONSONANT_MASK;

pub fn is_consonant(k: u16) -> bool {
    !matches!(k, KEY_A | KEY_E | KEY_U | KEY_Y | KEY_I | KEY_O)
}

pub fn is_number_key(k: u16) -> bool {
    matches!(k, KEY_1 | KEY_2 | KEY_3 | KEY_4 | KEY_5 | KEY_6 | KEY_7 | KEY_8 | KEY_9 | KEY_0)
}

pub type Charset = &'static [&'static [u16]];

pub fn vowel(key: u16) -> Charset {
    match key {
        KEY_A => &[
            &[KEY_A, KEY_N, KEY_G], &[KEY_A, G_END], &[KEY_A, KEY_N], &[KEY_A, KEY_M], &[KEY_A, KEY_U],
            &[KEY_A, KEY_Y], &[KEY_A, KEY_T], &[KEY_A, KEY_P], &[KEY_A], &[KEY_A, KEY_C],
        ],
        KEY_O => &[
            &[KEY_O, KEY_N, KEY_G], &[KEY_O, G_END], &[KEY_O, KEY_N], &[KEY_O, KEY_M], &[KEY_O, KEY_I],
            &[KEY_O, KEY_C], &[KEY_O, KEY_T], &[KEY_O, KEY_P], &[KEY_O],
        ],
        KEY_E => &[
            &[KEY_E, KEY_N, KEY_H], &[KEY_E, H_END], &[KEY_E, KEY_N, KEY_G], &[KEY_E, G_END],
            &[KEY_E, KEY_C, KEY_H], &[KEY_E, K_END], &[KEY_E, KEY_C], &[KEY_E, KEY_T], &[KEY_E, KEY_Y],
            &[KEY_E, KEY_U], &[KEY_E, KEY_P], &[KEY_E, KEY_C], &[KEY_E, KEY_N], &[KEY_E, KEY_M], &[KEY_E],
        ],
        KEY_W => &[
            &[KEY_O, KEY_N],
            &[KEY_U, KEY_O, KEY_N, KEY_G], &[KEY_U, KEY_O, G_END],
            &[KEY_U, KEY_O, KEY_N], &[KEY_U, KEY_O, KEY_I], &[KEY_U, KEY_O, KEY_C],
            &[KEY_O, KEY_I], &[KEY_O, KEY_P], &[KEY_O, KEY_M], &[KEY_O, KEY_A], &[KEY_O, KEY_T],
            &[KEY_U, KEY_N, KEY_G], &[KEY_U, G_END], &[KEY_A, KEY_N, KEY_G], &[KEY_A, G_END],
            &[KEY_U, KEY_N], &[KEY_U, KEY_M], &[KEY_U, KEY_C], &[KEY_U, KEY_A], &[KEY_U, KEY_I],
            &[KEY_U, KEY_T], &[KEY_U],
            &[KEY_A, KEY_P], &[KEY_A, KEY_T], &[KEY_A, KEY_M],
            &[KEY_A, KEY_N], &[KEY_A], &[KEY_A, KEY_C], &[KEY_A, KEY_C, KEY_H], &[KEY_A, K_END],
            &[KEY_O], &[KEY_U, KEY_U],
        ],
        _ => &[],
    }
}

/// First element: 1 if the vowel combination can have an end consonant.
pub fn vowel_combine(key: u16) -> &'static [&'static [u32]] {
    const A: u32 = KEY_A as u32;
    const E: u32 = KEY_E as u32;
    const I: u32 = KEY_I as u32;
    const O: u32 = KEY_O as u32;
    const U: u32 = KEY_U as u32;
    const Y: u32 = KEY_Y as u32;
    const T: u32 = TONE_MASK;
    const W: u32 = TONEW_MASK;
    match key {
        KEY_A => &[&[0, A, I], &[0, A, O], &[0, A, U], &[0, A | T, U], &[0, A, Y], &[0, A | T, Y]],
        KEY_E => &[&[0, E, O], &[0, E | T, U]],
        KEY_I => &[&[1, I, E | T, U], &[0, I, A], &[1, I, E | T], &[0, I, U]],
        KEY_O => &[
            &[0, O, A, I], &[0, O, A, O], &[0, O, A, Y], &[0, O, E, O], &[1, O, A], &[1, O, A | W],
            &[1, O, E], &[0, O, I], &[0, O | T, I], &[0, O | W, I], &[1, O, O], &[1, O | T, O | T],
        ],
        KEY_U => &[
            &[0, U, Y, U], &[1, U, Y, E | T], &[0, U, Y, A], &[0, U | W, O | W, U], &[0, U | W, O | W, I],
            &[0, U, O | T, I], &[0, U, A | T, Y], &[1, U, A, O], &[1, U, A], &[1, U, A | W], &[1, U, A | T],
            &[0, U | W, A], &[1, U, E | T], &[0, U, I], &[0, U | W, I], &[1, U, O], &[1, U, O | T],
            &[0, U, O | W], &[1, U | W, O | W], &[0, U | W, U], &[1, U, Y],
        ],
        KEY_Y => &[&[0, Y, E | T, U], &[1, Y, E | T]],
        _ => &[],
    }
}

pub const CONSONANT_D: Charset = &[
    &[KEY_D, KEY_E, KEY_N, KEY_H], &[KEY_D, KEY_E, H_END], &[KEY_D, KEY_E, KEY_N, KEY_G], &[KEY_D, KEY_E, G_END],
    &[KEY_D, KEY_E, KEY_C, KEY_H], &[KEY_D, KEY_E, K_END], &[KEY_D, KEY_E, KEY_N], &[KEY_D, KEY_E, KEY_C],
    &[KEY_D, KEY_E, KEY_M], &[KEY_D, KEY_E], &[KEY_D, KEY_E, KEY_T], &[KEY_D, KEY_E, KEY_U], &[KEY_D, KEY_E, KEY_O],
    &[KEY_D, KEY_E, KEY_P],
    &[KEY_D, KEY_U, KEY_N, KEY_G], &[KEY_D, KEY_U, G_END], &[KEY_D, KEY_U, KEY_N], &[KEY_D, KEY_U, KEY_M],
    &[KEY_D, KEY_U, KEY_C], &[KEY_D, KEY_U, KEY_O], &[KEY_D, KEY_U, KEY_A], &[KEY_D, KEY_U, KEY_O, KEY_I],
    &[KEY_D, KEY_U, KEY_O, KEY_C], &[KEY_D, KEY_U, KEY_O, KEY_N], &[KEY_D, KEY_U, KEY_O, KEY_N, KEY_G],
    &[KEY_D, KEY_U, KEY_O, G_END], &[KEY_D, KEY_U], &[KEY_D, KEY_U, KEY_P], &[KEY_D, KEY_U, KEY_T],
    &[KEY_D, KEY_U, KEY_I],
    &[KEY_D, KEY_I, KEY_C, KEY_H], &[KEY_D, KEY_I, K_END], &[KEY_D, KEY_I, KEY_C], &[KEY_D, KEY_I, KEY_N, KEY_H],
    &[KEY_D, KEY_I, H_END], &[KEY_D, KEY_I, KEY_N], &[KEY_D, KEY_I], &[KEY_D, KEY_I, KEY_A], &[KEY_D, KEY_I, KEY_E],
    &[KEY_D, KEY_I, KEY_E, KEY_C], &[KEY_D, KEY_I, KEY_E, KEY_U], &[KEY_D, KEY_I, KEY_E, KEY_N],
    &[KEY_D, KEY_I, KEY_E, KEY_M], &[KEY_D, KEY_I, KEY_E, KEY_P], &[KEY_D, KEY_I, KEY_T],
    &[KEY_D, KEY_O], &[KEY_D, KEY_O, KEY_A], &[KEY_D, KEY_O, KEY_A, KEY_N], &[KEY_D, KEY_O, KEY_A, KEY_N, KEY_G],
    &[KEY_D, KEY_O, KEY_A, G_END], &[KEY_D, KEY_O, KEY_A, KEY_N, KEY_H], &[KEY_D, KEY_O, KEY_A, H_END],
    &[KEY_D, KEY_O, KEY_A, KEY_M], &[KEY_D, KEY_O, KEY_E], &[KEY_D, KEY_O, KEY_I], &[KEY_D, KEY_O, KEY_P],
    &[KEY_D, KEY_O, KEY_C], &[KEY_D, KEY_O, KEY_N], &[KEY_D, KEY_O, KEY_N, KEY_G], &[KEY_D, KEY_O, G_END],
    &[KEY_D, KEY_O, KEY_M], &[KEY_D, KEY_O, KEY_T],
    &[KEY_D, KEY_A], &[KEY_D, KEY_A, KEY_T], &[KEY_D, KEY_A, KEY_Y], &[KEY_D, KEY_A, KEY_U], &[KEY_D, KEY_A, KEY_I],
    &[KEY_D, KEY_A, KEY_O], &[KEY_D, KEY_A, KEY_P], &[KEY_D, KEY_A, KEY_C], &[KEY_D, KEY_A, KEY_C, KEY_H],
    &[KEY_D, KEY_A, K_END], &[KEY_D, KEY_A, KEY_N], &[KEY_D, KEY_A, KEY_N, KEY_H], &[KEY_D, KEY_A, H_END],
    &[KEY_D, KEY_A, KEY_N, KEY_G], &[KEY_D, KEY_A, G_END], &[KEY_D, KEY_A, KEY_M],
    &[KEY_D],
];

/// `_vowelForMark`, in the iteration order of the original std::map (ascending key code).
pub const VOWEL_FOR_MARK: &[Charset] = &[
    // KEY_A (0)
    &[
        &[KEY_A, KEY_N, KEY_G], &[KEY_A, G_END], &[KEY_A, KEY_N], &[KEY_A, KEY_N, KEY_H], &[KEY_A, H_END],
        &[KEY_A, KEY_M], &[KEY_A, KEY_U], &[KEY_A, KEY_Y], &[KEY_A, KEY_T], &[KEY_A, KEY_P], &[KEY_A],
        &[KEY_A, KEY_C], &[KEY_A, KEY_I], &[KEY_A, KEY_O], &[KEY_A, KEY_C, KEY_H], &[KEY_A, K_END],
    ],
    // KEY_E (14)
    &[
        &[KEY_E, KEY_N, KEY_H], &[KEY_E, H_END], &[KEY_E, KEY_N, KEY_G], &[KEY_E, G_END], &[KEY_E, KEY_C, KEY_H],
        &[KEY_E, K_END], &[KEY_E, KEY_C], &[KEY_E, KEY_T], &[KEY_E, KEY_Y], &[KEY_E, KEY_U], &[KEY_E, KEY_P],
        &[KEY_E, KEY_C], &[KEY_E, KEY_N], &[KEY_E, KEY_M], &[KEY_E],
    ],
    // KEY_Y (16)
    &[&[KEY_Y]],
    // KEY_O (31)
    &[
        &[KEY_O, KEY_O, KEY_N, KEY_G], &[KEY_O, KEY_O, G_END], &[KEY_O, KEY_N, KEY_G], &[KEY_O, G_END],
        &[KEY_O, KEY_O, KEY_N], &[KEY_O, KEY_O, KEY_C], &[KEY_O, KEY_O], &[KEY_O, KEY_N], &[KEY_O, KEY_M],
        &[KEY_O, KEY_I], &[KEY_O, KEY_C], &[KEY_O, KEY_T], &[KEY_O, KEY_P], &[KEY_O],
    ],
    // KEY_U (32)
    &[
        &[KEY_U, KEY_N, KEY_G], &[KEY_U, G_END], &[KEY_U, KEY_I], &[KEY_U, KEY_O], &[KEY_U, KEY_Y],
        &[KEY_U, KEY_Y, KEY_N], &[KEY_U, KEY_Y, KEY_T], &[KEY_U, KEY_Y, KEY_P], &[KEY_U, KEY_Y, KEY_C, KEY_H],
        &[KEY_U, KEY_Y, K_END], &[KEY_U, KEY_Y, KEY_N, KEY_H], &[KEY_U, KEY_Y, H_END], &[KEY_U, KEY_T],
        &[KEY_U, KEY_U], &[KEY_U, KEY_A], &[KEY_U, KEY_I], &[KEY_U, KEY_C], &[KEY_U, KEY_N], &[KEY_U, KEY_M],
        &[KEY_U, KEY_P], &[KEY_U],
    ],
    // KEY_I (34)
    &[
        &[KEY_I, KEY_N, KEY_H], &[KEY_I, H_END], &[KEY_I, KEY_C, KEY_H], &[KEY_I, K_END], &[KEY_I, KEY_N],
        &[KEY_I, KEY_T], &[KEY_I, KEY_U], &[KEY_I, KEY_U, KEY_P], &[KEY_I, KEY_N], &[KEY_I, KEY_M], &[KEY_I, KEY_P],
        &[KEY_I, KEY_A], &[KEY_I, KEY_C], &[KEY_I],
    ],
];

pub const CONSONANT_TABLE: Charset = &[
    &[KEY_N, KEY_G, KEY_H], &[KEY_P, KEY_H], &[KEY_T, KEY_H], &[KEY_T, KEY_R], &[KEY_G, KEY_I], &[KEY_C, KEY_H],
    &[KEY_N, KEY_H], &[KEY_N, KEY_G], &[KEY_K, KEY_H], &[KEY_G, KEY_H], &[KEY_G], &[KEY_C], &[KEY_Q], &[KEY_K],
    &[KEY_T], &[KEY_R], &[KEY_H], &[KEY_B], &[KEY_M], &[KEY_V], &[KEY_N], &[KEY_L], &[KEY_X], &[KEY_P], &[KEY_S],
    &[KEY_D],
    &[KEY_F | CONSONANT_ALLOW_MASK], &[KEY_W | CONSONANT_ALLOW_MASK], &[KEY_Z | CONSONANT_ALLOW_MASK],
    &[KEY_J | CONSONANT_ALLOW_MASK],
    &[KEY_F | END_CONSONANT_MASK], &[KEY_W | END_CONSONANT_MASK], &[KEY_J | END_CONSONANT_MASK],
];

pub const END_CONSONANT_TABLE: Charset = &[
    &[KEY_T], &[KEY_P], &[KEY_C], &[KEY_N], &[KEY_M], &[G_END], &[K_END], &[H_END],
    &[KEY_C, KEY_H], &[KEY_N, KEY_H], &[KEY_N, KEY_G],
];

pub const STANDALONE_W_BAD: &[u16] = &[KEY_W, KEY_E, KEY_Y, KEY_F, KEY_J, KEY_K, KEY_Z];

pub const DOUBLE_W_ALLOWED: &[[u16; 2]] = &[
    [KEY_T, KEY_R], [KEY_T, KEY_H], [KEY_C, KEY_H], [KEY_N, KEY_H], [KEY_N, KEY_G], [KEY_K, KEY_H], [KEY_G, KEY_I],
    [KEY_P, KEY_H], [KEY_G, KEY_H],
];

pub fn quick_start_consonant(k: u16) -> Option<[u16; 2]> {
    match k {
        KEY_F => Some([KEY_P, KEY_H]),
        KEY_J => Some([KEY_G, KEY_I]),
        KEY_W => Some([KEY_Q, KEY_U]),
        _ => None,
    }
}

pub fn quick_end_consonant(k: u16) -> Option<[u16; 2]> {
    match k {
        KEY_G => Some([KEY_N, KEY_G]),
        KEY_H => Some([KEY_N, KEY_H]),
        KEY_K => Some([KEY_C, KEY_H]),
        _ => None,
    }
}

pub fn quick_telex(k: u16) -> Option<[u16; 2]> {
    match k {
        KEY_C => Some([KEY_C, KEY_H]),
        KEY_G => Some([KEY_G, KEY_I]),
        KEY_K => Some([KEY_K, KEY_H]),
        KEY_N => Some([KEY_N, KEY_G]),
        KEY_Q => Some([KEY_Q, KEY_U]),
        KEY_P => Some([KEY_P, KEY_H]),
        KEY_T => Some([KEY_T, KEY_H]),
        KEY_U => Some([KEY_U, KEY_U]),
        _ => None,
    }
}

const AT: u32 = KEY_A as u32 | TONE_MASK;
const AW: u32 = KEY_A as u32 | TONEW_MASK;
const ET: u32 = KEY_E as u32 | TONE_MASK;
const OT: u32 = KEY_O as u32 | TONE_MASK;
const OW: u32 = KEY_O as u32 | TONEW_MASK;
const UW: u32 = KEY_U as u32 | TONEW_MASK;
const A: u32 = KEY_A as u32;
const D: u32 = KEY_D as u32;
const E: u32 = KEY_E as u32;
const Y: u32 = KEY_Y as u32;
const O: u32 = KEY_O as u32;
const U: u32 = KEY_U as u32;
const I: u32 = KEY_I as u32;

pub type CodeTable = [(u32, &'static [u16]); 13];

/// 0: Unicode, 1: TCVN3, 2: VNI Windows, 3: Unicode Compound, 4: Vietnamese Locale CP 1258.
/// Entries are in ascending key order (the iteration order of the original std::map).
/// Base vowels: {Â, â, Ă, ă, Á, á, À, à, Ả, ả, Ã, ã, Ạ, ạ}; toned vowels: {sắc, huyền, hỏi, ngã, nặng} × {caps, lower}.
pub static CODE_TABLES: [CodeTable; 5] = [
    [
        (A, &[0x00C2, 0x00E2, 0x0102, 0x0103, 0x00C1, 0x00E1, 0x00C0, 0x00E0, 0x1EA2, 0x1EA3, 0x00C3, 0x00E3, 0x1EA0, 0x1EA1]),
        (D, &[0x0110, 0x0111]),
        (E, &[0x00CA, 0x00EA, 0x0000, 0x0000, 0x00C9, 0x00E9, 0x00C8, 0x00E8, 0x1EBA, 0x1EBB, 0x1EBC, 0x1EBD, 0x1EB8, 0x1EB9]),
        (Y, &[0x00DD, 0x00FD, 0x1EF2, 0x1EF3, 0x1EF6, 0x1EF7, 0x1EF8, 0x1EF9, 0x1EF4, 0x1EF5]),
        (O, &[0x00D4, 0x00F4, 0x01A0, 0x01A1, 0x00D3, 0x00F3, 0x00D2, 0x00F2, 0x1ECE, 0x1ECF, 0x00D5, 0x00F5, 0x1ECC, 0x1ECD]),
        (U, &[0x0000, 0x0000, 0x01AF, 0x01B0, 0x00DA, 0x00FA, 0x00D9, 0x00F9, 0x1EE6, 0x1EE7, 0x0168, 0x0169, 0x1EE4, 0x1EE5]),
        (I, &[0x00CD, 0x00ED, 0x00CC, 0x00EC, 0x1EC8, 0x1EC9, 0x0128, 0x0129, 0x1ECA, 0x1ECB]),
        (AT, &[0x1EA4, 0x1EA5, 0x1EA6, 0x1EA7, 0x1EA8, 0x1EA9, 0x1EAA, 0x1EAB, 0x1EAC, 0x1EAD]),
        (ET, &[0x1EBE, 0x1EBF, 0x1EC0, 0x1EC1, 0x1EC2, 0x1EC3, 0x1EC4, 0x1EC5, 0x1EC6, 0x1EC7]),
        (OT, &[0x1ED0, 0x1ED1, 0x1ED2, 0x1ED3, 0x1ED4, 0x1ED5, 0x1ED6, 0x1ED7, 0x1ED8, 0x1ED9]),
        (AW, &[0x1EAE, 0x1EAF, 0x1EB0, 0x1EB1, 0x1EB2, 0x1EB3, 0x1EB4, 0x1EB5, 0x1EB6, 0x1EB7]),
        (OW, &[0x1EDA, 0x1EDB, 0x1EDC, 0x1EDD, 0x1EDE, 0x1EDF, 0x1EE0, 0x1EE1, 0x1EE2, 0x1EE3]),
        (UW, &[0x1EE8, 0x1EE9, 0x1EEA, 0x1EEB, 0x1EEC, 0x1EED, 0x1EEE, 0x1EEF, 0x1EF0, 0x1EF1]),
    ],
    [
        (A, &[0xA2, 0xA9, 0xA1, 0xA8, 0xB8, 0xB8, 0xB5, 0xB5, 0xB6, 0xB6, 0xB7, 0xB7, 0xB9, 0xB9]),
        (D, &[0xA7, 0xAE]),
        (E, &[0xA3, 0xAA, 0x00, 0x00, 0xD0, 0xD0, 0xCC, 0xCC, 0xCE, 0xCE, 0xCF, 0xCF, 0xD1, 0xD1]),
        (Y, &[0xFD, 0xFD, 0xFA, 0xFA, 0xFB, 0xFB, 0xFC, 0xFC, 0xFE, 0xFE]),
        (O, &[0xA4, 0xAB, 0xA5, 0xAC, 0xE3, 0xE3, 0xDF, 0xDF, 0xE1, 0xE1, 0xE2, 0xE2, 0xE4, 0xE4]),
        (U, &[0x00, 0x00, 0xA6, 0xAD, 0xF3, 0xF3, 0xEF, 0xEF, 0xF1, 0xF1, 0xF2, 0xF2, 0xF4, 0xF4]),
        (I, &[0xDD, 0xDD, 0xD7, 0xD7, 0xD8, 0xD8, 0xDC, 0xDC, 0xDE, 0xDE]),
        (AT, &[0xCA, 0xCA, 0xC7, 0xC7, 0xC8, 0xC8, 0xC9, 0xC9, 0xCB, 0xCB]),
        (ET, &[0xD5, 0xD5, 0xD2, 0xD2, 0xD3, 0xD3, 0xD4, 0xD4, 0xD6, 0xD6]),
        (OT, &[0xE8, 0xE8, 0xE5, 0xE5, 0xE6, 0xE6, 0xE7, 0xE7, 0xE9, 0xE9]),
        (AW, &[0xBE, 0xBE, 0xBB, 0xBB, 0xBC, 0xBC, 0xBD, 0xBD, 0xC6, 0xC6]),
        (OW, &[0xED, 0xED, 0xEA, 0xEA, 0xEB, 0xEB, 0xEC, 0xEC, 0xEE, 0xEE]),
        (UW, &[0xF8, 0xF8, 0xF5, 0xF5, 0xF6, 0xF6, 0xF7, 0xF7, 0xF9, 0xF9]),
    ],
    [
        (A, &[0xC241, 0xE261, 0xCA41, 0xEA61, 0xD941, 0xF961, 0xD841, 0xF861, 0xDB41, 0xFB61, 0xD541, 0xF561, 0xCF41, 0xEF61]),
        (D, &[0x00D1, 0x00F1]),
        (E, &[0xC245, 0xE265, 0x0000, 0x0000, 0xD945, 0xF965, 0xD845, 0xF865, 0xDB45, 0xFB65, 0xD545, 0xF565, 0xCF45, 0xEF65]),
        (Y, &[0xD959, 0xF979, 0xD859, 0xF879, 0xDB59, 0xFB79, 0xD559, 0xF579, 0x00CE, 0x00EE]),
        (O, &[0xC24F, 0xE26F, 0x00D4, 0x00F4, 0xD94F, 0xF96F, 0xD84F, 0xF86F, 0xDB4F, 0xFB6F, 0xD54F, 0xF56F, 0xCF4F, 0xEF6F]),
        (U, &[0x0000, 0x0000, 0x00D6, 0x00F6, 0xD955, 0xF975, 0xD855, 0xF875, 0xDB55, 0xFB75, 0xD555, 0xF575, 0xCF55, 0xEF75]),
        (I, &[0x00CD, 0x00ED, 0x00CC, 0x00EC, 0x00C6, 0x00E6, 0x00D3, 0x00F3, 0x00D2, 0x00F2]),
        (AT, &[0xC141, 0xE161, 0xC041, 0xE061, 0xC541, 0xE561, 0xC341, 0xE361, 0xC441, 0xE461]),
        (ET, &[0xC145, 0xE165, 0xC045, 0xE065, 0xC545, 0xE565, 0xC345, 0xE365, 0xC445, 0xE465]),
        (OT, &[0xC14F, 0xE16F, 0xC04F, 0xE06F, 0xC54F, 0xE56F, 0xC34F, 0xE36F, 0xC44F, 0xE46F]),
        (AW, &[0xC941, 0xE961, 0xC841, 0xE861, 0xDA41, 0xFA61, 0xDC41, 0xFC61, 0xCB41, 0xEB61]),
        (OW, &[0xD9D4, 0xF9F4, 0xD8D4, 0xF8F4, 0xDBD4, 0xFBF4, 0xD5D4, 0xF5F4, 0xCFD4, 0xEFF4]),
        (UW, &[0xD9D6, 0xF9F6, 0xD8D6, 0xF8F6, 0xDBD6, 0xFBF6, 0xD5D6, 0xF5F6, 0xCFD6, 0xEFF6]),
    ],
    [
        (A, &[0x00C2, 0x00E2, 0x0102, 0x0103, 0x2041, 0x2061, 0x4041, 0x4061, 0x6041, 0x6061, 0x8041, 0x8061, 0xA041, 0xA061]),
        (D, &[0x0110, 0x0111]),
        (E, &[0x00CA, 0x00EA, 0x0000, 0x0000, 0x2045, 0x2065, 0x4045, 0x4065, 0x6045, 0x6065, 0x8045, 0x8065, 0xA045, 0xA065]),
        (Y, &[0x2059, 0x2079, 0x4059, 0x4079, 0x6059, 0x6079, 0x8059, 0x8079, 0xA059, 0xA079]),
        (O, &[0x00D4, 0x00F4, 0x01A0, 0x01A1, 0x204F, 0x206F, 0x404F, 0x406F, 0x604F, 0x606F, 0x804F, 0x806F, 0xA04F, 0xA06F]),
        (U, &[0x0000, 0x0000, 0x01AF, 0x01B0, 0x2055, 0x2075, 0x4055, 0x4075, 0x6055, 0x6075, 0x8055, 0x8075, 0xA055, 0xA075]),
        (I, &[0x2049, 0x2069, 0x4049, 0x4069, 0x6049, 0x6069, 0x8049, 0x8069, 0xA049, 0xA069]),
        (AT, &[0x20C2, 0x20E2, 0x40C2, 0x40E2, 0x60C2, 0x60E2, 0x80C2, 0x80E2, 0xA0C2, 0xA0E2]),
        (ET, &[0x20CA, 0x20EA, 0x40CA, 0x40EA, 0x60CA, 0x60EA, 0x80CA, 0x80EA, 0xA0CA, 0xA0EA]),
        (OT, &[0x20D4, 0x20F4, 0x40D4, 0x40F4, 0x60D4, 0x60F4, 0x80D4, 0x80F4, 0xA0D4, 0xA0F4]),
        (AW, &[0x2102, 0x2103, 0x4102, 0x4103, 0x6102, 0x6103, 0x8102, 0x8103, 0xA102, 0xA103]),
        (OW, &[0x21A0, 0x21A1, 0x41A0, 0x41A1, 0x61A0, 0x61A1, 0x81A0, 0x81A1, 0xA1A0, 0xA1A1]),
        (UW, &[0x21AF, 0x21B0, 0x41AF, 0x41B0, 0x61AF, 0x61B0, 0x81AF, 0x81B0, 0xA1AF, 0xA1B0]),
    ],
    [
        (A, &[0x00C2, 0x00E2, 0x00C3, 0x00E3, 0xEC41, 0xEC61, 0xCC41, 0xCC61, 0xD241, 0xD261, 0xDE41, 0xDE61, 0xF241, 0xF261]),
        (D, &[0x00D0, 0x00F0]),
        (E, &[0x00CA, 0x00EA, 0x0000, 0x0000, 0xEC45, 0xEC65, 0xCC45, 0xCC65, 0xD245, 0xD265, 0xDE45, 0xDE65, 0xF245, 0xF265]),
        (Y, &[0xEC59, 0xEC79, 0xCC59, 0xCC79, 0xD259, 0xD279, 0xDE59, 0xDE79, 0xF259, 0xF279]),
        (O, &[0x00D4, 0x00F4, 0x00D5, 0x00F5, 0xEC4F, 0xEC6F, 0xCC4F, 0xCC6F, 0xD24F, 0xD26F, 0xDE4F, 0xDE6F, 0xF24F, 0xF26F]),
        (U, &[0x0000, 0x0000, 0x00DD, 0x00FD, 0xEC55, 0xEC75, 0xCC55, 0xCC75, 0xD255, 0xD275, 0xDE55, 0xDE75, 0xF255, 0xF275]),
        (I, &[0xEC49, 0xEC69, 0xCC49, 0xCC69, 0xD249, 0xD269, 0xDE49, 0xDE69, 0xF249, 0xF269]),
        (AT, &[0xECC2, 0xECE2, 0xCCC2, 0xCCE2, 0xD2C2, 0xD2E2, 0xDEC2, 0xDEE2, 0xF2C2, 0xF2E2]),
        (ET, &[0xECCA, 0xECEA, 0xCCCA, 0xCCEA, 0xD2CA, 0xD2EA, 0xDECA, 0xDEEA, 0xF2CA, 0xF2EA]),
        (OT, &[0xECD4, 0xECF4, 0xCCD4, 0xCCF4, 0xD2D4, 0xD2F4, 0xDED4, 0xDEF4, 0xF2D4, 0xF2F4]),
        (AW, &[0xECC3, 0xECE3, 0xCCC3, 0xCCE3, 0xD2C3, 0xD2E3, 0xDEC3, 0xDEE3, 0xF2C3, 0xF2E3]),
        (OW, &[0xECD5, 0xECF5, 0xCCD5, 0xCCF5, 0xD2D5, 0xD2F5, 0xDED5, 0xDEF5, 0xF2D5, 0xF2F5]),
        (UW, &[0xECDD, 0xECFD, 0xCCDD, 0xCCFD, 0xD2DD, 0xD2FD, 0xDEDD, 0xDEFD, 0xF2DD, 0xF2FD]),
    ],
];

pub fn code_entry(table: usize, key: u32) -> Option<&'static [u16]> {
    CODE_TABLES[table.min(4)].iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// sắc, huyền, hỏi, ngã, nặng - combining marks for Unicode Compound
pub const UNICODE_COMPOUND_MARK: [u16; 5] = [0x0301, 0x0300, 0x0309, 0x0303, 0x0323];

const CHARACTER_MAP: &[(char, u16, bool)] = &[
    ('a', KEY_A, false), ('b', KEY_B, false), ('c', KEY_C, false), ('d', KEY_D, false), ('e', KEY_E, false),
    ('f', KEY_F, false), ('g', KEY_G, false), ('h', KEY_H, false), ('i', KEY_I, false), ('j', KEY_J, false),
    ('k', KEY_K, false), ('l', KEY_L, false), ('m', KEY_M, false), ('n', KEY_N, false), ('o', KEY_O, false),
    ('p', KEY_P, false), ('q', KEY_Q, false), ('r', KEY_R, false), ('s', KEY_S, false), ('t', KEY_T, false),
    ('u', KEY_U, false), ('v', KEY_V, false), ('w', KEY_W, false), ('x', KEY_X, false), ('y', KEY_Y, false),
    ('z', KEY_Z, false),
    ('1', KEY_1, false), ('!', KEY_1, true), ('2', KEY_2, false), ('@', KEY_2, true), ('3', KEY_3, false),
    ('#', KEY_3, true), ('4', KEY_4, false), ('$', KEY_4, true), ('5', KEY_5, false), ('%', KEY_5, true),
    ('6', KEY_6, false), ('^', KEY_6, true), ('7', KEY_7, false), ('&', KEY_7, true), ('8', KEY_8, false),
    ('*', KEY_8, true), ('9', KEY_9, false), ('(', KEY_9, true), ('0', KEY_0, false), (')', KEY_0, true),
    ('`', KEY_BACKQUOTE, false), ('~', KEY_BACKQUOTE, true), ('-', KEY_MINUS, false), ('_', KEY_MINUS, true),
    ('=', KEY_EQUALS, false), ('+', KEY_EQUALS, true), ('[', KEY_LEFT_BRACKET, false), ('{', KEY_LEFT_BRACKET, true),
    (']', KEY_RIGHT_BRACKET, false), ('}', KEY_RIGHT_BRACKET, true), ('\\', KEY_BACK_SLASH, false),
    ('|', KEY_BACK_SLASH, true), (';', KEY_SEMICOLON, false), (':', KEY_SEMICOLON, true), ('\'', KEY_QUOTE, false),
    ('"', KEY_QUOTE, true), (',', KEY_COMMA, false), ('<', KEY_COMMA, true), ('.', KEY_DOT, false),
    ('>', KEY_DOT, true), ('/', KEY_SLASH, false), ('?', KEY_SLASH, true), (' ', KEY_SPACE, false),
];

/// Character -> engine key data (key code | CAPS_MASK).
pub fn char_to_key(c: u32) -> Option<u32> {
    let ch = char::from_u32(c)?;
    if ch.is_ascii_uppercase() {
        return char_to_key(ch.to_ascii_lowercase() as u32).map(|k| k | CAPS_MASK);
    }
    CHARACTER_MAP
        .iter()
        .find(|(c, _, _)| *c == ch)
        .map(|&(_, k, caps)| k as u32 | if caps { CAPS_MASK } else { 0 })
}

/// Engine key data (key code | CAPS_MASK) -> character, 0 if unknown.
pub fn key_to_char(key: u32) -> u16 {
    for &(c, k, shifted) in CHARACTER_MAP {
        let k = k as u32;
        if c.is_ascii_lowercase() && key == k | CAPS_MASK {
            return c.to_ascii_uppercase() as u16;
        }
        if key == k | if shifted { CAPS_MASK } else { 0 } {
            return c as u16;
        }
    }
    0
}
