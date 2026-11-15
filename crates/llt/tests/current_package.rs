#![cfg(feature = "real-data-tests")]

use llt::Timetable;
use std::collections::HashSet;

#[test]
fn current_package_trains_are_parseable() {
    let package = std::fs::read(std::env::var("LLT_CURRENT_PACKAGE").unwrap()).unwrap();
    let timetable = Timetable::parse(&package).expect("parse current package");
    assert!(!timetable.stations().is_empty());
    assert!(!timetable.trains().is_empty());
    assert!(!timetable.lines().is_empty());
    let mut expected_callers = vec![HashSet::new(); timetable.stations().len()];
    for index in 0..timetable.trains().len() {
        let train = timetable
            .train(index)
            .expect("decode train shard")
            .unwrap_or_else(|| panic!("unable to decode train {index}"));
        assert!(!train.stops.is_empty(), "train {index} has no stops");
        for stop in train.stops {
            assert!(
                stop.station < timetable.stations().len(),
                "train {index} has an invalid station index"
            );
            expected_callers[stop.station].insert(index);
        }
    }
    for (station, expected) in expected_callers.into_iter().enumerate() {
        let callers = timetable
            .trains_through(station)
            .expect("decode station shard");
        let actual: HashSet<_> = callers.iter().copied().collect();
        assert_eq!(
            callers.len(),
            actual.len(),
            "duplicate callers at station {station}"
        );
        assert!(
            actual.iter().all(|&index| index < timetable.trains().len()),
            "invalid caller at station {station}"
        );
        // S shards also include through trains without an explicit timetable stop.
        assert!(
            expected.is_subset(&actual),
            "missing stopping train at station {station}"
        );
    }
    let g1 = timetable
        .train(timetable.train_index("G1").expect("G1 in train table"))
        .expect("decode train shard");
    assert!(g1.is_some_and(|train| !train.stops.is_empty()));
}
