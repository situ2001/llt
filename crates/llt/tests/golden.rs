#![cfg(feature = "real-data-tests")]

mod common;

use common::timetable;
use llt::{Date, RunStatus, Segment, Timetable};

// Golden values below come from the independently written Python reference
// implementation (/tmp/llt-verify), cross-checked against live 12306 data —
// never recomputed from the Rust code under test.

#[test]
fn parse_exposes_the_station_index() {
    let timetable = timetable();
    let stations = timetable.stations();
    assert_eq!(stations.len(), 3310);
    assert_eq!(&stations[..3], ["叆河", "栟茶", "椑木镇"]);
    assert_eq!(stations[3309], "黟县东");
}

#[test]
fn parse_exposes_the_train_index_and_name_lookups() {
    let timetable = timetable();
    assert_eq!(timetable.trains().len(), 14563);
    assert_eq!(
        &timetable.trains()[..5],
        ["1461", "1462", "4016", "4017", "4019"]
    );
    assert_eq!(timetable.train_index("G1"), Some(6228));
    assert_eq!(timetable.train_index("D1"), Some(3199));
    assert_eq!(timetable.station_index("北京南"), Some(171));
    assert_eq!(timetable.station_index("上海虹桥"), Some(2041));
    assert_eq!(timetable.station_index("没有这个站"), None);
    assert_eq!(timetable.train_index("G99999"), None);
}

#[test]
fn trains_through_returns_trains_calling_at_a_station() {
    let timetable = timetable();
    let bjn = timetable.station_index("北京南").unwrap();
    let through = timetable.trains_through(bjn).expect("decode station shard");
    assert_eq!(through.len(), 567);
    assert!(through.contains(&timetable.train_index("G1").unwrap()));
    let names: Vec<&str> = through[..10]
        .iter()
        .map(|&train_index| timetable.trains()[train_index].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "C2075",
            "G4232/G4233",
            "C2074",
            "C2076",
            "G4136",
            "G4135",
            "G4231/G4234",
            "C2551",
            "G531",
            "G531B"
        ]
    );
}

#[test]
fn train_decodes_the_full_g1_schedule() {
    let timetable = timetable();
    let train = timetable
        .train(timetable.train_index("G1").unwrap())
        .expect("decode train shard")
        .unwrap();
    assert_eq!(train.kind, 6);
    assert_eq!(train.start_date, 20260127);
    assert_eq!(train.end_date, 20501231);
    assert_eq!(train.rule, 0);
    let stops: Vec<(&str, u8, u8, u8, usize)> = train
        .stops
        .iter()
        .map(|stop| {
            (
                timetable.stations()[stop.station].as_str(),
                stop.hour,
                stop.minute,
                stop.dwell,
                stop.dist,
            )
        })
        .collect();
    assert_eq!(
        stops,
        [
            ("北京南", 6, 30, 0, 0),
            ("沧州西", 7, 18, 2, 210),
            ("德州东", 7, 45, 2, 314),
            ("曲阜东", 8, 34, 2, 535),
            ("南京南", 10, 13, 2, 1023),
            ("苏州北", 10, 59, 2, 1237),
            ("上海虹桥", 11, 24, 0, 1318),
        ]
    );
}

#[test]
fn stops_carry_the_day_offset_for_overnight_runs() {
    let timetable = timetable();
    // D1 北京西 17:58 -> 长沙 08:16, crossing one midnight at 武昌.
    let d1 = timetable
        .train(timetable.train_index("D1").unwrap())
        .expect("decode train shard")
        .unwrap();
    let days: Vec<u8> = d1.stops.iter().map(|stop| stop.day).collect();
    assert_eq!(days, [0, 0, 0, 1, 1, 1]);
    // K118 成都西 21:12 -> 北京西 05:26, crossing two midnights.
    let k118 = timetable
        .train(timetable.train_index("K118").unwrap())
        .expect("decode train shard")
        .unwrap();
    assert_eq!(k118.stops[0].day, 0);
    assert_eq!(k118.stops.last().unwrap().day, 2);
}

