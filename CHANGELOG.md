# Changelog

All notable changes to this project since v2.0.0-beta will be documented in this file.

## [2.0.0-beta2] - 2025-12-19

### 🚀 Features

- Add filter to include all edges in a metric set. (#563)
- Allow specifying 365 values for daily profiles. (#566)
- Introduce NodeSlot enum for edges. (#569)

### 🐛 Bug Fixes

- Add TablesMeta for consistency with other objects. (#565)
- Swap "rows" and "cols" keys in table lookup definition. (#564)

### 🚜 Refactor

- Rename various schema types and errors. (#562)

### 📚 Documentation

- Fix references to table JSON examples. (#574)

### ⚙️ Miscellaneous Tasks

- Use Python 3.13 explicitly in actions. (#576)
- Pin mdbook to v0.4.52 (#577)
- Sort Cargo.toml files with cargo sort. (#584)
- Migrate to macos-15 runners. (#600)

## [2.0.0-beta1] - 2025-10-06

### 🚀 Features

- Allow the CBC solver to be used from Python. (#546)
- Add type hinting to Python convert functions. (#547)
- Add doc example tests and data for nodes. (#549)
- Initial implementation of hourly time-steps (#552)

### 🚜 Refactor

- Align Rust model struct names with Python class names. (#551)
