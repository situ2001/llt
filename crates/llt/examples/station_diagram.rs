//! Query trains stopping at a station and emit the data needed for a full
//! time-distance running diagram.
//!
//! The example follows the same public API that the application uses:
//! station name -> trains through the station -> decoded train timetable ->
//! exact train route. With no `--train`, it lists trains running on the query
//! date. With `--train`, it prints every timetable stop and every expanded
//! route node, including non-stop passing points.
//!
//! Examples:
//!
//! ```text
//! cargo run --release --example station_diagram -- 北京南 --date 2026-08-31
//! cargo run --release --example station_diagram -- 北京南 --train G1
//! cargo run --release --example station_diagram -- 北京南 --train G1 --format csv
//! cargo run --release --example station_diagram -- 北京南 --all --format csv
//! ```

use llt::{Date, ParseError, RouteNode, RunStatus, Stop, Timetable, Train};

const USAGE: &str = "usage: station_diagram <STATION> [--date YYYY-MM-DD] [--train TRAIN] [--all] [--format text|csv|json]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Csv,
    Json,
}

struct Options {
    station: String,
    date: Date,
    train: Option<String>,
    all: bool,
    format: Format,
}

struct Candidate {
    train: usize,
    stop: usize,
    timetable: Train,
}

struct Route {
    key: String,
    nodes: Vec<RouteNode>,
}

fn default_date() -> Date {
    Date::new(2026, 8, 31)
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
        let mut station = None;
        let mut date = default_date();
        let mut train = None;
        let mut all = false;
        let mut format = Format::Text;
        let mut args = args;

        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                println!("{USAGE}");
                println!("\nWithout --train, lists trains that call at STATION on the date.");
                println!("With --train, emits the timetable and expanded physical route.");
                println!("--all emits the full diagram for every matching train.");
                return Ok(None);
            }

            if arg == "--all" {
                all = true;
                continue;
            }

            if let Some(value) = option_value(&arg, "--date", &mut args)? {
                date = parse_date(&value)?;
                continue;
            }
            let train_value = option_value(&arg, "--train", &mut args)?;
            if train_value.is_some() && train.is_some() {
                return Err("--train may only be provided once".to_string());
            }
            if let Some(value) = train_value {
                train = Some(value);
                continue;
            }
            if let Some(value) = option_value(&arg, "--format", &mut args)? {
                format = match value.as_str() {
                    "text" => Format::Text,
                    "csv" => Format::Csv,
                    "json" => Format::Json,
                    _ => return Err(format!("unknown format `{value}`; use text, csv, or json")),
                };
                continue;
            }

            if arg.starts_with('-') {
                return Err(format!("unknown argument `{arg}`\n{USAGE}"));
            }
            if station.replace(arg).is_some() {
                return Err(format!("only one station is expected\n{USAGE}"));
            }
        }

        let Some(station) = station else {
            return Err(USAGE.to_string());
        };
        if all && train.is_some() {
            return Err("use either --train or --all, not both".to_string());
        }

        Ok(Some(Options {
            station,
            date,
            train,
            all,
            format,
        }))
    }
}

/// Parse a flag that is either `--flag value` or `--flag=value`.
fn option_value(
    arg: &str,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> Result<Option<String>, String> {
    if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
        if value.is_empty() {
            return Err(format!("missing value after {flag}"));
        }
        return Ok(Some(value.to_string()));
    }
    if arg == flag {
        return args
            .next()
            .ok_or_else(|| format!("missing value after {flag}"))
            .map(Some);
    }
    Ok(None)
}

fn parse_date(raw: &str) -> Result<Date, String> {
    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("invalid date `{raw}`; expected YYYY-MM-DD"));
    }
    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| format!("invalid year in date `{raw}`"))?;
    let month = parts[1]
        .parse::<u8>()
        .map_err(|_| format!("invalid month in date `{raw}`"))?;
    let day = parts[2]
        .parse::<u8>()
        .map_err(|_| format!("invalid day in date `{raw}`"))?;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    };
    if !(1..=12).contains(&month) || !(1..=max_day).contains(&day) {
        return Err(format!("invalid date `{raw}`; expected a calendar date"));
    }
    Ok(Date::new(year, month, day))
}

