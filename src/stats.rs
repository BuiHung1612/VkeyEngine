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
    let mut g = SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
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
    fn cpu_percent_from_samples() {
        sample(Duration::from_millis(0), 10);
        std::thread::sleep(Duration::from_millis(50));
        sample(Duration::from_millis(25), 20);
        let s = snapshot();
        assert_eq!(s.memory, 20);
        assert!(s.cpu > 20.0 && s.cpu < 60.0, "{}", s.cpu);
    }
}
