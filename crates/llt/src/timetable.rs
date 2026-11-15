//! The parsed offline data package (离线数据包) and the query interface
//! over it: the station/train tables, on-demand schedule decoding, the
//! running-day judgement (开行判定) and the station-to-station query
//! (站站查询).
//!
//! The decoded member tables come from [`crate::package::Package`]; the
//! raw `S`/`T` shards stay undecoded until a query needs them, and those
//! scans go through the same byte reader as the parse-time decoders.

use crate::ParseError;
use crate::package::{Consist, DiningCar, Line, Package};
use crate::reader::Reader;

/// The parsed offline timetable package (离线数据包).
pub struct Timetable {
    /// Every decoded member table, produced by `Package::read`.
    package: Package,
}

/// A calendar day.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// Whether a train runs on a given day (开行状态). Mirrors the app's
/// markers: NotStarted `[-]`, Ended `[+]`, OffDay `[*]` (from
/// `Rule.outOfDate`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Runs on this day.
    Runs,
    /// `[-]` — the query date is before the train's start date.
    NotStarted,
    /// `[+]` — the query date is after the train's end date.
    Ended,
    /// `[*]` — the period rule excludes this day.
    OffDay,
}

/// One station-to-station query hit (站站查询结果段): a train visiting
/// `from` before `to` on the query date.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Train index into the train table.
    pub train: usize,
    /// Stop index of the departure within the train's stops.
    pub from_stop: usize,
    /// Stop index of the arrival within the train's stops.
    pub to_stop: usize,
}

/// A train's timetable record (车次时刻): kind byte, running window,
/// running-day rule and ordered stops. Decoded on demand from the T shards.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Train {
    /// Train class byte (G/D/C/T/K/… categories as stored by the producer).
    pub kind: u8,
    /// First running day, as a packed YYYYMMDD integer.
    pub start_date: u32,
    /// Last running day, as a packed YYYYMMDD integer.
    pub end_date: u32,
    /// Index into the `t.rule` running-day table.
    pub rule: usize,
    /// Calling points in order.
    pub stops: Vec<Stop>,
}

/// One calling point of a train (停站).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Stop {
    /// Station index into the station table.
    pub station: usize,
    /// Time-of-day hour (0–23).
    /// Stored arrival hour, except at the origin where it is departure.
    pub hour: u8,
    pub minute: u8,
    /// Day offset relative to the departure day: 0 for the origin, +1 per
    /// midnight crossed so far.
    pub day: u8,
    /// Dwell time in minutes (0 at endpoints).
    pub dwell: u8,
    /// Cumulative distance from the origin in km.
    pub dist: usize,
}

/// One node of an expanded via path (经由节点) — includes non-stop passing
/// points (通过站), which never appear as timetable stops.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct RouteNode {
    pub station: String,
    /// Line name(s), `/`-joined when the station sits on several lines.
    pub lines: String,
    /// Cumulative km along the (last) line.
    pub dist: u32,
}

impl Date {
    pub fn new(year: i32, month: u8, day: u8) -> Date {
        Date { year, month, day }
    }

    /// Packed YYYYMMDD integer, the form used inside T records.
    fn ymd(self) -> u32 {
        self.year as u32 * 10000 + self.month as u32 * 100 + self.day as u32
    }

    /// Days since 1970-01-01.
    fn days(self) -> i64 {
        days_from_civil(self.year, self.month as u32, self.day as u32)
    }
}

impl Timetable {
    /// Parse a raw `an.db` package (the full JAR/ZIP bytes).
    pub fn parse(package: &[u8]) -> Result<Timetable, ParseError> {
        Ok(Timetable {
            package: Package::read(package)?,
        })
    }

    /// Physical railway lines (线路表) from `routes.dat`, in file order.
    pub fn lines(&self) -> &[Line] {
        &self.package.routes.lines
    }

