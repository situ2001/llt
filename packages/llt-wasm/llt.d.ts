/* tslint:disable */
/* eslint-disable */

/**
 * A parsed offline data package (离线数据包), held in wasm memory.
 */
export class Timetable {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Offline operating agency, when assigned.
     */
    agency(index: number): string | undefined;
    aliases(): any;
    consists(): any;
    /**
     * Dated rolling-stock assignments matching an exact timetable train name.
     */
    consists_for(name: string): any;
    dining_cars(index: number): any;
    display_flags(index: number): number | undefined;
    /**
     * The train number displayed at `stop` (显示车次切换), or `undefined`
     * when the package has no switch record for this train.
     */
    display_name_at(index: number, stop: number): string | undefined;
    expand_route(key: string): any;
    fuxing_groups(index: number): number;
    /**
     * Service's home province, when recorded.
     */
    home_province(index: number): string | undefined;
    is_suburban(index: number): boolean;
    /**
     * Physical-line station sequences, independent of OSM geometry.
     */
    lines(): any;
    pinyin_abbr(station: number): string | undefined;
    place_region(name: string): string | undefined;
    /**
     * Running trains that visit `from` before `to` on the given date.
     */
    query(from: number, to: number, date: any): any;
    region_name(id: number): string | undefined;
    related_stations(station: number): any;
    /**
     * Expanded via path in travel order, including passing stations, or null
     * when this train has no complete exact route entry.
     */
    route(index: number): any;
    route_key_exact(index: number): string | undefined;
    route_keys_for(name: string): any;
    /**
     * Whether the train at `index` runs on `date` (开行状态):
     * `"Runs" | "NotStarted" | "Ended" | "OffDay"`.
     *
     * # Errors
     * Throws when `index` is out of range or `date` is not a
     * `{year, month, day}` object, the record is malformed, or its running-day
     * rule is absent or has a zero period.
     */
    runs_on(index: number, date: any): any;
    /**
     * Index of `name` in the station table (车站表), or `undefined`.
     */
    station_index(name: string): number | undefined;
    /**
     * The station table (车站表), in index order.
     */
    stations(): any;
    /**
     * Decode the timetable record (车次时刻) of the train at `index`,
     * or `null` when the index is out of range.
     * Throws on truncated records or dangling station references.
     */
    train(index: number): any;
    /**
     * Index of `name` in the train table (车次表), or `undefined`.
     */
    train_index(name: string): number | undefined;
    /**
     * The 12306 internal code (12306 内码) of the train, or `undefined`.
     */
    train_no(index: number): string | undefined;
    /**
     * The train table (车次表), in index order.
     */
    trains(): any;
    /**
     * Indices of all trains calling at `station`, in the package's shard
     * order. The caller can decode each index with [`Timetable::train`].
     * Throws on malformed shard records.
     */
    trains_through(station: number): any;
    turnaround(index: number): any;
    units(): any;
    /**
     * Size of the station table (车站表).
     */
    readonly stationCount: number;
    /**
     * Size of the train table (车次表).
     */
    readonly trainCount: number;
}

/**
 * Parse a raw `an.db` package from the full JAR/ZIP bytes.
 *
 * # Errors
 * Throws a JS `Error` when the bytes are not a readable package.
 */
export function parseTimetable(_package: Uint8Array | number[]): Timetable;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_timetable_free: (a: number, b: number) => void;
    readonly parseTimetable: (a: number, b: number) => [number, number, number];
    readonly timetable_agency: (a: number, b: number) => [number, number];
    readonly timetable_aliases: (a: number) => any;
    readonly timetable_consists: (a: number) => any;
    readonly timetable_consists_for: (a: number, b: number, c: number) => any;
    readonly timetable_dining_cars: (a: number, b: number) => any;
    readonly timetable_display_flags: (a: number, b: number) => number;
    readonly timetable_display_name_at: (a: number, b: number, c: number) => [number, number];
    readonly timetable_expand_route: (a: number, b: number, c: number) => any;
    readonly timetable_fuxing_groups: (a: number, b: number) => number;
    readonly timetable_home_province: (a: number, b: number) => [number, number];
    readonly timetable_is_suburban: (a: number, b: number) => number;
    readonly timetable_lines: (a: number) => any;
    readonly timetable_pinyin_abbr: (a: number, b: number) => [number, number];
    readonly timetable_place_region: (a: number, b: number, c: number) => [number, number];
    readonly timetable_query: (a: number, b: number, c: number, d: any) => [number, number, number];
    readonly timetable_region_name: (a: number, b: number) => [number, number];
    readonly timetable_related_stations: (a: number, b: number) => any;
    readonly timetable_route: (a: number, b: number) => any;
    readonly timetable_route_key_exact: (a: number, b: number) => [number, number];
    readonly timetable_route_keys_for: (a: number, b: number, c: number) => any;
    readonly timetable_runs_on: (a: number, b: number, c: any) => [number, number, number];
    readonly timetable_stationCount: (a: number) => number;
    readonly timetable_station_index: (a: number, b: number, c: number) => number;
    readonly timetable_stations: (a: number) => any;
    readonly timetable_train: (a: number, b: number) => [number, number, number];
    readonly timetable_trainCount: (a: number) => number;
    readonly timetable_train_index: (a: number, b: number, c: number) => number;
    readonly timetable_train_no: (a: number, b: number) => [number, number];
    readonly timetable_trains: (a: number) => any;
    readonly timetable_trains_through: (a: number, b: number) => [number, number, number];
    readonly timetable_turnaround: (a: number, b: number) => any;
    readonly timetable_units: (a: number) => any;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
