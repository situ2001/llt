# llt

Rust parsing and timetable queries for Lulutong offline data packages. The
`llt-wasm` package provides JavaScript and WebAssembly bindings. Neither package
contains timetable data; callers supply their own offline data package bytes.

Build rust crate and WASM.

```sh
node scripts/release.mjs install-bindgen
node scripts/release.mjs build
```

The examples and benchmark read a user-supplied offline data package from
`LLT_PACKAGE`, for example:

```sh
LLT_PACKAGE=/path/to/an.db cargo run --release -p llt --example train -- G1
```
