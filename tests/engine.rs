use vkeyengine::convert::{convert, ConvertOptions};
use vkeyengine::engine::*;


use vkeyengine::sim::type_with;
use vkeyengine::tables::key_to_char;

fn typ(input: &str) -> String {
    type_with(&mut Engine::default(), input)
}

fn typ_s(s: Settings, input: &str) -> String {
    type_with(&mut Engine::new(s), input)
}

#[test]
fn telex_basic() {
    assert_eq!(typ("xin chaof cacs banj"), "xin chào các bạn");
    assert_eq!(typ("tieengs vieetj"), "tiếng việt");
    assert_eq!(typ("dduwowcj nguwowif thuwowngf"), "được người thường");
    assert_eq!(typ("quas gif giaf"), "quá gì già");
    assert_eq!(typ("w ww"), "ư w");
    assert_eq!(typ("Vieetj Nam"), "Việt Nam");
    assert_eq!(typ("khoong"), "không");
    assert_eq!(typ("thuowr"), "thuở");
    assert_eq!(typ("nghieeng"), "nghiêng");
    assert_eq!(typ("tuyeets"), "tuyết");
}

#[test]
fn mark_order_and_free_typing() {
    // mark typed before the vowel is complete is moved to the right place
    assert_eq!(typ("tiesng"), "tiéng");
    assert_eq!(typ("dduowcj"), "được");
    assert_eq!(typ("ass"), "as");
    assert_eq!(typ("ddd"), "dd");
}

#[test]
fn orthography() {
    assert_eq!(typ("hoaf"), "hoà");
    let old = Settings { modern_orthography: false, ..Settings::default() };
    assert_eq!(typ_s(old, "hoaf"), "hòa");
}

#[test]
fn uppercase_words_match_lowercase() {
    for w in ["thuoocs", "nguwowif", "dduwowcj", "khuyeens", "quoocs", "hoaf", "tieengs", "giuwax", "muoons"] {
        let lower = typ(w);
        let upper = typ(&w.to_uppercase());
        assert_eq!(upper, lower.to_uppercase(), "typing {w}");
    }
}

#[test]
fn restore_tone_on_marked_vowel_does_not_send_nul() {
    // C++ sent the raw engine value (a NUL character) for the restored vowel
    assert_eq!(typ("oos"), "ố");
    assert_eq!(typ("ooso"), "óo");
    assert_eq!(typ("aafa"), "àa");
}

#[test]
fn no_nul_output() {
    for input in ["r[asw\x08n", "y\x08woeerld", "[woo\x08xof jno", "[a", "wa", "[aa", "waa", "ooso", "aafa"] {
        for it in [TELEX, SIMPLE_TELEX_2] {
            let out = typ_s(Settings { input_type: it, ..Settings::default() }, input);
            assert!(!out.contains('\0'), "NUL while typing {input:?}: {out:?}");
        }
    }
}

#[test]
fn vni() {
    let s = Settings { input_type: VNI, ..Settings::default() };
    assert_eq!(typ_s(s.clone(), "Vie65t Na8m"), "Việt Năm");
    assert_eq!(typ_s(s.clone(), "d9u7o7c5"), "được");
    assert_eq!(typ_s(s, "Mi#"), "Mi#");
}

#[test]
fn backspace_keeps_word_state() {
    assert_eq!(typ("vieet\x08j"), "việ");
    assert_eq!(typ("chaof \x08s"), "cháo");
}

#[test]
fn spelling_restore() {
    let s = Settings { restore_if_wrong_spelling: true, ..Settings::default() };
    assert_eq!(typ_s(s.clone(), "windows "), "windows ");
    assert_eq!(typ_s(s.clone(), "tesst "), "test ");
    assert_eq!(typ_s(s, "tesk "), "tesk ");
}

#[test]
fn z_words_with_allow_zfwj() {
    let s = Settings { allow_consonant_zfwj: true, ..Settings::default() };
    assert_eq!(typ_s(s.clone(), "zaj"), "zạ");
    assert_eq!(typ_s(s, "zaayj"), "zậy");
}

fn macro_engine(s: Settings, defs: &[(&str, &str)]) -> Engine {
    let mut e = Engine::new(s);
    for (k, v) in defs {
        let t = e.s.code_table;
        e.macros.add(k, v, t);
    }
    e
}

#[test]
fn macro_basic() {
    let mut e = macro_engine(Settings::default(), &[("btw", "by the way")]);
    assert_eq!(type_with(&mut e, "btw "), "by the way ");
}

#[test]
fn macro_starting_with_standalone_w() {
    // #313: "ưd" typed as "wd" was never matched
    let mut e = macro_engine(Settings::default(), &[("ưd", "window"), ("ưds", "windows")]);
    assert_eq!(type_with(&mut e, "wd "), "window ");
    assert_eq!(type_with(&mut e, "wds "), "windows ");
}

#[test]
fn macro_after_processed_keys() {
    // #284: macro key buffer got out of sync after quick telex / grammar fixes
    let qt = Settings { quick_telex: true, ..Settings::default() };
    let mut e = macro_engine(qt, &[("ch", "chào")]);
    assert_eq!(type_with(&mut e, "cc "), "chào ");
    let mut e = macro_engine(Settings::default(), &[("được", "OK")]);
    assert_eq!(type_with(&mut e, "dduowcj "), "OK ");
    let mut e = macro_engine(Settings::default(), &[("as", "AS")]);
    assert_eq!(type_with(&mut e, "ass "), "AS ");
}