fn date_string(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn stop_minutes(stop: &Stop) -> i64 {
    stop.day as i64 * 1440 + stop.hour as i64 * 60 + stop.minute as i64
}

/// A stop stores arrival time; the first stop stores origin departure.
/// Adding dwell may move an intermediate departure into the next day.
fn arrival_minutes(train: &Train, index: usize, stop: &Stop) -> Option<i64> {
    if train.stops.len() == 1 || index > 0 {
        Some(stop_minutes(stop))
    } else {
        None
    }
}

fn departure_minutes(train: &Train, index: usize, stop: &Stop) -> Option<i64> {
    if train.stops.len() == 1 || index + 1 < train.stops.len() {
        Some(stop_minutes(stop) + if index == 0 { 0 } else { stop.dwell as i64 })
    } else {
        None
    }
}

fn clock(minutes: i64) -> String {
    let minutes = minutes.rem_euclid(1440);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn time_string(minutes: Option<i64>) -> String {
    let Some(minutes) = minutes else {
        return "-".to_string();
    };
    let day = minutes.div_euclid(1440);
    if day == 0 {
        clock(minutes)
    } else {
        format!("+{day} {}", clock(minutes))
    }
}

fn running_candidates(
    tt: &Timetable,
    station: usize,
    date: Date,
) -> Result<Vec<Candidate>, ParseError> {
    let mut candidates = Vec::new();
    for train in tt.trains_through(station)? {
        let Some(timetable) = tt.train(train)? else {
            continue;
        };
        if tt.runs_on(&timetable, date)? != RunStatus::Runs {
            continue;
        }
        if let Some((stop, _)) = timetable
            .stops
            .iter()
            .enumerate()
            .find(|(_, stop)| stop.station == station)
        {
            candidates.push(Candidate {
                train,
                stop,
                timetable,
            });
        }
    }
    candidates.sort_by(|left, right| {
        let left_timetable = &left.timetable;
        let right_timetable = &right.timetable;
        let left_time =
            departure_minutes(left_timetable, left.stop, &left_timetable.stops[left.stop])
                .unwrap_or_else(|| stop_minutes(&left_timetable.stops[left.stop]));
        let right_time = departure_minutes(
            right_timetable,
            right.stop,
            &right_timetable.stops[right.stop],
        )
        .unwrap_or_else(|| stop_minutes(&right_timetable.stops[right.stop]));
        left_time
            .cmp(&right_time)
            .then_with(|| tt.trains()[left.train].cmp(&tt.trains()[right.train]))
    });
    Ok(candidates)
}

fn route_key_for_train(tt: &Timetable, train: usize) -> Option<String> {
    let name = &tt.trains()[train];
    tt.route_key_exact(train).map(str::to_owned).or_else(|| {
        let keys = tt.route_keys_for(name);
        (keys.len() == 1).then(|| keys[0].to_string())
    })
}

fn route_for_train(tt: &Timetable, train: usize) -> Option<Route> {
    let key = route_key_for_train(tt, train)?;
    let nodes = tt.expand_route(&key)?;
    Some(Route { key, nodes })
}

/// Match timetable stops to route nodes in order. Route nodes may include
/// non-stop passing points, and a route can contain a repeated station.
fn route_matches<'a>(
    stops: &[Stop],
    stations: &'a [String],
    route: &'a [RouteNode],
) -> Vec<Option<&'a RouteNode>> {
    let mut cursor = 0usize;
    stops
        .iter()
        .map(|stop| {
            let name = &stations[stop.station];
            let found = route
                .iter()
                .enumerate()
                .skip(cursor)
                .find(|(_, node)| node.station == *name);
            if let Some((index, node)) = found {
                cursor = index + 1;
                Some(node)
            } else {
                None
            }
        })
        .collect()
}

