//! Footprint benchmark for llt against the real an.db package.
//!
//! Reports wall-clock time (min/median) and allocator-level memory
//! (live bytes after the phase, peak bytes during the phase) for:
//! loading (disk read + parse) and querying (lookups, shard scans,
//! train decoding, OD queries).
//!
//! Run in release mode: `cargo run --release --bin bench`

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use llt::{Date, Timetable};

/// System allocator wrapped with live/peak byte counters.
#[global_allocator]
static ALLOC: Tracking = Tracking;

struct Tracking;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: Allocations and deallocations use System with the caller's original
// pointer and layout. Accounting uses atomics and does not allocate or unwind.
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller guarantees that layout is valid.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let cur = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(cur, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: The caller supplies a live System allocation and its layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Start counting the next phase's peak from the current live bytes.
fn reset_peak() {
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn fmt_bytes(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.2} MB", n as f64 / 1048576.0)
    } else if n >= 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

fn fmt_dur(duration: Duration) -> String {
    let us = duration.as_nanos() as f64 / 1000.0;
    if us < 1000.0 {
        format!("{us:.0} µs")
    } else if us < 1_000_000.0 {
        format!("{:.2} ms", us / 1000.0)
    } else {
        format!("{:.2} s", us / 1_000_000.0)
    }
}

fn median(durations: &mut [Duration]) -> Duration {
    durations.sort();
    durations[durations.len() / 2]
}

fn minimum(durations: &[Duration]) -> Duration {
    #[expect(
        clippy::unwrap_used,
        reason = "every benchmark phase records a fixed, nonzero number of samples"
    )]
    durations.iter().copied().min().unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        eprintln!(
            "WARNING: debug build — run `cargo run --release --bin bench` for meaningful numbers"
        );
    }
    let path = std::env::var("LLT_PACKAGE")?;

    println!("llt footprint bench");
    println!(
        "  build: {}  os={} arch={}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    // ---- Phase 1: read the package from disk -------------------------------
    let mut read_times = Vec::new();
    let mut package: Vec<u8> = Vec::new();
    let mut read_peak = 0;
    for _ in 0..7 {
        drop(std::mem::take(&mut package));
        reset_peak();
        let t0 = Instant::now();
        package = std::fs::read(&path)?;
        read_times.push(t0.elapsed());
        read_peak = peak();
    }
    println!("  package: {} bytes on disk ({path})", package.len());
    row(
        "read package from disk",
        &fmt_dur(minimum(&read_times)),
        &fmt_dur(median(&mut read_times)),
        &fmt_bytes(read_peak),
        &fmt_bytes(live()),
    );

    // ---- Phase 2: parse ----------------------------------------------------
    let mut parse_times = Vec::new();
    for _ in 0..7 {
        let t0 = Instant::now();
        let parsed = Timetable::parse(&package)?;
        parse_times.push(t0.elapsed());
        drop(parsed);
    }
    reset_peak();
    let tt = Timetable::parse(&package)?;
    let parse_retained_peak = peak();
    drop(package);
    let retained = live();
    row(
        "Timetable::parse (hot cache)",
        &fmt_dur(minimum(&parse_times)),
        &fmt_dur(median(&mut parse_times)),
        &format!("{} (incl. package)", fmt_bytes(parse_retained_peak)),
        &fmt_bytes(retained),
    );
    println!("    retained = the parsed tables alone (package bytes dropped)");

    // ---- Phase 3: name lookups ---------------------------------------------
    let names: Vec<&str> = tt.stations().iter().map(|name| name.as_str()).collect();
    let mut lookup_times = Vec::new();
    for _ in 0..5 {
        reset_peak();
        let t0 = Instant::now();
        let mut found = 0usize;
        for name in &names {
            if tt.station_index(name).is_some() {
                found += 1;
            }
        }
        std::hint::black_box(found);
        lookup_times.push(t0.elapsed());
    }
    row(
        "station_index: all 3310 names",
        &fmt_dur(minimum(&lookup_times)),
        &fmt_dur(median(&mut lookup_times)),
        &fmt_bytes(peak()),
        &fmt_bytes(live()),
    );

    // ---- Phase 4: trains_through for every station --------------------------
    let mut through_times = Vec::new();
    for _ in 0..5 {
        reset_peak();
        let t0 = Instant::now();
        let total = (0..tt.stations().len()).try_fold(0usize, |total, station| {
            tt.trains_through(station)
                .map(|trains| total + trains.len())
        })?;
        std::hint::black_box(total);
        through_times.push(t0.elapsed());
    }
    row(
        "trains_through: all 3310 stations",
        &fmt_dur(minimum(&through_times)),
        &fmt_dur(median(&mut through_times)),
        &fmt_bytes(peak()),
        &fmt_bytes(live()),
    );

    // ---- Phase 5: decode every train (worst-case full sweep) ----------------
    let train_count = tt.trains().len();
    let mut decode_times = Vec::new();
    for _ in 0..5 {
        reset_peak();
        let t0 = Instant::now();
        let stops = (0..train_count).try_fold(0usize, |total, train| {
            tt.train(train).map(|train| {
                total + train.map_or(0, |timetable_record| timetable_record.stops.len())
            })
        })?;
        std::hint::black_box(stops);
        decode_times.push(t0.elapsed());
    }
    row(
        &format!("train(): decode all {train_count} trains"),
        &fmt_dur(minimum(&decode_times)),
        &fmt_dur(median(&mut decode_times)),
        &fmt_bytes(peak()),
        &fmt_bytes(live()),
    );

    // ---- Phase 6: OD queries ------------------------------------------------
    println!();
    println!("queries (25 timed runs each, after 3 warmups; steady state = retained tables)");
    println!(
        "  {:<28} {:>6}  {:>10}  {:>10}  {:>10}  {:>9}",
        "from -> to", "rows", "min", "median", "q/s", "peak"
    );
    let od_pairs = [
        ("北京南", "上海虹桥", Some(40)), // golden: exactly 40 running
        ("北京西", "武昌", None),
        ("北京西", "广州", None),
        ("成都西", "北京西", None),
    ];
    for (from_name, to_name, expected_rows) in od_pairs {
        let (Some(from), Some(to)) = (tt.station_index(from_name), tt.station_index(to_name))
        else {
            println!("  {from_name} -> {to_name}: station not found, skipped");
            continue;
        };
        let date = Date::new(2026, 8, 31);
        for _ in 0..3 {
            std::hint::black_box(tt.query(from, to, date)?);
        }
        let mut times = Vec::new();
        for _ in 0..25 {
            let t0 = Instant::now();
            let rows = tt.query(from, to, date)?;
            times.push(t0.elapsed());
            std::hint::black_box(&rows);
        }
        reset_peak();
        let rows = tt.query(from, to, date)?;
        let one_peak = peak();
        if let Some(expected) = expected_rows {
            assert_eq!(
                rows.len(),
                expected,
                "golden row count mismatch for {from_name} -> {to_name}"
            );
        }
        let min = minimum(&times);
        let med = median(&mut times);
        let qps = 1e9 / med.as_nanos() as f64;
        println!(
            "  {:<28} {:>6}  {:>10}  {:>10}  {:>9.0}  {:>9}",
            format!("{from_name} -> {to_name}"),
            rows.len(),
            fmt_dur(min),
            fmt_dur(med),
            qps,
            fmt_bytes(one_peak),
        );
    }

    println!();
    println!(
        "  steady-state memory: {} live in Timetable (allocator-level bytes; \
         proxy for WASM linear memory / native heap, excludes malloc metadata)",
        fmt_bytes(retained)
    );
    Ok(())
}

fn row(name: &str, min: &str, median: &str, peak: &str, retained: &str) {
    println!(
        "  {name:<38} min {min:>10}   median {median:>10}   peak {peak:>9}   live {retained:>9}"
    );
}
