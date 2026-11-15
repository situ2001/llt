//! wasm-bindgen bindings over llt. Beyond the loader seam
//! (dbloader ⇒ parser: raw `an.db` package bytes in, parsed [`Timetable`]
//! out) this now exposes the per-train detail query: resolve a train name
//! to an index, decode its timetable record, and judge its running-day
//! status (开行判定).

use serde::Serialize;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::UnwrapThrowExt;
use wasm_bindgen::prelude::*;

fn to_js<T: Serialize + ?Sized>(value: &T) -> JsValue {
    serde_wasm_bindgen::to_value(value).expect_throw("infallible serialization")
}

/// A parsed offline data package (离线数据包), held in wasm memory.
#[wasm_bindgen]
pub struct Timetable(llt::Timetable);

/// Parse a raw `an.db` package from the full JAR/ZIP bytes.
///
/// # Errors
/// Throws a JS `Error` when the bytes are not a readable package.
#[wasm_bindgen(js_name = parseTimetable)]
pub fn parse_timetable(package: &[u8]) -> Result<Timetable, JsError> {
    llt::Timetable::parse(package)
        .map(Timetable)
        .map_err(JsError::from)
}

#[wasm_bindgen]
impl Timetable {
    /// Size of the station table (车站表).
    #[wasm_bindgen(getter, js_name = "stationCount")]
    pub fn station_count(&self) -> usize {
        self.0.stations().len()
    }

    /// Size of the train table (车次表).
    #[wasm_bindgen(getter, js_name = "trainCount")]
    pub fn train_count(&self) -> usize {
        self.0.trains().len()
    }

    /// The station table (车站表), in index order.
    pub fn stations(&self) -> JsValue {
        to_js(self.0.stations())
    }

    /// Physical-line station sequences, independent of OSM geometry.
    pub fn lines(&self) -> JsValue {
        to_js(self.0.lines())
    }

    /// The train table (车次表), in index order.
    pub fn trains(&self) -> JsValue {
        to_js(self.0.trains())
    }

    /// Index of `name` in the station table (车站表), or `undefined`.
    pub fn station_index(&self, name: &str) -> Option<u32> {
        self.0.station_index(name).map(|i| i as u32)
    }

    /// Index of `name` in the train table (车次表), or `undefined`.
    pub fn train_index(&self, name: &str) -> Option<u32> {
        self.0.train_index(name).map(|i| i as u32)
    }

    /// Indices of all trains calling at `station`, in the package's shard
    /// order. The caller can decode each index with [`Timetable::train`].
    /// Throws on malformed shard records.
    pub fn trains_through(&self, station: u32) -> Result<JsValue, JsError> {
        Ok(to_js(&self.0.trains_through(station as usize)?))
    }

    /// Decode the timetable record (车次时刻) of the train at `index`,
    /// or `null` when the index is out of range.
    /// Throws on truncated records or dangling station references.
    pub fn train(&self, index: u32) -> Result<JsValue, JsError> {
        Ok(self
            .0
            .train(index as usize)?
            .map(|train| to_js(&train))
            .unwrap_or(JsValue::NULL))
    }

    /// Dated rolling-stock assignments matching an exact timetable train name.
    pub fn consists_for(&self, name: &str) -> JsValue {
        to_js(&self.0.consists_for(name))
    }

    /// Expanded via path in travel order, including passing stations, or null
    /// when this train has no complete exact route entry.
    pub fn route(&self, index: u32) -> JsValue {
        self.0
            .route_key_exact(index as usize)
            .and_then(|key| self.0.expand_route(key))
            .map(|nodes| to_js(&nodes))
            .unwrap_or(JsValue::NULL)
    }

    /// Whether the train at `index` runs on `date` (开行状态):
    /// `"Runs" | "NotStarted" | "Ended" | "OffDay"`.
    ///
    /// # Errors
    /// Throws when `index` is out of range or `date` is not a
    /// `{year, month, day}` object, the record is malformed, or its running-day
    /// rule is absent or has a zero period.
    pub fn runs_on(&self, index: u32, date: JsValue) -> Result<JsValue, JsError> {
        let train = self
            .0
            .train(index as usize)?
            .ok_or_else(|| JsError::new(&format!("train index {index} out of range")))?;
        let date: llt::Date = from_value(date).map_err(JsError::from)?;
        Ok(to_js(&self.0.runs_on(&train, date)?))
    }

    /// The train number displayed at `stop` (显示车次切换), or `undefined`
    /// when the package has no switch record for this train.
    pub fn display_name_at(&self, index: u32, stop: u32) -> Option<String> {
        self.0
            .display_name_at(index as usize, stop as usize)
            .map(str::to_owned)
    }

    /// The 12306 internal code (12306 内码) of the train, or `undefined`.
    pub fn train_no(&self, index: u32) -> Option<String> {
        self.0.train_no(index as usize).map(str::to_owned)
    }

    /// Offline operating agency, when assigned.
    pub fn agency(&self, index: u32) -> Option<String> {
        self.0.agency(index as usize).map(str::to_owned)
    }

    /// Service's home province, when recorded.
    pub fn home_province(&self, index: u32) -> Option<String> {
        self.0.home_province(index as usize).map(str::to_owned)
    }

    pub fn fuxing_groups(&self, index: u32) -> u8 {
        self.0.fuxing_groups(index as usize)
    }

    pub fn turnaround(&self, index: u32) -> JsValue {
        to_js(self.0.turnaround(index as usize))
    }

    pub fn is_suburban(&self, index: u32) -> bool {
        self.0.is_s_train(index as usize)
    }

    pub fn dining_cars(&self, index: u32) -> JsValue {
        to_js(&self.0.dining_cars(index as usize))
    }
    /// Running trains that visit `from` before `to` on the given date.
    pub fn query(&self, from: u32, to: u32, date: JsValue) -> Result<JsValue, JsError> {
        let date: llt::Date = from_value(date).map_err(JsError::from)?;
        Ok(to_js(&self.0.query(from as usize, to as usize, date)?))
    }

    pub fn route_keys_for(&self, name: &str) -> JsValue {
        to_js(&self.0.route_keys_for(name))
    }

    pub fn route_key_exact(&self, index: u32) -> Option<String> {
        self.0.route_key_exact(index as usize).map(str::to_owned)
    }

    pub fn expand_route(&self, key: &str) -> JsValue {
        self.0
            .expand_route(key)
            .map(|route| to_js(&route))
            .unwrap_or(JsValue::NULL)
    }

    pub fn units(&self) -> JsValue {
        to_js(self.0.units())
    }

    pub fn consists(&self) -> JsValue {
        to_js(self.0.consists())
    }

    pub fn aliases(&self) -> JsValue {
        to_js(self.0.aliases())
    }

    pub fn related_stations(&self, station: u32) -> JsValue {
        to_js(self.0.related_stations(station as usize))
    }

    pub fn pinyin_abbr(&self, station: u32) -> Option<String> {
        self.0.pinyin_abbr(station as usize).map(str::to_owned)
    }

    pub fn region_name(&self, id: u32) -> Option<String> {
        self.0.region_name(id).map(str::to_owned)
    }

    pub fn place_region(&self, name: &str) -> Option<String> {
        self.0.place_region(name).map(str::to_owned)
    }

    pub fn display_flags(&self, index: u32) -> Option<u16> {
        self.0.display_flags(index as usize)
    }
}
