#!/usr/bin/env bash

cargo +nightly criterion --no-default-features --features highs,clipm --message-format=json random-models-hyper-scenarios > benches/random-models-hyper-scenarios3.json


cargo +nightly criterion --no-default-features --features highs,clipm --message-format=json random-models-hyper-scenarios > benches/random-models-hyper-scenarios3.json
