//! Find the "longest" trains in the package: the most stops, the longest
//! journey (calendar span), and the longest operating window.
//!
//! Run: `cargo run --release --example longest`

use llt::Timetable;

struct Record {
    name: String,
    index: usize,
    stops: usize,
    last_day: u8,
    minutes: u64,
    origin: String,
    terminal: String,
    start: u32,
    end: u32,
}

impl Record {
    fn duration(&self) -> String {
        let days = self.minutes / 1440;
        let (hours, minutes) = ((self.minutes % 1440) / 60, self.minutes % 60);
        format!("{days}d {hours:02}:{minutes:02}")
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i32, month: u32, day: u32) -> i64 {
    let y = y as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn window_days(start: u32, end: u32) -> i64 {
    let civil = |ymd: u32| days_from_civil((ymd / 10000) as i32, ymd / 100 % 100, ymd % 100);
    civil(end) - civil(start)
}

fn top3<'a>(records: &'a [Record], key: impl Fn(&Record) -> i64) -> Vec<&'a Record> {
    let mut keyed: Vec<(i64, &'a Record)> =
        records.iter().map(|record| (key(record), record)).collect();
    keyed.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    keyed
        .into_iter()
        .map(|(_, record)| record)
        .take(3)
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let timetable = Timetable::parse(&package)?;

    let mut records = Vec::new();
    for train_index in 0..timetable.trains().len() {
        let Some(train) = timetable.train(train_index)? else {
            continue;
        };
        let (Some(first), Some(last)) = (train.stops.first(), train.stops.last()) else {
            continue;
        };
        let minutes = (last.day as u64 * 1440 + last.hour as u64 * 60 + last.minute as u64)
            - (first.hour as u64 * 60 + first.minute as u64);
        records.push(Record {
            name: timetable.trains()[train_index].clone(),
            index: train_index,
            stops: train.stops.len(),
            last_day: last.day,
            minutes,
            origin: timetable.stations()[first.station].clone(),
            terminal: timetable.stations()[last.station].clone(),
            start: train.start_date,
            end: train.end_date,
        });
    }

    println!("parsed {} trains\n", records.len());

    println!("most stops:");
    for (i, record) in top3(&records, |record| record.stops as i64)
        .into_iter()
        .enumerate()
    {
        println!(
            "  {}. {} (idx {}) — {} stops, {} -> {}, duration {}",
            i + 1,
            record.name,
            record.index,
            record.stops,
            record.origin,
            record.terminal,
            record.duration()
        );
    }

    println!("\nlongest journey (calendar span):");
    for (i, record) in top3(&records, |record| record.minutes as i64)
        .into_iter()
        .enumerate()
    {
        println!(
            "  {}. {} (idx {}) — {} stops, {} -> {}, crosses {} midnight(s), duration {}",
            i + 1,
            record.name,
            record.index,
            record.stops,
            record.origin,
            record.terminal,
            record.last_day,
            record.duration()
        );
    }

    println!("\nlongest operating window:");
    for (i, record) in top3(&records, |record| window_days(record.start, record.end))
        .into_iter()
        .enumerate()
    {
        println!(
            "  {}. {} (idx {}) — {} .. {} ({} days)",
            i + 1,
            record.name,
            record.index,
            record.start,
            record.end,
            window_days(record.start, record.end)
        );
    }

    // Full stop lists for the two winners most useful in tests.
    type RecordKey = fn(&Record) -> usize;
    let picks: [(&str, RecordKey); 2] = [
        ("most stops", |record| record.stops),
        ("longest journey", |record| record.last_day as usize),
    ];
    for (label, key) in picks {
        let winner = records
            .iter()
            .max_by_key(|record| key(record))
            .ok_or("no trains in offline data package")?;
        println!(
            "\n=== {label}: {} (idx {}) — {} stops, {} -> {} ===",
            winner.name, winner.index, winner.stops, winner.origin, winner.terminal
        );
        let train = timetable
            .train(winner.index)?
            .ok_or("train record missing")?;
        for stop in &train.stops {
            println!(
                "  {} {:02}:{:02}  day={}  dwell={}  dist={}",
                timetable.stations()[stop.station],
                stop.hour,
                stop.minute,
                stop.day,
                stop.dwell,
                stop.dist
            );
        }
    }
    Ok(())
}
