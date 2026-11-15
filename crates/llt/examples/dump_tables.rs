//! Dump machine-readable TSV tables from the offline package for
//! cross-checking against the live 12306 API snapshots in
//! `12306-live-api/snapshots/`.
//!
//! Run: `cargo run --release --example dump_tables -- <cmd> [arg]`
//!
//! Commands:
//! - `trainnos` — train index, name, 12306 internal code (sp.dat)
//! - `stations` — station index, name (s.i)
//! - `aliases` — virtual station aliases, index 10000+ (sn.dat)
//! - `stops <NAME>` — one train's stops: order, station, time, day
//!   offset, dwell (min), cumulative distance (km)

use llt::Timetable;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "trainnos".to_string());
    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let timetable = Timetable::parse(&package)?;

    match cmd.as_str() {
        "trainnos" => {
            for (i, name) in timetable.trains().iter().enumerate() {
                println!("{}\t{}\t{}", i, name, timetable.train_no(i).unwrap_or("-"));
            }
        }
        "stations" => {
            for (i, name) in timetable.stations().iter().enumerate() {
                println!("{i}\t{name}");
            }
        }
        "aliases" => {
            for (i, name) in timetable.aliases().iter().enumerate() {
                println!("{}\t{}", i + 10000, name);
            }
        }
        "stops" => {
            let name = args.next().ok_or("train name required, e.g. G1")?;
            let Some(train_index) = timetable.train_index(&name) else {
                eprintln!("train {name} not found");
                std::process::exit(1);
            };
            let train = timetable
                .train(train_index)?
                .ok_or("train record missing")?;
            for (j, stop) in train.stops.iter().enumerate() {
                println!(
                    "{}\t{}\t{:02}:{:02}\t{}\t{}\t{}",
                    j + 1,
                    timetable.stations()[stop.station],
                    stop.hour,
                    stop.minute,
                    stop.day,
                    stop.dwell,
                    stop.dist
                );
            }
        }
        other => {
            eprintln!("unknown command {other}; use trainnos | stations | stops <NAME>");
            std::process::exit(2);
        }
    }
    Ok(())
}
