use llt::{Date, RunStatus, Timetable};
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::SimpleFileOptions};

fn index(names: &[&str]) -> Vec<u8> {
    let mut bytes = (names.len() as u16).to_be_bytes().to_vec();
    for name in names {
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());
    }
    bytes
}

fn date(mut value: u32) -> [u8; 4] {
    let mut bytes = [0; 4];
    for byte in &mut bytes {
        *byte = (value % 128) as u8;
        value /= 128;
    }
    bytes
}

fn short_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.push(value.len() as u8);
    bytes.extend_from_slice(value.as_bytes());
}

fn package(hour: u8) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mut member = |name: &str, data: &[u8]| {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    };
    member("s.i", &index(&["North", "South"]));
    member("t.i", &index(&["T1"]));
    for i in 0..10 {
        let record = match i {
            1 => vec![0, 0, 0, 1, 0, 0],
            2 => vec![0, 1, 0, 1, 0, 0],
            _ => vec![],
        };
        member(&format!("S{i}.dat"), &record);
    }
    let mut train = vec![0, 0, 1, 0, 0];
    train.extend_from_slice(&date(20260101));
    train.extend_from_slice(&date(20261231));
    train.extend_from_slice(&[0, 0, 0, 2]);
    train.extend_from_slice(&[0, 0, hour, 30, 0, 0, 0]);
    train.extend_from_slice(&[0, 1, 0, 15, 0, 0, 50]);
    for i in 0..20 {
        member(&format!("T{i}.dat"), if i == 1 { &train } else { &[] });
    }
    member("t.rule", b"1 1\n");
    let mut routes = Vec::new();
    routes.extend_from_slice(&1u32.to_be_bytes());
    routes.extend_from_slice(&1u32.to_be_bytes());
    short_string(&mut routes, "Line A");
    routes.extend_from_slice(&2u32.to_be_bytes());
    for (name, code, distance) in [("North", "NOR", 0u32), ("South", "SOU", 50u32)] {
        short_string(&mut routes, name);
        short_string(&mut routes, code);
        routes.extend_from_slice(&0u32.to_be_bytes());
        routes.extend_from_slice(&distance.to_be_bytes());
    }
    routes.extend_from_slice(&1u32.to_be_bytes());
    short_string(&mut routes, "SYNTHETIC01");
    short_string(&mut routes, "South");
    routes.extend_from_slice(&1u32.to_be_bytes());
    short_string(&mut routes, "North");
    routes.extend_from_slice(&1u32.to_be_bytes());
    member("routes.dat", &routes);
    member("extra.dat", &[0; 14]);
    member("jlb.dat", &[0; 8]);
    let mut references = vec![0; 4];
    references.extend_from_slice(&1u32.to_be_bytes());
    short_string(&mut references, "SYNTHETIC01");
    references.extend_from_slice(&[0; 14]);
    member("sp.dat", &references);
    member("xw.dat", &[]);
    member("sn.dat", &[0; 6]);
    zip.finish().unwrap().into_inner()
}

#[test]
fn finds_train_and_queries_its_timetable() {
    let timetable = Timetable::parse(&package(23)).unwrap();
    assert_eq!(timetable.stations(), &["North", "South"]);
    let train_index = timetable.train_index("T1").unwrap();
    let train = timetable.train(train_index).unwrap().unwrap();
    assert_eq!(train.stops.len(), 2);
    assert_eq!(train.stops[1].day, 1);
    let date = Date::new(2026, 5, 1);
    assert_eq!(timetable.runs_on(&train, date).unwrap(), RunStatus::Runs);
    let segments = timetable.query(0, 1, date).unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].train, train_index);
}

#[test]
fn resolves_the_declared_route() {
    let timetable = Timetable::parse(&package(23)).unwrap();
    let index = timetable.train_index("T1").unwrap();
    let key = timetable.route_key_exact(index).unwrap();
    let route = timetable.expand_route(key).unwrap();
    assert_eq!(
        route
            .iter()
            .map(|node| node.station.as_str())
            .collect::<Vec<_>>(),
        ["North", "South"]
    );
    assert_eq!(route[1].dist, 50);
}

#[test]
fn rejects_invalid_train_clock() {
    let error = Timetable::parse(&package(24)).err().unwrap();
    assert!(error.to_string().contains("invalid clock"));
}