#[test]
fn runs_on_reports_the_running_day_status() {
    let timetable = timetable();
    let g1 = timetable
        .train(timetable.train_index("G1").unwrap())
        .expect("decode train shard")
        .unwrap();
    assert_eq!(
        timetable
            .runs_on(&g1, Date::new(2026, 8, 31))
            .expect("resolve running-day rule"),
        RunStatus::Runs
    );
    assert_eq!(
        timetable
            .runs_on(&g1, Date::new(2026, 1, 26))
            .expect("resolve running-day rule"),
        RunStatus::NotStarted
    );
    assert_eq!(
        timetable
            .runs_on(&g1, Date::new(2051, 1, 1))
            .expect("resolve running-day rule"),
        RunStatus::Ended
    );
    // G531B is excluded by its period rule on both query dates.
    let g531b = timetable
        .train(timetable.train_index("G531B").unwrap())
        .expect("decode train shard")
        .unwrap();
    assert_eq!(
        timetable
            .runs_on(&g531b, Date::new(2026, 8, 31))
            .expect("resolve running-day rule"),
        RunStatus::OffDay
    );
    assert_eq!(
        timetable
            .runs_on(&g531b, Date::new(2026, 8, 30))
            .expect("resolve running-day rule"),
        RunStatus::OffDay
    );
}

fn dep_minutes(timetable: &Timetable, segment: &Segment) -> u32 {
    let dep = &timetable
        .train(segment.train)
        .expect("decode train shard")
        .unwrap()
        .stops[segment.from_stop];
    dep.hour as u32 * 60 + dep.minute as u32
}

#[test]
fn query_lists_trains_running_between_two_stations_in_order() {
    let timetable = timetable();
    let bjn = timetable.station_index("北京南").unwrap();
    let aoh = timetable.station_index("上海虹桥").unwrap();
    let rows = timetable
        .query(bjn, aoh, Date::new(2026, 8, 31))
        .expect("query timetable");
    assert_eq!(rows.len(), 40);
    let row = |i: usize| {
        let segment = &rows[i];
        let train = timetable
            .train(segment.train)
            .expect("decode train shard")
            .unwrap();
        let dep = &train.stops[segment.from_stop];
        let arr = &train.stops[segment.to_stop];
        (
            timetable.trains()[segment.train].as_str(),
            (dep.hour, dep.minute),
            (arr.hour, arr.minute),
        )
    };
    assert_eq!(row(0), ("G531", (6, 8), (12, 4)));
    assert_eq!(row(2), ("G1", (6, 30), (11, 24)));
    assert_eq!(row(39), ("G35", (19, 24), (23, 51)));
    // Sorted by departure time.
    for pair in rows.windows(2) {
        assert!(dep_minutes(timetable, &pair[0]) <= dep_minutes(timetable, &pair[1]));
    }
    // The off-day train G531B is filtered out.
    let g531b = timetable.train_index("G531B").unwrap();
    assert!(!rows.iter().any(|segment| segment.train == g531b));
}

#[test]
fn query_exposes_overnight_arrivals_through_stop_day_offsets() {
    let timetable = timetable();
    let bx = timetable.station_index("北京西").unwrap();
    let wc = timetable.station_index("武昌").unwrap();
    let rows = timetable
        .query(bx, wc, Date::new(2026, 9, 1))
        .expect("query timetable");
    let d1 = timetable.train_index("D1").unwrap();
    let segment = rows
        .iter()
        .find(|segment| segment.train == d1)
        .expect("D1 北京西->武昌");
    let train = timetable.train(d1).expect("decode train shard").unwrap();
    let dep = &train.stops[segment.from_stop];
    let arr = &train.stops[segment.to_stop];
    assert_eq!((dep.hour, dep.minute), (17, 58));
    assert_eq!((arr.hour, arr.minute), (4, 44));
    assert_eq!(arr.day, 1);
}

#[test]
fn routes_contain_the_line_table() {
    let timetable = timetable();
    let lines = timetable.lines();
    assert_eq!(lines.len(), 784);
    let jgg = lines.iter().find(|line| line.name == "京广高铁").unwrap();
    assert_eq!(jgg.code, 331);
    assert_eq!(jgg.stations.len(), 42);
    let first = &jgg.stations[0];
    assert_eq!(
        (first.name.as_str(), first.telecode.as_str()),
        ("北京丰台", "FTP")
    );
    assert_eq!(first.flag, u32::MAX);
    assert_eq!(first.dist, 0);
    assert_eq!(
        (jgg.stations[1].name.as_str(), jgg.stations[1].dist),
        ("杜家坎所", 10)
    );
    let last = jgg.stations.last().unwrap();
    assert_eq!(
        (last.name.as_str(), last.telecode.as_str(), last.dist),
        ("广州南", "IZQ", 2291)
    );
    let hns = lines.iter().find(|line| line.name == "沪宁高速").unwrap();
    assert_eq!(hns.code, 244);
    assert_eq!(hns.stations.len(), 22);
    assert_eq!(
        (
            hns.stations[0].name.as_str(),
            hns.stations[0].telecode.as_str()
        ),
        ("上海", "SHH")
    );
    assert_eq!(hns.stations.last().unwrap().dist, 301);
}

