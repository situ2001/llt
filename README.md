# llt

Rust parsing and timetable queries for Lulutong offline data packages. The
`llt-wasm` package provides JavaScript and WebAssembly bindings. Neither package
contains timetable data; callers supply their own offline data package bytes.

Build the bindings with a `wasm-bindgen` CLI matching the version in
`packages/llt-wasm/Cargo.toml`:

```sh
cargo build -p llt-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir packages/llt-wasm --out-name llt \
  target/wasm32-unknown-unknown/release/llt_wasm.wasm
```

The synthetic fixture test requires no external data. The `real-data-tests`
feature enables checks against locally supplied current and reference offline
data packages via `LLT_CURRENT_PACKAGE` and `LLT_REFERENCE_PACKAGE`. The
reference checks expect the 20260904 package.

The parser originated in [12306-parser](https://github.com/situ2001/12306-parser).
Its format was derived from analysis of Lulutong data; the library does not
include or distribute that data.

The examples and benchmark read a user-supplied offline data package from
`LLT_PACKAGE`, for example:

```sh
LLT_PACKAGE=/path/to/an.db cargo run --release -p llt --example train -- G1
```
