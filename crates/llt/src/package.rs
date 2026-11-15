//! Decoders for the members of the Lulutong offline data package
//! (离线数据包) `an.db` — a JAR (ZIP) whose binary members hold the
//! station/train name indexes (`s.i`/`t.i`), the station→trains shards
//! (`S0–S9.dat`), the train schedules (`T0–T19.dat`), running-day rules
//! (`t.rule`) and the auxiliary tables (routes/extra/jlb/sp/xw/sn .dat).
//!
//! [`Package::read`] is the single entry point: extract every member and
//! decode it into the tables `Timetable` serves. Everything here is
//! crate-private; the public seam stays `Timetable::parse`.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::ParseError;
use crate::reader::Reader;

/// The decoded contents of one offline data package (离线数据包): every
/// member table `Timetable` serves, in one place.
pub(crate) struct Package {
    pub(crate) stations: Vec<String>,
    pub(crate) trains: Vec<String>,
    /// Raw station→trains shards `S0–S9.dat`; decoded on demand.
    pub(crate) s_shards: Vec<Vec<u8>>,
    /// Raw train-schedule shards `T0–T19.dat`; decoded on demand.
    pub(crate) t_shards: Vec<Vec<u8>>,
    pub(crate) rules: Vec<(u32, u32)>,
    pub(crate) routes: Routes,
    pub(crate) extras: Extras,
    pub(crate) jlb: Jlb,
    pub(crate) references: ReferenceTables,
    pub(crate) display: HashMap<usize, TrainDisplay>,
    pub(crate) station_names: StationNames,
}

pub(crate) struct ReferenceTables {
    pub(crate) trainnos: Vec<String>,
    pub(crate) home_province: HashMap<usize, String>,
    pub(crate) regions: HashMap<u32, String>,
    pub(crate) place_regions: HashMap<String, String>,
}

pub(crate) struct TrainDisplay {
    pub(crate) flags: u16,
    /// Switch points [(station order, name-part index)].
    pub(crate) segments: Vec<(usize, u8)>,
}

pub(crate) struct StationNames {
    pub(crate) aliases: Vec<String>,
    /// Station/alias index -> other members of its same-city group.
    pub(crate) same_city: HashMap<usize, Vec<usize>>,
    /// Station name -> pinyin initial abbreviation (367 entries).
    pub(crate) pinyin: HashMap<String, String>,
}

impl Package {
    /// Extract and decode every member of the raw JAR/ZIP bytes.
    pub(crate) fn read(bytes: &[u8]) -> Result<Package, ParseError> {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| ParseError::Archive)?;
        let stations = read_index(&mut archive, "s.i")?;
        let trains = read_index(&mut archive, "t.i")?;
        let s_shards = read_shards(&mut archive, "S", 10)?;
        let t_shards = read_shards(&mut archive, "T", 20)?;
        validate_shards(&s_shards, &t_shards)?;
        let rules = read_rules(&mut archive)?;
        let routes = read_routes(&mut archive)?;
        let extras = read_extras(&mut archive)?;
        let jlb = read_jlb(&mut archive)?;
        let references = read_sp(&mut archive)?;
        let display = read_xw(&mut archive)?;
        let station_names = read_sn(&mut archive)?;
        Ok(Package {
            stations,
            trains,
            s_shards,
            t_shards,
            rules,
            routes,
            extras,
            jlb,
            references,
            display,
            station_names,
        })
    }
}

pub(crate) struct Routes {
    pub(crate) lines: Vec<Line>,
    pub(crate) line_by_code: HashMap<u32, usize>,
    pub(crate) entries: Vec<RouteEntry>,
}

pub(crate) struct RouteEntry {
    pub(crate) key: String,
    pub(crate) terminal: String,
    pub(crate) segments: Vec<(String, u32)>,
}

