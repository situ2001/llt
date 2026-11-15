//! Parser for the Lulutong (路路通) offline timetable package `an.db`.
//!
//! The package is a JAR (ZIP) of binary members: `s.i`/`t.i` name indexes,
//! `S0–S9.dat` station→trains shards, `T0–T19.dat` train schedules and
//! `t.rule` running-day rules. Integer fields use a 2-byte 255-base encoding
//! (`b0 * 255 + b1`); dates are 4-byte little-endian base-128 packed
//! YYYYMMDD. Shards are addressed by `[(index + 1) % n]`.
//!
//! # Module map
//!
//! Three private modules sit behind the single public seam
//! (`Timetable::parse` + the query methods):
//!
//! - `reader` — the bounds-checked byte primitives (`Reader`); every wire
//!   encoding (255-base integers, base-128 dates, length-prefixed strings)
//!   is defined exactly once here.
//! - `package` — ZIP extraction plus one decoder per member, producing the
//!   crate-private `Package` (all decoded tables in one struct).
//! - `timetable` — the public interface, plus the on-demand shard decodes
//!   (`train`, `trains_through`), which go through the same byte reader.
//!
//! # Domain vocabulary (领域术语)
//!
//! The domain terms map
//! to this API as follows:
//!
//! | Code | Term (术语) |
//! |---|---|
//! | `Timetable` | the parsed data package (离线数据包) |
//! | `stations`, `trains` | station table (车站表) / train table (车次表) |
//! | `Train`, `Stop` | timetable record (车次时刻) and its calling points (停站) |
//! | `Stop::day` | derived day offset (跨日偏移) — never stored in the data |
//! | `Line`, `LineStation` | physical line (线路), telegraph codes (电报码) |
//! | `route_key_exact`, `expand_route`, `RouteNode` | via path (经由) — the geographic path, not the equipment |
//! | `Consist`, `rotation` | rolling-stock assignment (车底交路 / 轮转序列), incl. deadheads (空驶) |
//! | `train_no` | 12306 internal code (12306 内码) — the via table's key space |
//! | `RunStatus` | running-day status `[-]` `[+]` `[*]` (开行周期规则) |
//! | `query`, `Segment` | station-to-station query (站站查询): candidates (候选车次) → running set (当日开行) |
//! | `related_stations`, `aliases` | same-city groups (同城站组), virtual stations (站名别名) |
//! | `agency`, `units` | operating agency (担当单位) |
//! | `fuxing_groups`, `turnaround`, `is_s_train` | attributes (复兴号分组 / 折返站 / S车) |
//! | `display_name_at` | cross-day display switch (显示车次切换) |
//! | `region_name`, `place_region` | administrative regions (行政区划) |
//! | `pinyin_abbr` | pinyin initials (拼音缩写) |
//! | `dining_cars` | dining-car placement (餐车) |

mod package;
mod reader;
mod timetable;

pub use package::{Consist, DiningCar, Line, LineStation};
pub use timetable::{Date, RouteNode, RunStatus, Segment, Stop, Timetable, Train};

use std::fmt;

/// Errors produced while opening or decoding a package.
#[derive(Debug)]
pub enum ParseError {
    /// The input is not a readable ZIP/JAR archive.
    Archive,
    /// A required member is missing from the package.
    MissingMember(String),
    /// A member ends mid-record or contains undecodable data.
    Truncated(String),
    /// A timetable record contains unusable schedule data.
    InvalidRecord(String),
    /// A timetable record refers to an absent or unusable running-day rule.
    InvalidRule(usize),
    /// A calling point refers to a station absent from the station table.
    InvalidStation(usize),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidRecord(reason) => write!(f, "invalid timetable record: {reason}"),
            ParseError::Archive => write!(f, "not a valid an.db package (ZIP/JAR)"),
            ParseError::MissingMember(name) => write!(f, "package member `{name}` is missing"),
            ParseError::Truncated(name) => write!(f, "package member `{name}` is truncated"),
            ParseError::InvalidRule(index) => write!(
                f,
                "running-day rule {index} is missing or has a zero period"
            ),
            ParseError::InvalidStation(index) => write!(
                f,
                "calling point station index {index} is absent from the station table"
            ),
        }
    }
}

impl std::error::Error for ParseError {}