fn endpoint_names<'a>(tt: &'a Timetable, train: &Train) -> (&'a str, &'a str) {
    let first = train
        .stops
        .first()
        .map(|stop| tt.stations()[stop.station].as_str())
        .unwrap_or("-");
    let last = train
        .stops
        .last()
        .map(|stop| tt.stations()[stop.station].as_str())
        .unwrap_or("-");
    (first, last)
}

fn print_candidates_text(tt: &Timetable, station: &str, date: Date, candidates: &[Candidate]) {
    println!(
        "{station}  {}: {} running trains",
        date_string(date),
        candidates.len()
    );
    println!();
    println!("#    train          origin -> destination       at station  route");
    for (rank, candidate) in candidates.iter().enumerate() {
        let timetable = &candidate.timetable;
        let (origin, destination) = endpoint_names(tt, timetable);
        let stop = &timetable.stops[candidate.stop];
        let route = route_key_for_train(tt, candidate.train).unwrap_or_else(|| "-".to_string());
        println!(
            "{:>3}  {:<14} {:<8} -> {:<8}  {}  {}",
            rank + 1,
            tt.trains()[candidate.train],
            origin,
            destination,
            time_string(Some(stop_minutes(stop))),
            route
        );
    }
    if !candidates.is_empty() {
        println!();
        println!("Use --train TRAIN to print one full running diagram, or --all for all rows.");
    }
}

