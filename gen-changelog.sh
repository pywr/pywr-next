#!/usr/bin/env bash

set -e
git-cliff -o CHANGELOG.md v2.0.0-beta.. --bump