#[test]
fn route_keys_match_train_names_and_expand_to_walking_routes() {
    let timetable = timetable();
    // train_no keys are <prefix><name><2-char suffix>: match on the name part.
    assert_eq!(timetable.route_keys_for("G1"), ["24000000G10L"]);
    let nodes = timetable.expand_route("24000000G10L").unwrap();
    assert_eq!(nodes.len(), 27);
    assert_eq!(nodes[0].station, "北京南");
    assert_eq!(nodes[0].lines, "京沪高速线");
    assert_eq!(nodes[0].dist, 0);
    assert_eq!(nodes.last().unwrap().station, "上海虹桥");
    assert_eq!(nodes.last().unwrap().dist, 1318);
    // Cross-day names have no exact key: fall back to the part before '/'.
    assert_eq!(timetable.route_keys_for("G1239/8"), ["02000G12390D"]);
    let n2 = timetable.expand_route("02000G12390D").unwrap();
    // Independently expanded from routes.dat: 68 section nodes minus nine
    // shared junctions. Reverse sections must meet before junction merging.
    assert_eq!(n2.len(), 59);
    assert_eq!(n2[0].station, "双鸭山西");
    assert_eq!(n2.last().unwrap().station, "石家庄");
    // Ambiguity is surfaced (1461 also suffix-matches G1461), not hidden.
    assert_eq!(timetable.route_keys_for("1461").len(), 2);
}

#[test]
fn extras_expose_agency_fuxing_and_flags() {
    let timetable = timetable();
    assert_eq!(timetable.units().len(), 371);
    let g1 = timetable.train_index("G1").unwrap();
    let d1 = timetable.train_index("D1").unwrap();
    assert_eq!(timetable.agency(g1), Some("上局上海段"));
    assert_eq!(
        timetable.agency(timetable.train_index("G7").unwrap()),
        Some("京局天津客运段")
    );
    assert_eq!(
        timetable.agency(timetable.train_index("K118").unwrap()),
        Some("成都局成都客运段")
    );
    assert_eq!(timetable.agency(d1), Some("广铁长沙段"));
    // 复兴号 groups as a bitmask: bit0 = group 1, bit1 = group 2, bit2 = group 3.
    assert_eq!(timetable.fuxing_groups(g1), 0b001);
    assert_eq!(
        timetable.fuxing_groups(timetable.train_index("G2").unwrap()),
        0b001
    );
    assert_eq!(
        timetable.fuxing_groups(timetable.train_index("G7").unwrap()),
        0b001
    );
    assert_eq!(timetable.fuxing_groups(d1), 0b101);
    // Group sizes over the full train table.
    let (mut c1, mut c2, mut c3) = (0, 0, 0);
    for train_index in 0..timetable.trains().len() {
        let groups = timetable.fuxing_groups(train_index);
        c1 += (groups & 1 != 0) as usize;
        c2 += (groups & 2 != 0) as usize;
        c3 += (groups & 4 != 0) as usize;
    }
    assert_eq!((c1, c2, c3), (3978, 309, 799));
    // Turnaround stations, resolved against the station table.
    let t4367 = timetable.train_index("4367/4370").unwrap();
    assert_eq!(
        timetable.turnaround(t4367),
        &[timetable.station_index("甘旗卡").unwrap()]
    );
    // S-line suburban trains.
    assert!(timetable.is_s_train(timetable.train_index("S201").unwrap()));
    let s_count = (0..timetable.trains().len())
        .filter(|&train_index| timetable.is_s_train(train_index))
        .count();
    assert_eq!(s_count, 101);
}

