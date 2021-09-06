# Simple WASM parameter test

This is a basic test of the WASM parameter. The test WASM is generated from Rust
using the `wasm-unknown-unknown` target. There's a small cargo project that
can be used to generate the WASM if required:

```
cd tests/models/simple-wasm/simple-wasm-parameter
cargo build --target wasm32-unknown-unknown --release
```
