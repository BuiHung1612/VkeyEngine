//! Activity numbers for the "Hoạt động" page: key handling time (recorded by the key hooks)
//! and process CPU / memory samples (taken by the platform's 2 s watchdog).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// CPU samples kept for the graph (2 min at one sample every 2 s).
pub const HISTORY: usize = 60;

static KEYS: AtomicU64 = AtomicU64::new(0);
static BUSY_NS: AtomicU64 = AtomicU64::new(0);
static MAX_NS: AtomicU64 = AtomicU64::new(0);

struct Samples {
    started: Instant,
    last: Option<(Instant, Duration)>,
    cpu: VecDeque<f32>,
    memory: u64,
}

static SAMPLES: Mutex<Option<Samples>> = Mutex::new(None);

/// Time spent handling one key event.
pub fn record_key(spent: Duration) {
    let ns = spent.as_nanos() as u64;
    KEYS.fetch_add(1, Ordering::Relaxed);
    BUSY_NS.fetch_add(ns, Ordering::Relaxed);
    MAX_NS.fetch_max(ns, Ordering::Relaxed);
}

/// Add a sample: total CPU time used by the process so far, and its memory in bytes.
pub fn sample(cpu_time: Duration, memory: u64) {
    sample_at(Instant::now(), cpu_time, memory);
}

fn sample_at(now: Instant, cpu_time: Duration, memory: u64) {
    let mut g = SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
    let s = g.get_or_insert_with(|| Samples { started: now, last: None, cpu: VecDeque::new(), memory });
    if let Some((at, cpu)) = s.last {
        let wall = now.duration_since(at).as_secs_f32();
        if wall > 0.0 {
            let percent = cpu_time.saturating_sub(cpu).as_secs_f32() / wall * 100.0;
            if s.cpu.len() == HISTORY {
                s.cpu.pop_front();
            }
            s.cpu.push_back(percent);
        }
    }
    s.last = Some((now, cpu_time));
    s.memory = memory;
}

#[derive(Default, Clone)]
pub struct Snapshot {
    pub memory: u64,
    /// latest CPU %, and the history (oldest first)
    pub cpu: f32,
    pub cpu_history: Vec<f32>,
    pub uptime: Duration,
    pub keys: u64,
    pub avg_key: Duration,
    pub max_key: Duration,
}

pub fn snapshot() -> Snapshot {
    let keys = KEYS.load(Ordering::Relaxed);
    let busy = BUSY_NS.load(Ordering::Relaxed);
    let mut snap = Snapshot {
        keys,
        avg_key: Duration::from_nanos(busy.checked_div(keys).unwrap_or(0)),
        max_key: Duration::from_nanos(MAX_NS.load(Ordering::Relaxed)),
        ..Default::default()
    };
    if let Some(s) = SAMPLES.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        snap.memory = s.memory;
        snap.cpu = s.cpu.back().copied().unwrap_or(0.0);
        snap.cpu_history = s.cpu.iter().copied().collect();
        snap.uptime = s.started.elapsed();
    }
    snap
}

// ---------------------------------------------------------------- typing statistics

/// Today's typing: only counts, never what was typed.
#[derive(Default)]
struct Typing {
    /// local date as yyyymmdd
    day: u32,
    words: u64,
    best_wpm: u64,
    macros: std::collections::BTreeMap<String, u64>,
    in_word: bool,
    /// end times of the words of the last minute
    recent: VecDeque<Instant>,
    dirty: bool,
}

static TYPING: Mutex<Option<Typing>> = Mutex::new(None);

fn typing<R>(f: impl FnOnce(&mut Typing) -> R) -> R {
    f(TYPING.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Typing::default))
}

/// A character was typed (letters and digits make words; anything else ends one).
pub fn record_char(c: char) {
    typing(|t| {
        if c.is_alphanumeric() {
            t.in_word = true;
            return;
        }
        if !t.in_word {
            return;
        }
        t.in_word = false;
        t.words += 1;
        let now = Instant::now();
        t.recent.push_back(now);
        while t.recent.front().is_some_and(|&w| now.duration_since(w) > Duration::from_secs(60)) {
            t.recent.pop_front();
        }
        t.best_wpm = t.best_wpm.max(t.recent.len() as u64);
        t.dirty = true;
    });
}

pub fn record_macro(name: &str) {
    typing(|t| {
        *t.macros.entry(name.to_string()).or_default() += 1;
        t.dirty = true;
    });
}

/// Start a new day's counts when the local date changes.
pub fn set_day(day: u32) {
    typing(|t| {
        if t.day != day {
            *t = Typing { day, dirty: true, ..Default::default() };
        }
    });
}