const CSV_HEADER: &str = "record_type,query_station,query_date,train,origin,destination,route_key,sequence,station,display_train,arrival_clock,departure_clock,arrival_min,departure_min,day_offset,dwell_min,train_dist_km,route_dist_km,route_lines,route_match";

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_row(values: impl IntoIterator<Item = String>) {
    println!(
        "{}",
        values
            .into_iter()
            .map(|value| csv_field(&value))
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn print_candidates_csv(tt: &Timetable, station: &str, date: Date, candidates: &[Candidate]) {
    println!("{CSV_HEADER}");
    for candidate in candidates {
        let timetable = &candidate.timetable;
        let (origin, destination) = endpoint_names(tt, timetable);
        let stop = &timetable.stops[candidate.stop];
        let route = route_for_train(tt, candidate.train);
        let route_key = route.as_ref().map(|route| route.key.as_str()).unwrap_or("");
        csv_row([
            "candidate".to_string(),
            station.to_string(),
            date_string(date),
            tt.trains()[candidate.train].clone(),
            origin.to_string(),
            destination.to_string(),
            route_key.to_string(),
            candidate.stop.to_string(),
            station.to_string(),
            tt.display_name_at(candidate.train, candidate.stop)
                .unwrap_or(tt.trains()[candidate.train].as_str())
                .to_string(),
            time_string(arrival_minutes(timetable, candidate.stop, stop)),
            time_string(departure_minutes(timetable, candidate.stop, stop)),
            arrival_minutes(timetable, candidate.stop, stop)
                .map(|minutes| minutes.to_string())
                .unwrap_or_default(),
            departure_minutes(timetable, candidate.stop, stop)
                .map(|minutes| minutes.to_string())
                .unwrap_or_default(),
            stop.day.to_string(),
            stop.dwell.to_string(),
            stop.dist.to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
        ]);
    }
}

fn print_train_text(tt: &Timetable, station: &str, date: Date, candidate: &Candidate) {
    let timetable = &candidate.timetable;
    let name = &tt.trains()[candidate.train];
    let (origin, destination) = endpoint_names(tt, timetable);
    let route = route_for_train(tt, candidate.train);
    let matches = route
        .as_ref()
        .map(|route| route_matches(&timetable.stops, tt.stations(), &route.nodes));

    println!("=== {name} ===");
    println!("query station  : {station}");
    println!("query date     : {}", date_string(date));
    println!("origin         : {origin}");
    println!("destination    : {destination}");
    println!(
        "running window : {} .. {}",
        timetable.start_date, timetable.end_date
    );
    println!(
        "route key      : {}",
        route
            .as_ref()
            .map(|route| route.key.as_str())
            .unwrap_or("none")
    );
    println!();
    println!(
        "-- timetable / running diagram points ({}) --",
        timetable.stops.len()
    );
    println!(
        "seq  station          arrival       departure     day  dwell  train km  route km  route line"
    );
    for (index, stop) in timetable.stops.iter().enumerate() {
        let arrival = arrival_minutes(timetable, index, stop);
        let departure = departure_minutes(timetable, index, stop);
        let route_node = matches
            .as_ref()
            .and_then(|matches| matches[index].as_ref().copied());
        println!(
            "{:>3}  {:<15} {:<13} {:<13} {:>4}  {:>5}  {:>8}  {:>8}  {}",
            index,
            tt.stations()[stop.station],
            time_string(arrival),
            time_string(departure),
            stop.day,
            stop.dwell,
            stop.dist,
            route_node
                .map(|node| node.dist.to_string())
                .unwrap_or_else(|| "-".to_string()),
            route_node.map(|node| node.lines.as_str()).unwrap_or("-")
        );
    }

    match route {
        Some(route) => {
            println!();
            println!(
                "-- expanded physical route ({} nodes) --",
                route.nodes.len()
            );
            println!("seq  station          route km  line(s)");
            for (index, node) in route.nodes.iter().enumerate() {
                println!(
                    "{:>3}  {:<15} {:>8}  {}",
                    index, node.station, node.dist, node.lines
                );
            }
            println!();
            println!(
                "route km is the distance supplied by routes.dat; train km is the train's cumulative timetable distance."
            );
        }
        None => {
            println!();
            println!("-- expanded physical route: unavailable for this train --");
            println!(
                "The timetable points above are still complete and can be plotted with train km."
            );
        }
    }
}

fn print_train_csv(tt: &Timetable, station: &str, date: Date, candidate: &Candidate) {
    let timetable = &candidate.timetable;
    let name = &tt.trains()[candidate.train];
    let (origin, destination) = endpoint_names(tt, timetable);
    let route = route_for_train(tt, candidate.train);
    let route_key = route
        .as_ref()
        .map(|route| route.key.clone())
        .unwrap_or_default();
    let matches = route
        .as_ref()
        .map(|route| route_matches(&timetable.stops, tt.stations(), &route.nodes));

    for (index, stop) in timetable.stops.iter().enumerate() {
        let arrival = arrival_minutes(timetable, index, stop);
        let departure = departure_minutes(timetable, index, stop);
        let route_node = matches
            .as_ref()
            .and_then(|matches| matches[index].as_ref().copied());
        csv_row([
            "stop".to_string(),
            station.to_string(),
            date_string(date),
            name.clone(),
            origin.to_string(),
            destination.to_string(),
            route_key.to_string(),
            index.to_string(),
            tt.stations()[stop.station].clone(),
            tt.display_name_at(candidate.train, index)
                .unwrap_or(name.as_str())
                .to_string(),
            arrival
                .map(|minutes| time_string(Some(minutes)))
                .unwrap_or_default(),
            departure
                .map(|minutes| time_string(Some(minutes)))
                .unwrap_or_default(),
            arrival
                .map(|minutes| minutes.to_string())
                .unwrap_or_default(),
            departure
                .map(|minutes| minutes.to_string())
                .unwrap_or_default(),
            stop.day.to_string(),
            stop.dwell.to_string(),
            stop.dist.to_string(),
            route_node
                .map(|node| node.dist.to_string())
                .unwrap_or_default(),
            route_node
                .map(|node| node.lines.clone())
                .unwrap_or_default(),
            route_node
                .map(|_| "true".to_string())
                .unwrap_or_else(|| "false".to_string()),
        ]);
    }

    if let Some(route) = route {
        for (index, node) in route.nodes.iter().enumerate() {
            csv_row([
                "route".to_string(),
                station.to_string(),
                date_string(date),
                name.clone(),
                origin.to_string(),
                destination.to_string(),
                route_key.to_string(),
                index.to_string(),
                node.station.clone(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string(),
                node.dist.to_string(),
                node.lines.clone(),
                "".to_string(),
            ]);
        }
    }
}

/// Emit the normalized shape consumed by the browser running diagram. Unlike
/// the CSV format, this intentionally contains only timetable stops; route
/// nodes are derived separately by the renderer from its OSM snapshots.
fn json_train(tt: &Timetable, candidate: &Candidate) -> serde_json::Value {
    let timetable = &candidate.timetable;
    let name = &tt.trains()[candidate.train];
    let (origin, destination) = endpoint_names(tt, timetable);
    let stops = timetable
        .stops
        .iter()
        .enumerate()
        .map(|(index, stop)| {
            serde_json::json!({
                "stationName": tt.stations()[stop.station],
                "arrivalMinutes": arrival_minutes(timetable, index, stop),
                "departureMinutes": departure_minutes(timetable, index, stop),
                "day": stop.day,
                "dwellMinutes": stop.dwell,
                "distanceKm": stop.dist,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "name": name,
        "displayName": tt.display_name_at(candidate.train, candidate.stop).unwrap_or(name),
        "trainNo": tt.train_no(candidate.train).unwrap_or_default(),
        "source": "llt",
        "originName": origin,
        "destinationName": destination,
        "stops": stops,
    })
}

fn print_trains_json(tt: &Timetable, candidates: &[&Candidate]) -> Result<(), serde_json::Error> {
    let records = candidates
        .iter()
        .map(|candidate| json_train(tt, candidate))
        .collect::<Vec<_>>();
    println!("{}", serde_json::to_string_pretty(&records)?);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let path = std::env::var("LLT_PACKAGE")?;
    let package = std::fs::read(&path)?;
    let tt = Timetable::parse(&package)?;
    let Some(station_index) = tt.station_index(&options.station) else {
        eprintln!("station not found: {}", options.station);
        std::process::exit(1);
    };

    let candidates = running_candidates(&tt, station_index, options.date)?;
    if options.train.is_none() && !options.all {
        match options.format {
            Format::Text => print_candidates_text(&tt, &options.station, options.date, &candidates),
            Format::Csv => print_candidates_csv(&tt, &options.station, options.date, &candidates),
            Format::Json => print_trains_json(&tt, &candidates.iter().collect::<Vec<_>>())?,
        }
        return Ok(());
    }

    let selected: Vec<&Candidate> = if let Some(name) = options.train.as_deref() {
        let Some(train) = tt.train_index(name) else {
            eprintln!("train not found: {name}");
            std::process::exit(1);
        };
        let Some(candidate) = candidates.iter().find(|candidate| candidate.train == train) else {
            eprintln!(
                "train {name} does not call at {} or does not run on {}",
                options.station,
                date_string(options.date)
            );
            std::process::exit(1);
        };
        vec![candidate]
    } else {
        candidates.iter().collect()
    };

    if selected.is_empty() {
        eprintln!(
            "no trains call at {} on {}",
            options.station,
            date_string(options.date)
        );
        std::process::exit(1);
    }
    if options.format == Format::Json {
        print_trains_json(&tt, &selected)?;
        return Ok(());
    }
    if options.format == Format::Csv {
        println!("{CSV_HEADER}");
    }
    for candidate in selected {
        match options.format {
            Format::Text => print_train_text(&tt, &options.station, options.date, candidate),
            Format::Csv => print_train_csv(&tt, &options.station, options.date, candidate),
            Format::Json => unreachable!("JSON is emitted before the per-train loop"),
        }
    }
    Ok(())
}
