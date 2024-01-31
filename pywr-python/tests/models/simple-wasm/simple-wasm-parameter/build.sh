cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/simple_wasm_parameter.wasm ../simple_wasm_parameter.wasm

# Optionally optimise the WASM file
# wasm-opt -O3 -o ../simple_wasm_parameter.wasm ../simple_wasm_parameter.wasm