    /// Via-path key lookup by name suffix (经由键名后缀匹配 — 经由, the
    /// geographic path, NOT the consist 交路). Keys are `<prefix><name><2-char
    /// suffix>` layouts of the 12306 train_no. Cross-day names like `G1239/8`
    /// have no exact key, so the part before `/` is tried as a fallback.
    /// Several keys can match (e.g. `1461` also suffix-matches `G1461`) —
    /// disambiguate with `route_key_exact` or by checking expanded endpoints.
    pub fn route_keys_for(&self, train_name: &str) -> Vec<&str> {
        let matching = |name: &str| {
            self.package
                .routes
                .entries
                .iter()
                .filter(|entry| {
                    entry.key.len() > 2 && entry.key[..entry.key.len() - 2].ends_with(name)
                })
                .map(|entry| entry.key.as_str())
                .collect::<Vec<&str>>()
        };
        let mut keys = matching(train_name);
        if keys.is_empty()
            && let Some((prefix, _)) = train_name.split_once('/')
        {
            keys = matching(prefix);
        }
        keys
    }

    /// Expand a via key into the train's full walking path (走行径路展开):
    /// the sequence of stations traversed on each line, with consecutive
    /// repeats merged and line names joined by `/` where a station sits on
    /// several lines. Returns `None` if any segment's line or endpoint is
    /// missing; a partial route is not a complete via path.
    pub fn expand_route(&self, key: &str) -> Option<Vec<RouteNode>> {
        let entry = self
            .package
            .routes
            .entries
            .iter()
            .find(|entry| entry.key == key)?;
        let mut walk: Vec<(String, String, u32)> = Vec::new();
        for (i, (station, line_code)) in entry.segments.iter().enumerate() {
            let next = entry
                .segments
                .get(i + 1)
                .map(|(line_station, _)| line_station.as_str())
                .unwrap_or(entry.terminal.as_str());
            let &line_index = self.package.routes.line_by_code.get(line_code)?;
            let line = &self.package.routes.lines[line_index];
            let start_index = line
                .stations
                .iter()
                .position(|line_station| line_station.name == *station)?;
            let end_index = line
                .stations
                .iter()
                .rposition(|line_station| line_station.name == next)?;
            let stations = &line.stations[start_index.min(end_index)..=start_index.max(end_index)];
            let mut append = |line_station: &crate::package::LineStation| {
                walk.push((
                    line_station.name.clone(),
                    line.name.clone(),
                    line_station.dist,
                ));
            };
            if start_index <= end_index {
                stations.iter().for_each(&mut append);
            } else {
                stations.iter().rev().for_each(&mut append);
            }
        }
        let mut out: Vec<RouteNode> = Vec::new();
        for (station, line, dist) in walk {
            let Some(last) = out.last_mut().filter(|last| last.station == station) else {
                out.push(RouteNode {
                    station,
                    lines: line,
                    dist,
                });
                continue;
            };
            if !last.lines.split('/').any(|name| name == line) {
                last.lines.push('/');
                last.lines.push_str(&line);
            }
        }
        Some(out)
    }

    /// Operating-agency unit strings (担当单位) shared by extra.dat and
    /// jlb.dat.
    pub fn units(&self) -> &[String] {
        &self.package.extras.units
    }

    /// The operating agency (担当单位) running the train, e.g. 上局上海段
    /// for G1. `None` when unknown (the `暂无` placeholder) or absent.
    pub fn agency(&self, train: usize) -> Option<&str> {
        self.package
            .extras
            .agency
            .get(&train)
            .map(|&unit_index| self.package.extras.units[unit_index].as_str())
            .filter(|unit_name| *unit_name != "暂无")
    }

    /// Fuxing groups (复兴号分组) as a bitmask: bit0/bit1/bit2 = group
    /// 1/2/3 (stackable).
    pub fn fuxing_groups(&self, train: usize) -> u8 {
        self.package.extras.fuxing.get(&train).copied().unwrap_or(0)
    }