pub(crate) struct Extras {
    /// Shared agency/depot unit strings; extra.dat and jlb.dat both index it.
    pub(crate) units: Vec<String>,
    /// Train index -> unit index.
    pub(crate) agency: HashMap<usize, usize>,
    /// Train index -> 复兴号 group bitmask (bit0..bit2 = group 1..3).
    pub(crate) fuxing: HashMap<usize, u8>,
    /// Train index -> turnaround station indices.
    pub(crate) turnaround: HashMap<usize, Vec<usize>>,
    pub(crate) s_trains: HashSet<usize>,
}

pub(crate) struct Jlb {
    pub(crate) consists: Vec<Consist>,
    pub(crate) dining: Vec<DiningCar>,
}

/// A physical railway line (线路) with its ordered calling points.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Line {
    /// Internal line id referenced by train routes.
    pub code: u32,
    /// Real line name, e.g. 京广高铁, 津秦高速线.
    pub name: String,
    pub stations: Vec<LineStation>,
}

/// One calling point on a line (线路停站), with its telegraph code (电报码).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct LineStation {
    pub name: String,
    /// Railway telegraph code, e.g. "FTP" for 北京丰台.
    pub telecode: String,
    /// Line-membership flag (0xFFFFFFFF in the current package).
    pub flag: u32,
    /// Cumulative km along the line.
    pub dist: u32,
}

/// One rolling-stock consist (车底交路): a vehicle type set running a
/// rotation of train numbers during a date range (same vehicle set
/// turn-by-turn).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Consist {
    /// First day the assignment applies (packed YYYYMMDD).
    pub start_date: u32,
    /// Last day the assignment applies (packed YYYYMMDD).
    pub end_date: u32,
    /// Unknown flag word at record byte offset 8.
    pub flag8: u32,
    /// Unknown flag word at record byte offset 12.
    pub flag12: u32,
    /// Index into the shared unit strings (`Timetable::units`).
    pub agency: usize,
    /// Vehicle model description, e.g. `CRH380BG`, `BSP型25T型，AC380V，…`.
    pub model: String,
    /// `#`-separated train numbers the set cycles through (`Z16#Z15`);
    /// cross-day numbers keep the `/` (`G1239/8`), `0`-prefixed are deadheads.
    pub rotation: String,
}

/// Dining-coach placement (餐车) for a train during a date range.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct DiningCar {
    /// Train index into the train table.
    pub train: usize,
    /// First day the placement applies (packed YYYYMMDD).
    pub start_date: u32,
    /// Last day the placement applies (packed YYYYMMDD).
    pub end_date: u32,
    /// e.g. `9车[餐]`, `4车无餐座`.
    pub coach: String,
}

fn read_member(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<u8>, ParseError> {
    let mut member = archive
        .by_name(name)
        .map_err(|_| ParseError::MissingMember(name.to_string()))?;
    let mut buf = Vec::new();
    member
        .read_to_end(&mut buf)
        .map_err(|_| ParseError::Truncated(name.to_string()))?;
    Ok(buf)
}

/// Shard members `S0–S9.dat` / `T0–T19.dat`.
fn read_shards(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    prefix: &str,
    count: usize,
) -> Result<Vec<Vec<u8>>, ParseError> {
    (0..count)
        .map(|shard_index| read_member(archive, &format!("{prefix}{shard_index}.dat")))
        .collect()
}

/// The `t.rule` member: ASCII lines of "<period> <mask>" pairs.
fn read_rules(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<Vec<(u32, u32)>, ParseError> {
    let bytes = read_member(archive, "t.rule")?;
    let text = String::from_utf8(bytes).map_err(|_| ParseError::Truncated("t.rule".to_string()))?;
    let mut rules = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(period), Some(mask)) = (parts.next(), parts.next()) else {
            return Err(ParseError::Truncated("t.rule".to_string()));
        };
        let (Ok(period), Ok(mask)) = (period.parse::<u32>(), mask.parse::<u32>()) else {
            return Err(ParseError::Truncated("t.rule".to_string()));
        };
        rules.push((period, mask));
    }
    Ok(rules)
}