#[test]
fn jlb_exposes_consists_and_dining_cars() {
    let timetable = timetable();
    let consists = timetable.consists();
    assert_eq!(consists.len(), 3573);
    let z16 = timetable.consists_for("Z16");
    assert_eq!(z16.len(), 1);
    let consist = z16[0];
    assert_eq!((consist.start_date, consist.end_date), (20250703, 20991231));
    assert_eq!((consist.flag8, consist.flag12), (1, 1));
    assert_eq!(timetable.units()[consist.agency], "哈局哈尔滨段");
    assert_eq!(consist.model, "BSP型25T型，AC380V，双管供风,集便");
    assert_eq!(consist.rotation, "Z16#Z15");
    // Distinct vehicle-model strings across the whole table.
    let models: std::collections::HashSet<&str> = consists
        .iter()
        .map(|consist| consist.model.as_str())
        .collect();
    assert_eq!(models.len(), 967);
    assert_eq!(
        models
            .iter()
            .filter(|model| model.starts_with("CR"))
            .count(),
        345
    );
    for want in [
        "CRH5A",
        "CRH380BG",
        "CRH380BG型重联",
        "CR400BF-C",
        "CR400BF-GS",
        "CR400BF-GZ型重联",
    ] {
        assert!(models.contains(want), "missing model {want}");
    }
    // Dining-car placement.
    let foods = timetable.dining_cars(timetable.train_index("Z16").unwrap());
    assert_eq!(foods.len(), 1);
    assert_eq!(
        (foods[0].start_date, foods[0].end_date),
        (20190105, 20400331)
    );
    assert_eq!(foods[0].coach, "9车[餐]");
}

#[test]
fn sp_maps_trains_to_12306_train_nos() {
    let timetable = timetable();
    let g1 = timetable.train_index("G1").unwrap();
    assert_eq!(timetable.train_no(g1), Some("24000000G10L"));
    assert_eq!(
        timetable.train_no(timetable.train_index("D1").unwrap()),
        Some("24000000D131")
    );
    assert_eq!(timetable.train_no(0), Some("240000146100")); // 1461, first in t.i
    assert_eq!(
        timetable.train_no(timetable.train_index("K118").unwrap()),
        Some("760000K1180L")
    );
    // The routes table's keys are exactly these train_nos: exact, unambiguous
    // lookup (no G1461-style suffix collisions).
    assert_eq!(timetable.route_key_exact(g1), Some("24000000G10L"));
    assert_eq!(timetable.route_key_exact(0), Some("240000146100"));
    // 33 trains carry a home province, including cross-border services.
    assert_eq!(
        timetable.home_province(timetable.train_index("1462").unwrap()),
        Some("河北省")
    );
    assert_eq!(
        timetable.home_province(timetable.train_index("4167/4170").unwrap()),
        Some("香港")
    );
    assert_eq!(
        timetable.home_province(timetable.train_index("4182/4183").unwrap()),
        Some("老挝")
    );
    assert_eq!(timetable.home_province(g1), None);
}

#[test]
fn sp_exposes_administrative_regions() {
    let timetable = timetable();
    assert_eq!(timetable.region_name(3), Some("黑龙江省 哈尔滨市 阿城区"));
    assert_eq!(timetable.region_name(1), Some("江苏省 南通市 如东县"));
    assert_eq!(timetable.region_name(2), Some("四川省 内江市 东兴区"));
    assert_eq!(timetable.region_name(99999), None);
    // Place-name-keyed region lookup (county-level places, not station names).
    assert_eq!(
        timetable.place_region("阿巴嘎旗"),
        Some("内蒙古 锡林郭勒盟 阿巴嘎旗")
    );
    assert_eq!(timetable.place_region("安图"), Some("吉林省 延边州 安图县"));
    assert_eq!(timetable.place_region("北京"), None);
}

#[test]
fn xw_resolves_per_station_display_names() {
    let timetable = timetable();
    // 4167/4170 displays as "4170" from station 0, then "4167" from 82 on.
    let train_index = timetable.train_index("4167/4170").unwrap();
    assert_eq!(timetable.display_name_at(train_index, 0), Some("4170"));
    assert_eq!(timetable.display_name_at(train_index, 81), Some("4170"));
    assert_eq!(timetable.display_name_at(train_index, 82), Some("4167"));
    assert_eq!(timetable.display_flags(train_index), Some(0x0005));
    // Trains without a cross-day name still carry the flags word (0x30 is
    // the modal value, 10308 trains) but have no display switch points.
    let g1 = timetable.train_index("G1").unwrap();
    assert_eq!(timetable.display_name_at(g1, 0), None);
    assert_eq!(timetable.display_flags(g1), Some(48));
}