    /// Turnaround stations (折返站) as station indices (empty when none).
    pub fn turnaround(&self, train: usize) -> &[usize] {
        self.package
            .extras
            .turnaround
            .get(&train)
            .map(|stations| stations.as_slice())
            .unwrap_or(&[])
    }

    /// Whether the train is an S-line suburban train (S201, S2101, …).
    pub fn is_s_train(&self, train: usize) -> bool {
        self.package.extras.s_trains.contains(&train)
    }

    /// The consists table (车底交路表) from jlb.dat, in file order.
    pub fn consists(&self) -> &[Consist] {
        &self.package.jlb.consists
    }

    /// Consists whose rotation includes the exact train name.
    pub fn consists_for(&self, train_name: &str) -> Vec<&Consist> {
        self.package
            .jlb
            .consists
            .iter()
            .filter(|consist| consist.rotation.split('#').any(|name| name == train_name))
            .collect()
    }

    /// Dining-coach placements (餐车) for the train (usually zero or one).
    pub fn dining_cars(&self, train: usize) -> Vec<&DiningCar> {
        self.package
            .jlb
            .dining
            .iter()
            .filter(|placement| placement.train == train)
            .collect()
    }

    /// The train's 12306 internal code (12306 内码, e.g. `24000000G10L` for
    /// G1), one entry per train in t.i order — the key space of the via
    /// table.
    pub fn train_no(&self, train: usize) -> Option<&str> {
        self.package
            .references
            .trainnos
            .get(train)
            .map(|code| code.as_str())
    }

    /// The route-table key for this train, found exactly via its train_no
    /// (unlike `route_keys_for`, which suffix-matches names and can be
    /// ambiguous). `None` when the train has no route entry.
    pub fn route_key_exact(&self, train: usize) -> Option<&str> {
        let train_no = self.train_no(train)?;
        self.package
            .routes
            .entries
            .iter()
            .find(|entry| entry.key == train_no)
            .map(|entry| entry.key.as_str())
    }

    /// Home province of the train's service (车次担当省份; 33 trains carry
    /// one, including cross-border services like 4182/4183 → 老挝).
    pub fn home_province(&self, train: usize) -> Option<&str> {
        self.package
            .references
            .home_province
            .get(&train)
            .map(|province| province.as_str())
    }

    /// Administrative region string (行政区划) for a region id, e.g. id 3 =
    /// `黑龙江省 哈尔滨市 阿城区`.
    pub fn region_name(&self, id: u32) -> Option<&str> {
        self.package
            .references
            .regions
            .get(&id)
            .map(|region| region.as_str())
    }

    /// Administrative region (行政区划) for a county-level place name (the
    /// 672-entry name-keyed table; names are places, not station names).
    pub fn place_region(&self, name: &str) -> Option<&str> {
        self.package
            .references
            .place_regions
            .get(name)
            .map(|region| region.as_str())
    }

    /// The displayed train number at a stop (显示车次切换). Cross-day trains
    /// change numbers mid-journey (4167/4170 shows "4170" until station 82,
    /// then "4167"); the display table in xw.dat gives the switch points.
    pub fn display_name_at(&self, train: usize, stop: usize) -> Option<&str> {
        let display = self.package.display.get(&train)?;
        let name = &self.package.trains[train];
        let parts: Vec<&str> = name.split('/').collect();
        let part = display
            .segments
            .iter()
            .rev()
            .find(|(start, _)| *start <= stop)
            .map(|(_, part)| *part)?;
        parts.get(part as usize).copied()
    }

    /// Raw display flag word of the train's xw.dat record (bit flags used
    /// for price-column display; e.g. 0x30 covers most trains).
    pub fn display_flags(&self, train: usize) -> Option<u16> {
        self.package
            .display
            .get(&train)
            .map(|display| display.flags)
    }

