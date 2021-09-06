#!/usr/bin/env bash
set -e
maturin develop
pip install -e .