#[derive(Default, Clone)]
pub struct TypingSnapshot {
    pub words: u64,
    /// words in the last 60 s
    pub wpm: u64,
    pub best_wpm: u64,
    /// most used macros, most first
    pub top_macros: Vec<(String, u64)>,
}

pub fn typing_snapshot() -> TypingSnapshot {
    typing(|t| {
        let now = Instant::now();
        let mut top: Vec<_> = t.macros.iter().map(|(k, &v)| (k.clone(), v)).collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top.truncate(3);
        TypingSnapshot {
            words: t.words,
            wpm: t.recent.iter().filter(|&&w| now.duration_since(w) <= Duration::from_secs(60)).count() as u64,
            best_wpm: t.best_wpm,
            top_macros: top,
        }
    })
}

/// "vn ×12, btw ×3"
pub fn top_macros(top: &[(String, u64)]) -> String {
    if top.is_empty() {
        return "—".into();
    }
    top.iter().map(|(name, n)| format!("{name} ×{n}")).collect::<Vec<_>>().join(", ")
}

/// Saved form (day, words, best speed, macro counts), if it changed since the last call.
pub fn take_saved_typing() -> Option<Vec<u8>> {
    typing(|t| {
        if !t.dirty {
            return None;
        }
        t.dirty = false;
        let mut s = format!("{}\n{}\n{}\n", t.day, t.words, t.best_wpm);
        for (name, n) in &t.macros {
            s.push_str(&format!("{name}\t{n}\n"));
        }
        Some(s.into_bytes())
    })
}

/// Restore today's counts saved by `take_saved_typing` (ignored if from another day).
pub fn load_typing(bytes: &[u8], today: u32) {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.lines();
    let mut num = || lines.next().and_then(|l| l.parse::<u64>().ok());
    let (Some(day), Some(words), Some(best)) = (num(), num(), num()) else { return };
    if day as u32 != today {
        return;
    }
    let macros = text.lines().skip(3).filter_map(|l| l.split_once('\t')).filter_map(|(k, v)| Some((k.to_string(), v.parse().ok()?))).collect();
    typing(|t| *t = Typing { day: today, words, best_wpm: best, macros, ..Default::default() });
}

// ---------------------------------------------------------------- formatting (Vietnamese)

/// 12345 -> "12.345"
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

pub fn memory(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0).replace('.', ",")
}

pub fn percent(p: f32) -> String {
    format!("{p:.1} %").replace('.', ",")
}

pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    match (s / 86400, s / 3600 % 24, s / 60 % 60) {
        (0, 0, 0) => format!("{s} giây"),
        (0, 0, m) => format!("{m} phút"),
        (0, h, m) => format!("{h} giờ {m} phút"),
        (d, h, _) => format!("{d} ngày {h} giờ"),
    }
}

/// Key handling time, in µs below 1 ms.
pub fn latency(d: Duration) -> String {
    let us = d.as_secs_f64() * 1e6;
    if us < 1000.0 {
        format!("{us:.0} µs")
    } else {
        format!("{:.1} ms", us / 1000.0).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234567), "1.234.567");
        assert_eq!(memory(19 * 1_048_576 + 524_288), "19,5 MB");
        assert_eq!(percent(0.34), "0,3 %");
        assert_eq!(duration(Duration::from_secs(3 * 3600 + 25 * 60)), "3 giờ 25 phút");
        assert_eq!(latency(Duration::from_nanos(4_200)), "4 µs");
    }

    #[test]
    fn counts_words_and_macros_per_day() {
        set_day(20261029);
        for c in "xin chào, việt nam\n".chars() {
            record_char(c);
        }
        record_macro("vn");
        record_macro("vn");
        record_macro("btw");
        let s = typing_snapshot();
        assert_eq!(s.words, 4);
        assert_eq!(s.wpm, 4);
        assert_eq!(top_macros(&s.top_macros), "vn ×2, btw ×1");
        let saved = take_saved_typing().unwrap();
        assert!(take_saved_typing().is_none(), "saved only when changed");
        set_day(20261030);
        assert_eq!(typing_snapshot().words, 0, "a new day starts from zero");
        load_typing(&saved, 20261030);
        assert_eq!(typing_snapshot().words, 0, "yesterday's counts are not restored");
        load_typing(&saved, 20261029);
        assert_eq!(typing_snapshot().words, 4);
    }

    #[test]
    fn cpu_percent_from_samples() {
        // 25 ms of CPU over 100 ms of wall time
        let t = Instant::now();
        sample_at(t, Duration::from_millis(0), 10);
        sample_at(t + Duration::from_millis(100), Duration::from_millis(25), 20);
        let s = snapshot();
        assert_eq!(s.memory, 20);
        assert!((s.cpu - 25.0).abs() < 0.01, "{}", s.cpu);
    }
}