    /// Station aliases (站名别名 / 虚拟站, index 10000+ in same-city
    /// groups), from sn.dat.
    pub fn aliases(&self) -> &[String] {
        &self.package.station_names.aliases
    }

    /// Other members of the station's same-city group (同城站组, e.g.
    /// 鞍山 ↔ 鞍山西, 安多 ↔ 那曲) — the matching the app applies for
    /// station-to-station queries. May contain alias ids ≥ 10000
    /// (`aliases`). Empty when the station has no group.
    pub fn related_stations(&self, station: usize) -> &[usize] {
        self.package
            .station_names
            .same_city
            .get(&station)
            .map(|stations| stations.as_slice())
            .unwrap_or(&[])
    }

    /// Pinyin initial abbreviation (拼音缩写) for the station name, when
    /// listed (e.g. 叆河 → "AH") — 367 names carry one.
    pub fn pinyin_abbr(&self, station: usize) -> Option<&str> {
        let name = self.package.stations.get(station)?;
        self.package
            .station_names
            .pinyin
            .get(name)
            .map(|abbreviation| abbreviation.as_str())
    }

    /// Station table (车站表): names in index order (3310 entries for ver
    /// 20260904).
    pub fn stations(&self) -> &[String] {
        &self.package.stations
    }

    /// Train table (车次表): names in index order (14563 entries for ver
    /// 20260904).
    pub fn trains(&self) -> &[String] {
        &self.package.trains
    }

    /// Index of the station with the exact given name.
    pub fn station_index(&self, name: &str) -> Option<usize> {
        self.package
            .stations
            .iter()
            .position(|station| station == name)
    }

    /// Index of the train with the exact given name.
    pub fn train_index(&self, name: &str) -> Option<usize> {
        self.package.trains.iter().position(|train| train == name)
    }

    /// Trains calling at the station (经停车次), in shard order.
    /// The S-shard scan: records of [station u16c][count u16c][count ×
    /// train-index u16c]; the shard holding the station is
    /// `S[(station + 1) % 10]`.
    ///
    /// # Errors
    /// Returns a decoding error for malformed shard records.
    pub fn trains_through(&self, station: usize) -> Result<Vec<usize>, ParseError> {
        let shard = (station + 1) % 10;
        let bytes = &self.package.s_shards[shard];
        let name = format!("S{shard}.dat");
        let mut reader = Reader::new(bytes, &name);
        while !reader.at_end() {
            let station_index = reader.u16_custom()?;
            let count = reader.u16_custom()?;
            if station_index == station {
                return (0..count).map(|_| reader.u16_custom()).collect();
            }
            reader.skip(2 * count)?;
        }
        Ok(Vec::new())
    }

    /// Decode the schedule of the train with the given index.
    /// The T-shard scan: sequential records of [train u16c][kind 1]
    /// [price_ref 2][start 4][end 4][rule 2][stop_count u16c], then 7 bytes
    /// per stop; the shard holding the train is `T[(index + 1) % 20]`.
    ///
    /// # Errors
    /// Returns a decoding error for truncated records or dangling stations.
    pub fn train(&self, index: usize) -> Result<Option<Train>, ParseError> {
        if index >= self.package.trains.len() {
            return Ok(None);
        }
        let shard = (index + 1) % 20;
        let bytes = &self.package.t_shards[shard];
        let name = format!("T{shard}.dat");
        let mut reader = Reader::new(bytes, &name);
        while !reader.at_end() {
            let train_index = reader.u16_custom()?;
            let kind = reader.byte()?;
            reader.skip(2)?; // price reference, unused here
            let start_date = reader.date()?;
            let end_date = reader.date()?;
            let rule = reader.u16_custom()?;
            let count = reader.u16_custom()?;
            if train_index != index {
                reader.skip(7 * count)?; // [station u16c][hour][minute][dwell][dist u16c] each
                continue;
            }
            let stops = read_stops(&mut reader, count, self.package.stations.len())?;
            return Ok(Some(Train {
                kind,
                start_date,
                end_date,
                rule,
                stops,
            }));
        }
        Ok(None)
    }

