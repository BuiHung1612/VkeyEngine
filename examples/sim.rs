//! Differential testing helper: types each stdin line with a fresh engine, prints the screen.
//! Args: input_type code_table modern(0/1) spelling(0/1) restore(0/1) quick_telex(0/1) free_mark(0/1)
use vkeyengine::engine::{Engine, Settings};
use std::io::BufRead;

fn main() {
    let a: Vec<u8> = std::env::args().skip(1).map(|x| x.parse().unwrap()).collect();
    let s = Settings {
        input_type: a[0],
        code_table: a[1],
        modern_orthography: a[2] == 1,
        check_spelling: a[3] == 1,
        restore_if_wrong_spelling: a[4] == 1,
        quick_telex: a[5] == 1,
        free_mark: a[6] == 1,
        ..Settings::default()
    };
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let out = std::panic::catch_unwind(|| vkeyengine::sim::type_with(&mut Engine::new(s.clone()), &line))
            .unwrap_or_else(|_| "<PANIC>".into());
        println!("{}", out.encode_utf16().map(|u| format!("{u:04x}")).collect::<Vec<_>>().join(" "));
    }
}
