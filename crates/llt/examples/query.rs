//! Station-to-station query with per-train consist (交路) enrichment: each
//! result row shows the timetable segment plus the vehicle model, rotation
//! validity and dining-coach placement from jlb.dat.
//!
//! Run: `cargo run --release --example query -- 北京南 上海虹桥 [YYYY-MM-DD]`

use llt::{Date, Timetable};

fn consists_on(timetable: &Timetable, names: &[&str], ymd: u32) -> Vec<String> {
    let mut consists = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in names {
        for consist in timetable.consists_for(name) {
            if consist.start_date <= ymd
                && ymd <= consist.end_date
                && seen.insert(consist.model.as_str())
            {
                consists.push(format!("{} [{}~]", consist.model, consist.start_date));
            }
        }
        if !consists.is_empty() {
            break;
        }
    }
    consists
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let from_name = args.next().unwrap_or_else(|| "北京南".to_string());
    let to_name = args.next().unwrap_or_else(|| "上海虹桥".to_string());
    let (year, month, day) = match args.next() {
        Some(raw_date) => {
            let parts: Vec<&str> = raw_date.split('-').collect();
            let [year, month, day] = parts.as_slice() else {
                return Err("expected date in YYYY-MM-DD format".into());
            };
            (year.parse()?, month.parse()?, day.parse()?)
        }
        None => (2026, 8, 31),
    };
    let date = Date::new(year, month, day);
    let ymd = year as u32 * 10000 + month as u32 * 100 + day as u32;

    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let timetable = Timetable::parse(&package)?;

    let (Some(from), Some(to)) = (
        timetable.station_index(&from_name),
        timetable.station_index(&to_name),
    ) else {
        eprintln!("station not found: {from_name} / {to_name}");
        std::process::exit(1);
    };

    let rows = timetable.query(from, to, date)?;
    println!(
        "{from_name} -> {to_name}  {year:04}-{month:02}-{day:02}: {} trains running",
        rows.len()
    );
    println!();
    for (i, segment) in rows.iter().enumerate() {
        let name = &timetable.trains()[segment.train];
        let train = timetable
            .train(segment.train)?
            .ok_or("train record missing")?;
        let dep = &train.stops[segment.from_stop];
        let arr = &train.stops[segment.to_stop];
        let departure = dep.day as i64 * 1440
            + dep.hour as i64 * 60
            + dep.minute as i64
            + if segment.from_stop == 0 {
                0
            } else {
                dep.dwell as i64
            };
        let arrival = arr.day as i64 * 1440 + arr.hour as i64 * 60 + arr.minute as i64;
        let days = arrival / 1440 - departure / 1440;
        let dur_min = arrival - departure;
        let stops_between = segment.to_stop - segment.from_stop - 1;

        // Consists running this train on the query date (交路). Cross-day
        // names (Z295/Z298) may be stored in rotations as separate legs, so
        // fall back to the name parts.
        let mut candidate_names = vec![name.as_str()];
        if let Some((first_name, second_name)) = name.split_once('/') {
            candidate_names.push(first_name);
            candidate_names.push(second_name);
        }
        let consists = consists_on(&timetable, &candidate_names, ymd);
        // Dining-coach placement valid on the query date.
        let mut dining = Vec::new();
        for dcar in timetable.dining_cars(segment.train) {
            if dcar.start_date <= ymd && ymd <= dcar.end_date {
                dining.push(dcar.coach.as_str());
            }
        }

        println!(
            "#{:<3} {:<12} {:02}:{:02} -> {:02}:{:02}{:+}  {:>2} stops  {:>2}h{:02}m",
            i + 1,
            name,
            departure.rem_euclid(1440) / 60,
            departure.rem_euclid(60),
            arr.hour,
            arr.minute,
            days,
            stops_between,
            dur_min / 60,
            dur_min % 60
        );
        if !consists.is_empty() {
            println!("      车底: {}", consists.join(" | "));
        }
        if !dining.is_empty() {
            println!("      餐车: {}", dining.join(", "));
        }
    }
    Ok(())
}