/// Index members (`s.i`, `t.i`): a big-endian u16 entry count, then
/// Java-`readUTF` length-prefixed UTF-8 names.
fn read_index(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<String>, ParseError> {
    let bytes = read_member(archive, name)?;
    let mut reader = Reader::new(&bytes, name);
    let n = reader.u16be()?;
    let mut entries = Vec::with_capacity(n as usize);
    for _ in 0..n {
        entries.push(reader.str16()?);
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(entries)
}

/// extra.dat: per-train attributes. Counts use the 255-base encoding;
/// strings are length-prefixed UTF-8. The per-station date-range entries
/// exist in the layout but are empty for every record in the current
/// package, so they are consumed and discarded.
fn read_extras(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<Extras, ParseError> {
    let bytes = read_member(archive, "extra.dat")?;
    let mut reader = Reader::new(&bytes, "extra.dat");
    let n_units = reader.u16_custom()?;
    let mut units = Vec::with_capacity(n_units);
    for _ in 0..n_units {
        units.push(reader.str8()?);
    }
    let mut agency = HashMap::new();
    let n_records = reader.u16_custom()?;
    for _ in 0..n_records {
        let train = reader.u16_custom()?;
        let unit = reader.u16_custom()?;
        agency.insert(train, unit);
        let n_st = reader.byte()?;
        for _ in 0..n_st {
            reader.u16_custom()?;
            let n_entries = reader.byte()?;
            for _ in 0..n_entries {
                reader.take(8)?; // start + end, base-128 packed YYYYMMDD
                reader.str8()?;
            }
        }
    }
    let mut fuxing: HashMap<usize, u8> = HashMap::new();
    for bit in 0..3u8 {
        let n = reader.u16_custom()?;
        for _ in 0..n {
            let train = reader.u16_custom()?;
            *fuxing.entry(train).or_insert(0) |= 1 << bit;
        }
    }
    let mut turnaround: HashMap<usize, Vec<usize>> = HashMap::new();
    let n_turn = reader.u16_custom()?;
    for _ in 0..n_turn {
        let train = reader.u16_custom()?;
        let n = reader.byte()? as usize;
        let stations = (0..n)
            .map(|_| reader.u16_custom())
            .collect::<Result<Vec<_>, _>>()?;
        turnaround.insert(train, stations);
    }
    let mut s_trains = HashSet::new();
    let n_s = reader.u16_custom()?;
    for _ in 0..n_s {
        s_trains.insert(reader.u16_custom()?);
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(Extras {
        units,
        agency,
        fuxing,
        turnaround,
        s_trains,
    })
}

/// sp.dat: the per-train 12306 train_no table, a small train→province
/// table, a per-station 3-word table of unknown semantics (consumed), the
/// administrative-region string table and a place-name→region table.
/// Big-endian u32 counters; strings 1-byte length-prefixed.
fn read_sp(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<ReferenceTables, ParseError> {
    let bytes = read_member(archive, "sp.dat")?;
    let mut reader = Reader::new(&bytes, "sp.dat");
    // TSDD records (empty in the current package): consume for forward
    // compatibility — (u32, u32, u32, u16c, u16c) each.
    let tsdd = reader.u32be()?;
    for _ in 0..tsdd {
        reader.take(12)?;
        reader.u16_custom()?;
        reader.u16_custom()?;
    }
    let n = reader.u32be()?;
    let mut trainnos = Vec::with_capacity(n as usize);
    for _ in 0..n {
        trainnos.push(reader.str8()?);
    }
    let mut home_province = HashMap::new();
    let n = reader.u16_custom()?;
    for _ in 0..n {
        let train = reader.u16_custom()?;
        home_province.insert(train, reader.str8()?);
    }
    // Per-station 3-word records (station, a, c); the meaning of the two
    // 255-base words is not established — consumed, not exposed.
    let n = reader.u32be()?;
    for _ in 0..n {
        reader.take(4)?;
        reader.u16_custom()?;
        reader.u16_custom()?;
    }
    let mut regions = HashMap::new();
    let n = reader.u32be()?;
    for _ in 0..n {
        let id = reader.u16be()? as u32;
        let len = reader.byte()? as usize;
        let name = String::from_utf8(reader.take(len)?.to_vec()).map_err(|_| reader.truncated())?;
        regions.insert(id, name);
    }
    let mut place_regions = HashMap::new();
    let n = reader.u32be()?;
    for _ in 0..n {
        let kv = reader.str8()?;
        if let Some((name, region)) = kv.split_once(',') {
            place_regions.insert(name.to_string(), region.to_string());
        }
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(ReferenceTables {
        trainnos,
        home_province,
        regions,
        place_regions,
    })
}

/// xw.dat: per-train display-name switch points. Sequential records of
/// [train u16c][flags u16c][count u8][count × (station order u16c, name-part u8)].
fn read_xw(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<HashMap<usize, TrainDisplay>, ParseError> {
    let bytes = read_member(archive, "xw.dat")?;
    let mut reader = Reader::new(&bytes, "xw.dat");
    let mut display = HashMap::new();
    while !reader.at_end() {
        let train = reader.u16_custom()?;
        let flags = reader.u16_custom()? as u16;
        let n = reader.byte()? as usize;
        let mut segments = Vec::with_capacity(n);
        for _ in 0..n {
            let start = reader.u16_custom()?;
            let part = reader.byte()?;
            segments.push((start, part));
        }
        display.insert(train, TrainDisplay { flags, segments });
    }
    Ok(display)
}

/// sn.dat (Java DataInputStream layout): station-name aliases (virtual
/// indices 10000+), same-city station groups and name→pinyin-abbreviation
/// pairs.
fn read_sn(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<StationNames, ParseError> {
    let bytes = read_member(archive, "sn.dat")?;
    let mut reader = Reader::new(&bytes, "sn.dat");
    let mut aliases = Vec::new();
    let n1 = reader.u16be()?;
    for _ in 0..n1 {
        aliases.push(reader.str16()?);
    }
    let mut same_city: HashMap<usize, Vec<usize>> = HashMap::new();
    let n2 = reader.u16be()?;
    for _ in 0..n2 {
        let head = reader.u16be()? as usize;
        let n = reader.byte()? as usize;
        let mut members = Vec::with_capacity(n + 1);
        members.push(head);
        for _ in 0..n {
            members.push(reader.u16be()? as usize);
        }
        for member_index in &members {
            let others: Vec<usize> = members
                .iter()
                .copied()
                .filter(|x| x != member_index)
                .collect();
            same_city.insert(*member_index, others);
        }
    }
    let mut pinyin = HashMap::new();
    let n3 = reader.u16be()?;
    for _ in 0..n3 {
        let pair = reader.str8()?;
        if let Some((name, abbr)) = pair.rsplit_once('-') {
            pinyin.insert(name.to_string(), abbr.to_string());
        }
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(StationNames {
        aliases,
        same_city,
        pinyin,
    })
}

/// jlb.dat: rolling-stock consist rotations plus dining-car records.
/// Counters and dates are big-endian u32; the two per-record flag words
/// sit at byte offsets 8 and 12 (purpose unknown).
fn read_jlb(archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>) -> Result<Jlb, ParseError> {
    let bytes = read_member(archive, "jlb.dat")?;
    let mut reader = Reader::new(&bytes, "jlb.dat");
    let mut consists = Vec::new();
    let n_jlb = reader.u32be()?;
    for _ in 0..n_jlb {
        let start_date = reader.u32be()?;
        let end_date = reader.u32be()?;
        let flag8 = reader.u32be()?;
        let flag12 = reader.u32be()?;
        let agency = reader.u16_custom()?;
        let model = reader.str8()?;
        let rotation = reader.str8()?;
        consists.push(Consist {
            start_date,
            end_date,
            flag8,
            flag12,
            agency,
            model,
            rotation,
        });
    }
    let mut dining = Vec::new();
    let n_food = reader.u32be()?;
    for _ in 0..n_food {
        let train = reader.u32be()? as usize;
        let start_date = reader.u32be()?;
        let end_date = reader.u32be()?;
        let coach = reader.str8()?;
        dining.push(DiningCar {
            train,
            start_date,
            end_date,
            coach,
        });
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(Jlb { consists, dining })
}

/// routes.dat: the line table plus per-train route segments. All counters
/// and flags are big-endian u32; strings are length-prefixed UTF-8.
fn read_routes(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
) -> Result<Routes, ParseError> {
    let bytes = read_member(archive, "routes.dat")?;
    let mut reader = Reader::new(&bytes, "routes.dat");
    let mut lines = Vec::new();
    let line_count = reader.u32be()?;
    for _ in 0..line_count {
        let code = reader.u32be()?;
        let name = reader.str8()?;
        let n = reader.u32be()?;
        let mut stations = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let name = reader.str8()?;
            let telecode = reader.str8()?;
            let flag = reader.u32be()?;
            let dist = reader.u32be()?;
            stations.push(LineStation {
                name,
                telecode,
                flag,
                dist,
            });
        }
        lines.push(Line {
            code,
            name,
            stations,
        });
    }
    let line_by_code = lines
        .iter()
        .enumerate()
        .map(|(i, line)| (line.code, i))
        .collect();

    let mut entries = Vec::new();
    let train_count = reader.u32be()?;
    for _ in 0..train_count {
        let key = reader.str8()?;
        let terminal = reader.str8()?;
        let n = reader.u32be()?;
        let mut segments = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let station = reader.str8()?;
            let line = reader.u32be()?;
            segments.push((station, line));
        }
        entries.push(RouteEntry {
            key,
            terminal,
            segments,
        });
    }
    if !reader.at_end() {
        return Err(reader.truncated());
    }
    Ok(Routes {
        lines,
        line_by_code,
        entries,
    })
}

/// Validate untrusted shard bytes once, while retaining on-demand record decoding.
fn validate_shards(stations: &[Vec<u8>], trains: &[Vec<u8>]) -> Result<(), ParseError> {
    for (index, bytes) in stations.iter().enumerate() {
        let name = format!("S{index}.dat");
        let mut reader = Reader::new(bytes, &name);
        while !reader.at_end() {
            reader.u16_custom()?;
            let count = reader.u16_custom()?;
            reader.skip(2 * count)?;
        }
    }
    for (index, bytes) in trains.iter().enumerate() {
        validate_train_shard(bytes, &format!("T{index}.dat"))?;
    }
    Ok(())
}

fn validate_train_shard(bytes: &[u8], name: &str) -> Result<(), ParseError> {
    let mut reader = Reader::new(bytes, name);
    while !reader.at_end() {
        let train = reader.u16_custom()?;
        reader.skip(13)?; // kind, price reference, dates and running-day rule
        let count = reader.u16_custom()?;
        if count == 0 {
            return Err(ParseError::InvalidRecord(format!(
                "{name}: train {train} has no stops"
            )));
        }
        for stop in 0..count {
            reader.u16_custom()?; // station
            let hour = reader.byte()?;
            let minute = reader.byte()?;
            if hour > 23 || minute > 59 {
                return Err(ParseError::InvalidRecord(format!(
                    "{name}: train {train}, stop {stop} has invalid clock {hour:02}:{minute:02}"
                )));
            }
            reader.skip(3)?; // dwell and cumulative distance
        }
    }
    Ok(())
}