    /// Whether the train runs on the given date (开行判定).
    ///
    /// # Errors
    /// Returns an error when the applicable running-day rule is absent or has
    /// a zero period, rather than assuming the train runs daily.
    pub fn runs_on(&self, train: &Train, date: Date) -> Result<RunStatus, ParseError> {
        let ymd = date.ymd();
        if ymd < train.start_date {
            return Ok(RunStatus::NotStarted);
        }
        if ymd > train.end_date {
            return Ok(RunStatus::Ended);
        }
        let (period, mask) = self
            .package
            .rules
            .get(train.rule)
            .copied()
            .ok_or(ParseError::InvalidRule(train.rule))?;
        if period == 0 {
            return Err(ParseError::InvalidRule(train.rule));
        }
        if period != 1 {
            let start = train.start_date;
            let days = date.days()
                - days_from_civil(
                    (start / 10000) as i32,
                    (start / 100 % 100) as u8 as u32,
                    start % 100,
                );
            if (1i64 << (days % period as i64)) & mask as i64 == 0 {
                return Ok(RunStatus::OffDay);
            }
        }
        Ok(RunStatus::Runs)
    }

    /// Station-to-station query (站站查询): trains running from station
    /// `from` to station `to` on `date`, sorted by departure time — the
    /// running set (当日开行) left after filtering the candidates (候选车次).
    /// Ports the app's `QueryZZ`: candidates are trains through both
    /// stations (via the S shards) visiting `from` strictly before `to`,
    /// kept only when the running-day rule allows the date.
    ///
    /// # Errors
    /// Propagates shard decoding and running-day rule errors.
    pub fn query(&self, from: usize, to: usize, date: Date) -> Result<Vec<Segment>, ParseError> {
        let to_set: std::collections::HashSet<usize> =
            self.trains_through(to)?.into_iter().collect();
        let mut hits: Vec<(u32, Segment)> = Vec::new();
        for train_index in self.trains_through(from)? {
            if !to_set.contains(&train_index) {
                continue;
            }
            let Some(train) = self.train(train_index)? else {
                continue;
            };
            let Some((from_stop, to_stop)) = ordered_stops(&train, from, to) else {
                continue;
            };
            if self.runs_on(&train, date)? != RunStatus::Runs {
                continue;
            }
            let departure = &train.stops[from_stop];
            let dep_minutes = (departure.hour as u32 * 60
                + departure.minute as u32
                + if from_stop == 0 {
                    0
                } else {
                    departure.dwell as u32
                })
                % 1440;
            hits.push((
                dep_minutes,
                Segment {
                    train: train_index,
                    from_stop,
                    to_stop,
                },
            ));
        }
        hits.sort_by_key(|(dep_minutes, _)| *dep_minutes);
        Ok(hits.into_iter().map(|(_, segment)| segment).collect())
    }
}

/// Decode calling points and derive day offsets from midnight crossings.
fn read_stops(
    reader: &mut Reader<'_>,
    count: usize,
    station_count: usize,
) -> Result<Vec<Stop>, ParseError> {
    let mut stops = Vec::with_capacity(count);
    let mut day = 0u8;
    let mut previous_minutes: Option<u32> = None;
    for _ in 0..count {
        let station = reader.u16_custom()?;
        if station >= station_count {
            return Err(ParseError::InvalidStation(station));
        }
        let hour = reader.byte()?;
        let minute = reader.byte()?;
        let dwell = reader.byte()?;
        let dist = reader.u16_custom()?;
        let time_of_day_minutes = hour as u32 * 60 + minute as u32;
        if previous_minutes.is_some_and(|previous_minutes| time_of_day_minutes < previous_minutes) {
            day += 1;
        }
        previous_minutes = Some(time_of_day_minutes);
        stops.push(Stop {
            station,
            hour,
            minute,
            day,
            dwell,
            dist,
        });
    }
    Ok(stops)
}

