#!/usr/bin/env bash

cargo +nightly criterion --no-default-features --features highs,ipm-ocl,ipm-simd --message-format=json random-models-size > benches/random-models-size.json
cargo +nightly criterion --no-default-features --features highs,ipm-ocl,ipm-simd --message-format=json random-models-hyper-scenarios > benches/random-models-hyper-scenarios.json
cargo +nightly criterion --no-default-features --features highs,clipm --message-format=json random-models-threads > benches/random-models-threads.json
cargo +nightly criterion --no-default-features --features highs,ipm-ocl,ipm-simd --message-format=json random-models-ipm-convergence > benches/random-models-ipm-convergence.json
cargo +nightly criterion --no-default-features --features highs,ipm-ocl,ipm-simd --message-format=json random-models-olc-chunks > benches/random-models-ipm-convergence.json
