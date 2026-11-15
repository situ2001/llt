//! Top 10 longest single-station dwell times across every stop of every
//! train in the package. Intermediate stops store their arrival time plus
//! the dwell length; departure is arrival + dwell and can cross midnight.
//!
//! Trains can be filtered by name prefix (case-insensitive):
//!
//! ```text
//! cargo run --release --example dwell -- --exclude Y
//! cargo run --release --example dwell -- --only G,D,C
//! cargo run --release --example dwell -- --only G --exclude C4064
//! ```
//!
//! Without arguments every train counts.

use llt::Timetable;

const TOP: usize = 10;
const USAGE: &str = "usage: dwell [--only PREFIX,PREFIX] [--exclude PREFIX,PREFIX]";

/// Train-name prefix filters: `only` whitelists, `exclude` blacklists
/// (exclude wins over only).
#[derive(Default)]
struct Filters {
    only: Vec<String>,
    exclude: Vec<String>,
}

impl Filters {
    /// `--only G,D` / `--exclude Y` — the value may also be attached with
    /// `=`; repeated flags accumulate.
    fn parse(args: impl Iterator<Item = String>) -> Result<Filters, String> {
        let mut filters = Filters::default();
        let mut args = args;
        while let Some(arg) = args.next() {
            let (flag, value) = match arg.split_once('=') {
                Some((flag_name, flag_value)) => {
                    (flag_name.to_string(), Some(flag_value.to_string()))
                }
                None => (arg.clone(), None),
            };
            let value = match value {
                Some(flag_value) => flag_value,
                None => args
                    .next()
                    .ok_or_else(|| format!("missing value after {flag}"))?,
            };
            let prefixes: Vec<String> = value
                .split(',')
                .map(|prefix| prefix.trim().to_uppercase())
                .filter(|prefix| !prefix.is_empty())
                .collect();
            match flag.as_str() {
                "--only" => filters.only.extend(prefixes),
                "--exclude" => filters.exclude.extend(prefixes),
                _ => return Err(format!("unknown argument `{arg}`\n{USAGE}")),
            }
        }
        Ok(filters)
    }

    fn keeps(&self, name: &str) -> bool {
        (self.only.is_empty() || self.only.iter().any(|prefix| name.starts_with(prefix)))
            && !self.exclude.iter().any(|prefix| name.starts_with(prefix))
    }
}

/// `HH:mm` for minutes counted from the departure day's midnight (may be
/// negative for a derived arrival before the departure day).
fn clock(mins: i64) -> String {
    format!(
        "{:02}:{:02}",
        mins.rem_euclid(1440) / 60,
        mins.rem_euclid(1440) % 60
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filters = match Filters::parse(std::env::args().skip(1)) {
        Ok(filters) => filters,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let timetable = Timetable::parse(&package)?;

    // (dwell, train index, stop index) — endpoints carry dwell 0 and drop out.
    let mut hits: Vec<(u8, usize, usize)> = Vec::new();
    let mut kept = 0usize;
    for train_index in 0..timetable.trains().len() {
        if !filters.keeps(&timetable.trains()[train_index]) {
            continue;
        }
        kept += 1;
        let Some(train) = timetable.train(train_index)? else {
            continue;
        };
        for (stop_index, stop) in train.stops.iter().enumerate() {
            if stop.dwell > 0 {
                hits.push((stop.dwell, train_index, stop_index));
            }
        }
    }
    // Dwell descending; train/stop index as a deterministic tie-break.
    hits.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });

    println!(
        "parsed {} trains ({} after filter), {} stops with a dwell\n",
        timetable.trains().len(),
        kept,
        hits.len()
    );
    println!("top {TOP} longest single-station dwell:");
    for (rank, &(dwell, train_index, stop_index)) in hits.iter().take(TOP).enumerate() {
        let train = timetable
            .train(train_index)?
            .ok_or("previously decoded train record missing")?;
        let stop = &train.stops[stop_index];
        let (Some(first), Some(last)) = (train.stops.first(), train.stops.last()) else {
            continue;
        };
        let arr = stop.day as i64 * 1440 + stop.hour as i64 * 60 + stop.minute as i64;
        let dep = arr + dwell as i64;
        println!(
            "  {:2}. {:<10} (idx {}) — {:>3} min at {}  (dep {} day+{}, arr {} day+{})  {} -> {}",
            rank + 1,
            timetable.trains()[train_index],
            train_index,
            dwell,
            timetable.stations()[stop.station],
            clock(dep),
            dep / 1440,
            clock(arr),
            arr / 1440,
            timetable.stations()[first.station],
            timetable.stations()[last.station],
        );
    }
    Ok(())
}