#[test]
fn macro_auto_caps() {
    let s = Settings { auto_caps_macro: true, ..Settings::default() };
    let mut e = macro_engine(s, &[("tv", "tiếng Việt")]);
    assert_eq!(type_with(&mut e, "tv "), "tiếng Việt ");
    assert_eq!(type_with(&mut e, "Tv "), "Tiếng Việt ");
    assert_eq!(type_with(&mut e, "TV "), "TIẾNG VIỆT ");
}

#[test]
fn macro_auto_caps_non_vietnamese_char() {
    let s = Settings { auto_caps_macro: true, ..Settings::default() };
    let mut e = macro_engine(s, &[("ny", "ñandú")]);
    assert_eq!(type_with(&mut e, "Ny "), "Ñandú ");
}

#[test]
fn macro_survives_code_table_change() {
    let mut e = macro_engine(Settings::default(), &[("ưd", "window")]);
    e.set_code_table(2);
    e.set_code_table(0);
    assert_eq!(type_with(&mut e, "wd "), "window ");
}

#[test]
fn macro_in_english_mode() {
    let s = Settings { language: 0, ..Settings::default() };
    let mut e = macro_engine(s, &[("btw", "by the way")]);
    assert_eq!(type_with(&mut e, "btw "), "by the way ");
}

#[test]
fn macro_binary_roundtrip() {
    let e = macro_engine(Settings::default(), &[("btw", "by the way"), ("vn", "Việt Nam 🇻🇳")]);
    let bin = e.macros.save_binary();
    let mut m = vkeyengine::macros::Macros::default();
    m.load_binary(&bin, 0);
    assert_eq!(m.all(), e.macros.all());
    let mut m2 = vkeyengine::macros::Macros::default();
    m2.import_text(&e.macros.export_text(), false, 0);
    assert_eq!(m2.all(), e.macros.all());
}

#[test]
fn upper_case_first_char() {
    let s = Settings { upper_case_first_char: true, ..Settings::default() };
    assert_eq!(typ_s(s, "chaof. banj"), "chào. Bạn");
}

#[test]
fn code_tables_output() {
    let vni = Settings { code_table: 2, ..Settings::default() };
    let out = typ_s(vni, "vieetj");
    assert_eq!(out, "vie\u{00e4}t");
    let compound = Settings { code_table: 3, ..Settings::default() };
    assert_eq!(typ_s(compound, "as"), "a\u{0301}");
}

#[test]
fn convert_preserves_case() {
    let o = ConvertOptions { from_code: 0, to_code: 0, ..Default::default() };
    assert_eq!(convert("Hà Nội", &o), "Hà Nội");
    let caps = ConvertOptions { to_all_caps: true, ..Default::default() };
    assert_eq!(convert("Hà Nội", &caps), "HÀ NỘI");
    let lower = ConvertOptions { to_all_non_caps: true, ..Default::default() };
    assert_eq!(convert("HÀ NỘI", &lower), "hà nội");
    let each = ConvertOptions { caps_each_word: true, ..Default::default() };
    assert_eq!(convert("hà nội", &each), "Hà Nội");
    let first = ConvertOptions { caps_first_letter: true, ..Default::default() };
    assert_eq!(convert("hà nội. đẹp", &first), "Hà nội. Đẹp");
    let rm = ConvertOptions { remove_mark: true, ..Default::default() };
    assert_eq!(convert("Đà Nẵng", &rm), "Da Nang");
}

#[test]
fn convert_roundtrip_all_tables() {
    let text = "Tiếng Việt có dấu: ắ ằ ẳ ẵ ặ ư ơ đ Đ. Ừ";
    for t in 1..=4u8 {
        let to = convert(text, &ConvertOptions { from_code: 0, to_code: t, ..Default::default() });
        let back = convert(&to, &ConvertOptions { from_code: t, to_code: 0, ..Default::default() });
        if t == 1 {
            // TCVN3 has no upper case marked letters
            assert_eq!(back, "Tiếng Việt có dấu: ắ ằ ẳ ẵ ặ ư ơ đ Đ. ừ", "table {t}");
        } else {
            assert_eq!(back, text, "table {t}");
        }
    }
}

#[test]
fn english_words_typed_with_telex_come_back() {
    let s = Settings { restore_english: true, ..Settings::default() };
    let t = |input: &str| type_with(&mut Engine::new(s.clone()), input);
    assert_eq!(t("boss "), "boss ");
    assert_eq!(t("good "), "good ");
    assert_eq!(t("coffee, "), "coffee, ");
    assert_eq!(t("Windows "), "Windows ");
    // still Vietnamese
    assert_eq!(t("nguowif "), "người ");
    assert_eq!(t("tieengs Vieejt "), "tiếng Việt ");
    // English and a valid Vietnamese syllable: Vietnamese wins
    assert_eq!(t("this "), "thí ");
    // an invalid word that is not English is left alone (as without the option)
    let off = type_with(&mut Engine::new(Settings::default()), "asdf ");
    assert_eq!(t("asdf "), off);
}

#[test]
fn undo_word_gives_the_keys_back() {
    let mut e = Engine::new(Settings::default());
    assert_eq!(type_with(&mut e, "vieejt"), "việt");
    assert!(e.undo_word());
    assert_eq!(e.hook.backspace_count, 4);
    assert_eq!(e.hook.new_char_count, 6);
    let typed: String =
        (0..6).rev().map(|i| char::from_u32(key_to_char(e.hook.char_data[i]) as u32).unwrap()).collect();
    assert_eq!(typed, "vieejt");
    // a word shown as typed has nothing to undo
    let mut e = Engine::new(Settings::default());
    type_with(&mut e, "ban");
    assert!(!e.undo_word());
}