#[test]
fn sn_links_same_city_stations() {
    let timetable = timetable();
    assert_eq!(timetable.aliases().len(), 69);
    assert_eq!(timetable.aliases()[0], "安图");
    // Same-city groups work in both directions.
    let ans = timetable.station_index("鞍山").unwrap();
    let ansx = timetable.station_index("鞍山西").unwrap();
    assert_eq!(timetable.related_stations(ans), [ansx]);
    assert_eq!(timetable.related_stations(ansx), [ans]);
    let bc = timetable.station_index("栟茶").unwrap();
    assert_eq!(
        timetable.related_stations(bc),
        [timetable.station_index("如东").unwrap()]
    );
    let ac = timetable.station_index("阿城").unwrap();
    assert_eq!(
        timetable.related_stations(ac),
        [timetable.station_index("阿城北").unwrap()]
    );
    // Beijing is one big same-city group (verified against sn.dat directly).
    let bjn = timetable.station_index("北京南").unwrap();
    assert_eq!(
        timetable.related_stations(bjn),
        [
            timetable.station_index("北京").unwrap(),
            timetable.station_index("北京北").unwrap(),
            timetable.station_index("北京朝阳").unwrap(),
            timetable.station_index("北京大兴").unwrap(),
            timetable.station_index("北京东").unwrap(),
            timetable.station_index("北京丰台").unwrap(),
            timetable.station_index("北京西").unwrap(),
            timetable.station_index("昌平北").unwrap(),
            timetable.station_index("清河").unwrap(),
            timetable.station_index("亦庄").unwrap(),
        ]
    );
    // Pinyin abbreviations for input matching.
    assert_eq!(timetable.pinyin_abbr(0), Some("AH")); // 叆河, first station
    assert_eq!(
        timetable.pinyin_abbr(timetable.station_index("椑木镇").unwrap()),
        Some("PMZ")
    );
    assert_eq!(
        timetable.pinyin_abbr(timetable.station_index("北京南").unwrap()),
        None
    );
}

#[test]
fn intercity_via_paths_preserve_travel_direction_and_passing_stations() {
    let timetable = timetable();
    for name in ["C4850", "S4792"] {
        let index = timetable.train_index(name).unwrap();
        let route = timetable
            .expand_route(timetable.route_key_exact(index).unwrap())
            .unwrap();
        let names: Vec<&str> = route.iter().map(|node| node.station.as_str()).collect();
        let from = names.iter().position(|&name| name == "东莞西").unwrap();
        let to = names.iter().position(|&name| name == "科韵路").unwrap();
        assert_eq!(
            &names[from..=to],
            &[
                "东莞西",
                "麻涌",
                "广州莲花山",
                "化龙南",
                "深井",
                "琶洲",
                "科韵路"
            ],
            "{name}"
        );
        let train = timetable.train(index).expect("decode train shard").unwrap();
        assert_eq!(
            names.first().copied(),
            Some(timetable.stations()[train.stops[0].station].as_str())
        );
        assert_eq!(
            names.last().copied(),
            Some(timetable.stations()[train.stops.last().unwrap().station].as_str())
        );
        assert_eq!(
            route
                .iter()
                .find(|node| node.station == "广州莲花山")
                .unwrap()
                .lines,
            "广惠城际线/琶莲城际"
        );
    }
}

#[test]
fn intermediate_station_query_sorts_by_departure_including_dwell() {
    let timetable = timetable();
    let rows = timetable
        .query(
            timetable.station_index("台山").unwrap(),
            timetable.station_index("广州南").unwrap(),
            Date::new(2026, 8, 31),
        )
        .expect("query timetable");
    assert!(rows.len() > 1);
    let departures: Vec<u32> = rows
        .iter()
        .map(|segment| {
            let train = timetable
                .train(segment.train)
                .expect("decode train shard")
                .unwrap();
            let stop = &train.stops[segment.from_stop];
            (stop.hour as u32 * 60
                + stop.minute as u32
                + if segment.from_stop == 0 {
                    0
                } else {
                    stop.dwell as u32
                })
                % 1440
        })
        .collect();
    assert!(departures.windows(2).all(|w| w[0] <= w[1]));
}
