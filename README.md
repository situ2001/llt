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

The examples and benchmark read a user-supplied offline data package from
`LLT_PACKAGE`, for example:

```sh
LLT_PACKAGE=/path/to/an.db cargo run --release -p llt --example train -- G1
```
