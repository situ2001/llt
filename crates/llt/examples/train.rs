//! Dump the full structural data the library holds for one train, using
//! only the public API.
//!
//! Run: `cargo run --release --example train -- G1`

use llt::{Date, Timetable};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "G1".to_string());
    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let timetable = Timetable::parse(&package)?;

    let Some(train_index) = timetable.train_index(&name) else {
        eprintln!("train {name} not found");
        std::process::exit(1);
    };
    let train = timetable
        .train(train_index)?
        .ok_or("train record missing")?;

    println!("=== {name} (train index {train_index}) ===");
    println!("12306 train_no : {:?}", timetable.train_no(train_index));
    println!("kind byte      : {}", train.kind);
    println!(
        "runs           : {} .. {}",
        train.start_date, train.end_date
    );
    println!(
        "rule           : index {} -> {:?}",
        train.rule,
        timetable.runs_on(&train, Date::new(2026, 8, 31))?
    );
    if let Some(province) = timetable.home_province(train_index) {
        println!("home province  : {province}");
    }

    // ---- timetable (T shard) ----
    println!("\n-- stops ({}) --", train.stops.len());
    for (i, stop) in train.stops.iter().enumerate() {
        println!(
            "  {:2}  {:<12} {:02}:{:02}  day+{}  dwell {:>2} min  dist {:>4} km",
            i,
            timetable.stations()[stop.station],
            stop.hour,
            stop.minute,
            stop.day,
            stop.dwell,
            stop.dist
        );
    }

    // ---- display name switch points (xw.dat) ----
    match (
        timetable.display_flags(train_index),
        timetable.display_name_at(train_index, 0),
    ) {
        (Some(flags), Some(first)) => {
            println!("\n-- display names (flags 0x{flags:04x}) --");
            let mut last = 0usize;
            for stop in 0..train.stops.len() {
                let Some(dn) = timetable.display_name_at(train_index, stop) else {
                    continue;
                };
                if stop == 0 || timetable.display_name_at(train_index, last) != Some(dn) {
                    println!("  from stop {stop:>3}: {dn}");
                }
                last = stop;
            }
            let _ = first;
        }
        (Some(flags), None) => println!("\n-- display: uniform (flags 0x{flags:04x}) --"),
        _ => println!("\n-- display: no xw record --"),
    }

    // ---- walking route (routes.dat) ----
    match timetable.route_key_exact(train_index).and_then(|key| {
        timetable
            .expand_route(key)
            .map(|route_nodes| (key, route_nodes))
    }) {
        Some((key, nodes)) => {
            println!("\n-- route ({key}, {} nodes) --", nodes.len());
            for n in &nodes {
                println!("  {:<12} {:<18} {:>5} km", n.station, n.lines, n.dist);
            }
        }
        None => println!("\n-- route: none --"),
    }

    // ---- attributes (extra.dat) ----
    println!("\n-- attributes --");
    println!("  agency      : {:?}", timetable.agency(train_index));
    let groups_mask = timetable.fuxing_groups(train_index);
    if groups_mask != 0 {
        let mut groups = Vec::new();
        if groups_mask & 1 != 0 {
            groups.push("1");
        }
        if groups_mask & 2 != 0 {
            groups.push("2");
        }
        if groups_mask & 4 != 0 {
            groups.push("3");
        }
        println!("  fuxing      : group {}", groups.join(" + "));
    }
    let turn = timetable.turnaround(train_index);
    if !turn.is_empty() {
        let names: Vec<&str> = turn
            .iter()
            .map(|&station_index| timetable.stations()[station_index].as_str())
            .collect();
        println!("  turnaround  : {}", names.join(", "));
    }
    println!("  S-train     : {}", timetable.is_s_train(train_index));

    // ---- consists (jlb.dat) ----
    let consists = timetable.consists_for(&name);
    println!("\n-- consists ({}) --", consists.len());
    for consist in consists {
        println!(
            "  [{} ~ {}] {} — model: {}",
            consist.start_date,
            consist.end_date,
            timetable.units()[consist.agency],
            consist.model
        );
        println!("    rotation: {}", consist.rotation);
    }

    // ---- dining cars (jlb.dat) ----
    let foods = timetable.dining_cars(train_index);
    println!("\n-- dining cars ({}) --", foods.len());
    for placement in &foods {
        println!(
            "  [{} ~ {}] {}",
            placement.start_date, placement.end_date, placement.coach
        );
    }
    Ok(())
}
