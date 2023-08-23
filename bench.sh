#!/usr/bin/env bash

cargo criterion --no-default-features --features highs,clipm --message-format=json random-models-hyper-scenarios > benches/random-models-hyper-scenarios.json
