#!/usr/bin/env bash

cargo +nightly criterion --no-default-features --features highs,ipm-ocl,ipm-simd --message-format=json random-models-scenarios > benches/random-models-scenarios3.json
