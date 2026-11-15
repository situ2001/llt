# llt-wasm

WebAssembly bindings for the `llt` Rust crate. Initialize the generated module,
then pass offline data package bytes to `parseTimetable`. Call `free()` on the
returned timetable when finished. See `llt.d.ts` for the available queries.