/// Days since 1970-01-01 for a calendar date (Howard Hinnant's
/// `days_from_civil`), used for period-rule day arithmetic.
fn days_from_civil(y: i32, month: u32, day: u32) -> i64 {
    let y = y as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Stop indices of the first visit to `from` and the last visit to `to`,
/// provided `from` comes strictly before `to`.
fn ordered_stops(train: &Train, from: usize, to: usize) -> Option<(usize, usize)> {
    let mut first_from = None;
    let mut last_to = None;
    for (j, stop) in train.stops.iter().enumerate() {
        if stop.station == from && first_from.is_none() {
            first_from = Some(j);
        }
        if stop.station == to {
            last_to = Some(j);
        }
    }
    match (first_from, last_to) {
        (Some(from_index), Some(to_index)) if from_index < to_index => Some((from_index, to_index)),
        _ => None,
    }
}

#[cfg(all(test, feature = "real-data-tests"))]
mod tests {
    use super::*;

    fn timetable() -> Timetable {
        Timetable::parse(&std::fs::read(std::env::var("LLT_CURRENT_PACKAGE").unwrap()).unwrap())
            .unwrap()
    }

    fn intercity_route(timetable: &Timetable) -> usize {
        let train = timetable.train_index("C4850").unwrap();
        let key = timetable.route_key_exact(train).unwrap();
        assert!(!timetable.expand_route(key).unwrap().is_empty());
        timetable
            .package
            .routes
            .entries
            .iter()
            .position(|entry| entry.key == key)
            .unwrap()
    }

    #[test]
    fn incomplete_route_line_does_not_publish_the_remaining_segments() {
        let mut timetable = timetable();
        let index = intercity_route(&timetable);
        assert!(timetable.package.routes.entries[index].segments.len() > 1);
        timetable.package.routes.entries[index].segments[0].1 = u32::MAX;
        let key = &timetable.package.routes.entries[index].key;
        assert!(timetable.expand_route(key).is_none());
    }

    #[test]
    fn missing_route_station_does_not_publish_the_remaining_segments() {
        let mut timetable = timetable();
        let index = intercity_route(&timetable);
        timetable.package.routes.entries[index].segments[0].0 = "absent station".to_string();
        let key = &timetable.package.routes.entries[index].key;
        assert!(timetable.expand_route(key).is_none());
    }

    #[test]
    fn missing_route_terminal_does_not_publish_a_shortened_journey() {
        let mut timetable = timetable();
        let index = intercity_route(&timetable);
        timetable.package.routes.entries[index].terminal = "absent terminal".to_string();
        let key = &timetable.package.routes.entries[index].key;
        assert!(timetable.expand_route(key).is_none());
    }

    #[test]
    fn station_query_propagates_station_shard_corruption() {
        let mut timetable = timetable();
        timetable.package.s_shards[1].truncate(1);
        assert!(
            matches!(timetable.query(0, 1, Date::new(2026, 10, 1)), Err(ParseError::Truncated(name)) if name == "S1.dat")
        );
    }

    #[test]
    fn station_query_propagates_train_shard_corruption() {
        let mut timetable = timetable();
        let train = timetable.train(0).unwrap().unwrap();
        let from = train.stops[0].station;
        let to = train.stops.last().unwrap().station;
        assert!(timetable.trains_through(from).unwrap().contains(&0));
        assert!(timetable.trains_through(to).unwrap().contains(&0));
        timetable.package.t_shards[1].truncate(1);
        assert!(
            matches!(timetable.query(from, to, Date::new(2026, 10, 1)), Err(ParseError::Truncated(name)) if name == "T1.dat")
        );
    }
}
