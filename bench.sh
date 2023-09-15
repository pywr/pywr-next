#!/usr/bin/env bash

set -e

ODIR=benches/$1
FEATURES="ipm-ocl,ipm-simd"

mkdir -p "${ODIR}"

cargo +nightly criterion --no-default-features --features ${FEATURES} --message-format=json -- random-models-size > "${ODIR}/random-models-size.json"
cargo +nightly criterion --no-default-features --features ${FEATURES} --message-format=json -- random-models-scenarios > "${ODIR}/random-models-scenarios.json"
cargo +nightly criterion --no-default-features --features ${FEATURES} --message-format=json -- random-models-hyper-scenarios > "${ODIR}/random-models-hyper-scenarios.json"
cargo +nightly criterion --no-default-features --features ${FEATURES} --message-format=json -- random-models-threads > "${ODIR}/random-models-threads.json"
cargo +nightly criterion --no-default-features --features ${FEATURES} --message-format=json -- random-models-ipm-convergence > "${ODIR}/random-models-ipm-convergence.json"
# cargo +nightly criterion --no-default-features --features ipm-ocl --message-format=json random-models-ocl-chunks > "${ODIR}/random-models-ocl-chunks.json"
