# Changelog

This changelog records changes since the v2.0.0 beta series.

The versioned entries below record implementation-level changes.

## [2.0.0-rc1] - 2026-10-04

### 🚀 Features

- *(schema)* Require piecewise curve values and non-zero reset months (#891)
- *(schema)* Require a catchment's flow (#893)
- *(schema)* Remove unused and unfinished NodeBuilder struct. (#892)
- *(schema)* Derive Debug for MetricSet types. (#894)
- *(schema)* Revise the implementation of Reservoir compensation and spill. (#896)

### ⚙️ Miscellaneous Tasks

- Drop Python 3.10 support. (#890)
- *(release)* Prepare v2.0.0-rc1

## [2.0.0-beta12] - 2026-10-03

### 🚀 Features

- Make Arrow stream output timeseries columns. (#878)
- *(core)* Add a clock module so pywr-core runs on wasm (#881)
- *(schema)* Read model input files through a FileProvider (#880)
- Let a run write several Arrow streams (#882)
- Let an Arrow stream output write to memory (#884)
- *(schema)* Add validate_reference and validate_member
- *(schema)* Include local parameters in a node's validate ()
- *(schema)* Refuse a local parameter name used twice in one node
- *(schema)* Refuse a literal in a metric set's metrics
- *(schema)* Check a node as an edge end on its own

### 🐛 Bug Fixes

- Application of Link soft max. (#887)

### 💼 Other

- Let the core, schema and runner protocol crates build for wasm (#879)

### ⚙️ Miscellaneous Tasks

- Add workflow for running Clippy against wasm target (#883)

## [2.0.0-beta11] - 2026-09-30

### 🚀 Features

- *(schema)* Validate a param's fields in NetworkSchema::validate (#864)
- *(schema)* Validate a node's fields in NetworkSchema::validate (#866)
- *(schema)* Validate a virtual node's fields in NetworkSchema::validate (#867)
- Add clear provenance method to ComponentMeta trait. (#869)
- *(schema)* Validate field values that core can't run correctly (#870)
- *(schema)* Validate node refs against the attributes of the node they name (#872)
- *(schema)* Validate param references against the phase of the param they name (#873)
- *(schema)* Hold a problem's owner as a ProblemOwner, not a string (#875)
- Implement Display and report for project manifest validation. (#871)

### 🐛 Bug Fixes

- Record the phases a param calculates with the all_parameters filter (#858)

### 🚜 Refactor

- *(schema)* Move validation problems to validation.rs and split network.rs into a module (#863)

## [2.0.0-beta10] - 2026-09-28

### 🚀 Features

- *(schema)* Validate parameter references against the value type of the parameter they name (#854)
- *(schema)* Add discriminant types for RoutingMethod and MuskingumInitialCondition (#855)
- Initial commit of pywr-project. (#767)
- *(schema)* Validate table refs against the tables they name (#856)

### 🐛 Bug Fixes

- *(schema)* Return the phase of Max, Min, Negative, and HydropowerTarget params from Parameter::phase (#852)

## [2.0.0-beta9] - 2026-09-27

### 🚀 Features

- *(schema)* Validate the scenario domain and references to its groups (#827)
- *(schema)* Do not display error sources. (#840)
- Add PyArrow extension type for reading ArrowStreamOutput files. (#835)
- Add parameter phase options for threshold, multithreshold and indexedarray (#764)
- Add network step timing and speed. (#843)
- Name optional field defaults in the schema and require them in core (#841)
- Make all components have consistent meta data fields. (#846)
- *(schema)* Validate virtual node members in NetworkSchema::validate (#848)
- *(schema)* Export the node and parameter types missing from the re-exports (#849)
- Add parameter phases to the control curve parameters (#776)
- Refuse a delay of zero time-steps when a model loads (#850)

### 🐛 Bug Fixes

- *(core)* Do not display error sources. (#838)
- *(core)* Check an aggregated node's factors and report their errors. (#839)
- *(schema)* Apply a memory output's time and scenario funcs to the correct dimensions. (#844)
- *(schema)* Build a reservoir's evaporation node as an output (#845)
- Fix parameter loading with parent. (#847)

## [2.0.0-beta8] - 2026-09-23

### 🚀 Features

- Add microlp feature to pywr-schema. (#808)
- *(schema)* Validate edges in NetworkSchema::validate (#799)
- *(schema)* Re-export the output field types from pywr_schema::outputs (#813)
- *(schema)* Make AnyNonZero's tolerance public and derive Default (#814)
- *(schema)* Derive PartialEq, Eq, Display and EnumIter for ParameterPhase (#815)
- *(schema)* Add Node::components and derive EnumIter for component subsets (#818)
- Support after_hook method in Python class parameters. (#807)
- Replace polars with arrow-rs for internal timeseries handling. (#801)
- Add Cbc to random benchmarks. (#819)
- *(schema)* Validate parameter, table, timeseries and metric set names. (#821)
- *(schema)* Add Node::attributes and derive EnumIter for attribute subsets (#822)
- Rationalise the solver traits. (#820)
- Allow referencing node-local parameters from global namespace. (#809)
- Add built-in solver enums. (#825)
- *(schema)* Derive Display and EnumIter for ArrowFormat, SpillNodeType and ParameterReturnValue. (#828)
- Add ArrowStreamOutput. (#826)
- Initial commit of Pywr runner service. (#836)

### 🐛 Bug Fixes

- *(schema)* Load scalar tables with three or four keys as the right variant (#817)
- *(schema)* Visit paths in tables, virtual nodes and metric sets (#816)
- Correct some TimeSeries references and names. (#823)
- *(schema)* Make arrow and parquet optional under the core feature (#824)
- Make the Cbc solver safe for multiple solves. (#812)
- *(core)* Check a virtual storage node's factors against its nodes. (#829)

### ⚡ Performance

- Inform Clp about what has changed in the LP between solves. (#810)

### ⚙️ Miscellaneous Tasks

- Enable HTML reports for criterion. (#811)
- Enforce cargo fmt. (#834)

## [2.0.0-beta7] - 2026-09-15

### 🚀 Features

- Support metrics providing a value aligned to the current phase. (#766)
- Refactor and improve Python extension (#785)
- Add simple row bounds sense check. (#787)
- *(schema)* Validate the time domain in ModelSchema::validate (#798)
- Add phases to InterpolatedParameter, OffsetParameter, Polynomial1DParameter and VectorParameter (#777)
- *(core)* Write PYWR_VERSION and PYWR_VERSION_STR attrs to H5 outputs (#800)
- *(schema)* Replace VisitNodeReferences with an owner-aware VisitReferences (#796)

### 🐛 Bug Fixes

- Fix application of bypass_cost in RiverGauge node. (#786)
- *(schema)* Build local parameters defined on virtual nodes (#797)

### 🚜 Refactor

- Use log and env_logger crates instead of tracing. (#789)
- Improve variable names for Network references. (#790)
- Refactor ScenarioDomainBuilder (#792)

### 📚 Documentation

- Improve license notifications. (#774)

### ⚙️ Miscellaneous Tasks

- Fix useless format lint in CSV recorder.
- Use Cargo resolver version 3. (#788)
- Tidy-up dependencies. (#791)

## [2.0.0-beta6] - 2026-08-25

### ⚙️ Miscellaneous Tasks

- Fix wheel path for PyPI publish action. (#772)

## [2.0.0-beta5] - 2026-08-25

### 🚀 Features

- Use builder patterns in pywr-core.
- Initial separation of parameter calc traits.
- Make SimpleParameters only computed once per time-step.
- Convert scenario_combinations to v2 schema.
- Move to jiff datetime library.
- *(schema)* Enforce unique node names within a network (#751)
- Allow specifying calculation phase in some parameters. (#745)
- *(schema)* Add mutable metadata accessors and node construction from NodeType (#760)
- *(schema)* Add a visitor for node references in a schema (#759)
- Add phase option to division parameter (#763)
- Implement merge for NetworkSchema. (#762)
- Slot validation API (#758)
- Add phase option to DifferenceParameter (#757)
- Add phase options to max & min parameters (#765)

### 🐛 Bug Fixes

- *(schema)* Visit virtual nodes when visiting network metrics (#750)

### 🚜 Refactor

- *(core)* Add a context type to GeneralParameter<T>
- *(core)* Add a context type to SimpleParameter<T>

### ⚙️ Miscellaneous Tasks

- Lints, dependencies and warnings for Rust v1.97 (#719)
- Use the official PyPI GHA for publishing wheels. (#718)
- Remove unused proc-macro2 dependency in pywr-schema-macros (#742)

## [2.0.0-beta4] - 2026-07-09

### 🚀 Features

- Add meta method to Parameter (#692)
- Allow parameters to (optionally) return value in after. (#599)
- *(schema)* Add tags field to node and parameter metadata (#691)
- *(core)* Implement thread pool for multi-model solves. (#713)

### 🐛 Bug Fixes

- *(core)* Remove extern crate from pywr-core. (#654)
- *(core)* Fix incorrect default lower bound. (#671)
- *(core)* Fix memory leak in the Cbc solver. (#672)
- *(core)* Fix memory leak in the Clp solver. (#673)

### 🚜 Refactor

- Use consts for float equality tolerances (#714)

### 🧪 Testing

- Add some basic unit tests for ipm-common. (#613)

### ⚙️ Miscellaneous Tasks

- *(pedantic)* Enforce clippy::pedantic in pywr_core::aggregated_node. (#653)
- *(core)* Enforce clippy::pedantic in pywr_core::aggregated_storage_node. (#655)
- Fix clippy lints from Rust v1.95 (#664)
- Bump cmake to 0.1.58
- Build free-threaded wheels for Python 3.14t instead of 3.13t

## [2.0.0-beta3] - 2026-04-02

### 💼 Other

- No longer build wheels for musl Linux platforms.

### 📚 Documentation

- Switch to mermaid for node diagrams. (#620)

### ⚡ Performance

- Box ComponentConversionError when returned as an error. (#621)

### ⚙️ Miscellaneous Tasks

- Upgrade Python dependencies (uv.lock). (#611)
- Remove allowing unexpected_cfg lint required for earlier PyO3 versions. (#615)
- *(pedantic)* Apply clippy::pedantic lints to pywr_core::scenario module. (#616)
- Update to ubuntu-24.04 runners for Python wheels. (#618)
- Bump bytes to v1.11.1 (#614)
- Bump polars to v0.53.
- Upgrade to latest COIN-OR release. (#619)

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
