<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
<!--
*** I'm using markdown "reference style" links for readability.
*** Reference links are enclosed in brackets [ ] instead of parentheses ( ).
*** See the bottom of this document for the declaration of the reference variables
*** for contributors-url, forks-url, etc. This is an optional, concise syntax you may use.
*** https://www.markdownguide.org/basic-syntax/#reference-style-links
-->
[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![MIT License][license-shield]][license-mit-url]
[![Apache License](https://img.shields.io/badge/License-Apache--2.0-green?style=for-the-badge)][license-apache-url]
[![LinkedIn][linkedin-shield]][linkedin-url]


<!-- PROJECT LOGO -->
<br />
<div align="center">

<!--
***  <a href="https://github.com/pywr/pywr-next">
***    <img src="images/logo.png" alt="Logo" width="80" height="80">
***  </a>
-->

<h3 align="center">Pywr-next</h3>

  <p align="center">
    The next major release of <a href="https://github.com/pywr/pywr">Pywr</a>, a water-resource allocation
    modelling system. Pywr v2 combines a Rust computational core with Python bindings and a command line interface.
    It is now at its first release candidate, moving beyond the experimental stage towards a stable v2.0 release.
    <br />
    <br />
    <a href="https://pywr.github.io/pywr-next/">User Guide</a>
    ·
    <a href="https://github.com/pywr/pywr-next/issues">Report Bug</a>
    ·
    <a href="https://github.com/pywr/pywr-next/issues">Request Feature</a>
  </p>
</div>



<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#benefits-over-pywr-v1x">Benefits over Pywr v1.x</a></li>
        <li><a href="#features">Features</a></li>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#installing-from-pypi">Installing from PyPI</a></li>
        <li><a href="#compiling-from-source">Compiling from source</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#porting-a-pywr-v1x-model-to-v2x">Porting a Pywr v1.x model to v2.x</a></li>
    <li><a href="#crates">Crates</a></li>
    <li><a href="#release-status">Release status</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
  </ol>
</details>



<!-- ABOUT THE PROJECT -->

## About The Project

Pywr simulates the allocation of water through a network of sources, stores, links and demands. Costs and constraints
control allocation at each time step, while parameters describe changing conditions and operating rules across
scenarios.

This repository contains Pywr v2. It retains the flexible, parameter-driven modelling approach of Pywr v1.x, with a
redesigned Rust core, a typed JSON model schema, and Python interfaces for running models and extending their behaviour.

### Benefits over Pywr v1.x

- **A reusable computational core.** The Rust engine can be used independently of Python, with separate crates for
  model schemas, project composition and command line tools. Python remains available for custom model logic and
  analysis.
- **Explicit model definitions and validation.** A typed JSON schema and structured validation help identify invalid
  references, connections and parameter types before simulation. JSON Schema can also be exported for external tooling.
- **Parallel scenario execution.** The engine supports running scenarios in parallel and releases Python's GIL during
  Rust model execution. Performance depends on the model, solver and use of Python callbacks rather than a universal
  speedup over v1.x.
- **Redesigned outputs and metrics.** Metric sets separate the quantities being recorded from their output format, with
  aggregation and in-memory results alongside CSV, HDF5 and Arrow outputs.
- **More explicit Python extensions.** Custom Python parameters declare their dependencies and can maintain per-scenario
  state, making calculation order and state ownership clearer.

Pywr v2 is not a drop-in replacement for v1.x: the JSON schema and Python API have changed. See the
[migration guidance below](#porting-a-pywr-v1x-model-to-v2x) before upgrading existing models.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

### Features

- Network components for reservoirs, catchments, abstractions, losses, treatment works and hydropower, including
  virtual and aggregated nodes.
- A parameter system with profiles, control curves, thresholds, arithmetic, interpolation, rolling calculations,
  delays and custom Python functions or classes.
- Delay and Muskingum river routing, and multi-network models with inter-network transfers.
- Multiple optimisation backends, including Clp, HiGHS and Cbc, with additional solver options in the Rust crates.
- Native CSV, Arrow IPC and Parquet time-series input, plus Python-backed data loaders.
- Metric aggregation and CSV, HDF5, Arrow-stream and in-memory outputs for subsequent analysis in Python.
- Multi-file project composition and command line tools for running models, converting v1.x files and exporting schemas.

See the [User Guide](https://pywr.github.io/pywr-next/) for model concepts, supported components and examples.

### Built With

[![Rust][Rust]][Rust-url]
[![Python][Python]][Python-url]


<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- GETTING STARTED -->

## Getting started

### Installing from PyPI

The Python package is named **`pywr`** and requires **Python 3.10 or later**. Use a separate virtual environment when
trying v2 alongside an existing v1.x installation.

To install the v2 release candidate (`2.0.0rc1`) or a newer v2 release from PyPI:

```bash
python -m pip install --upgrade --pre "pywr>=2.0.0rc1,<3"
```

Once the stable v2 release is published, use:

```bash
python -m pip install --upgrade "pywr>=2,<3"
```

The version constraint selects v2 rather than v1.x. To remain on v1.x, use `python -m pip install "pywr<2"` instead.

Optional extras are available for data integrations: `pandas`, `polars`, `excel` and `hdf`. For example, to install the
v2 release candidate with Pandas and Excel support:

```bash
python -m pip install --upgrade --pre "pywr[pandas,excel]>=2.0.0rc1,<3"
python -m pywr --help
```

Wheel builds target Linux x86-64, Windows x64, and macOS Intel and Apple Silicon. Installing a compatible wheel does not
require Rust or a C/C++ compiler. If no wheel is available for your platform and Python version, a source build is
required.
See the [installation guide](https://pywr.github.io/pywr-next/getting_started.html) for more details.

### Compiling from source

Source builds require a current stable Rust toolchain, Python 3.10 or later, C/C++ build tools, CMake, and
Clang/libclang
for native dependencies. The bundled COIN-OR solvers use Git submodules; initialise them before building.

From the repository root, create a Python development installation using Maturin:

```bash
git submodule update --init --recursive
python -m venv .venv # create a new virtual environment
source .venv/bin/activate # activate the virtual environment (linux)
# .venv\Scripts\activate # activate the virtual environment (windows)
python -m pip install "maturin>=1.15,<2"
maturin develop --release # build and install the Python extension
python -m pywr --help
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- USAGE EXAMPLES -->

## Usage

### Rust CLI

The `pywr-cli` crate provides commands for running single-network, multi-network and project models, converting v1.x
models, and exporting JSON Schema. The commands below are run from the repository root. The default build includes
Python support for models that use Python extensions.

To see the CLI commands available run the following:

```bash
cargo run --release -p pywr-cli -- --help
```

To run a Pywr v2 model use the following:

```bash
cargo run --release -p pywr-cli -- run pywr-schema/tests/simple1.json
```

### Python CLI

After installing from PyPI or building from source, run a model with `python -m pywr run path/to/model.json`.
For example, from a checkout of this repository:

```bash
python -m pywr run pywr-schema/tests/simple1.json
```

Use `python -m pywr run --help` for solver, input-data and output-directory options. The Python CLI supports Clp (the
default), HiGHS and Cbc. The example above writes an HDF5 output file.

## Porting a Pywr v1.x model to v2.x

Pywr v2 uses a new JSON schema and Python API. Existing v1.x models must be migrated; upgrading the Python package alone
is not sufficient. The Rust CLI includes a conversion tool to help translate v1.x JSON models:

```bash
cargo run --release -p pywr-cli -- convert old-model.json converted-model.json --stop-on-error
```

**Conversion is a starting point, not a guarantee of an equivalent model.** Not all v1.x features are supported, and
without `--stop-on-error` the converter may produce a partial model alongside conversion errors. Review all diagnostics,
complete the migration manually, and compare model outputs before relying on the converted model.

In particular:

- Tables and recorders/outputs are not automatically migrated; configure v2 data sources, metric sets and outputs.
- Custom Python parameters need updating to the new interface.
- Input time series must match the model's time resolution; v2 does not automatically resample them as v1.x did.

See the [migration guide](https://pywr.github.io/pywr-next/migration_guide.html) for details. Feedback on porting models
is welcome via [GitHub issues](https://github.com/pywr/pywr-next/issues).


<!-- _For more examples, please refer to the [Documentation](https://example.com)_ -->

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CRATES -->

## Crates

This repository contains the following crates:

### Pywr-core

A low-level Rust library for constructing network models. This crate interfaces with linear program solvers.

Feature flags (defaults for this crate when used directly):

| Feature    | Description                          | Default |
|------------|--------------------------------------|---------|
| `pyo3`     | Enable Python integration.           | Yes     |
| `clp`      | Enable the Clp LP solver.            | No      |
| `cbc`      | Enable the Cbc MILP solver.          | No      |
| `highs`    | Enable the HiGHS solver.             | No      |
| `microlp`  | Enable the pure-Rust MicroLP solver. | No      |
| `ipm-ocl`  | Enable the OpenCL IPM solver.        | No      |
| `ipm-simd` | Enable the SIMD IPM solver.          | No      |
| `hdf5`     | Enable HDF5 output.                  | No      |

### Pywr-schema

A Rust library for validating Pywr JSON files against a schema, and then building a model from the schema using
`pywr-core`.

Feature flags (defaults for this crate when used directly):

| Feature    | Description                                | Default |
|------------|--------------------------------------------|---------|
| `core`     | Build executable models using `pywr-core`. | Yes     |
| `pyo3`     | Enable Python integration.                 | Yes     |
| `clp`      | Enable the Clp LP solver.                  | Yes     |
| `hdf5`     | Enable HDF5 support.                       | Yes     |
| `cbc`      | Enable the Cbc MILP solver.                | No      |
| `highs`    | Enable the HiGHS solver.                   | No      |
| `microlp`  | Enable the pure-Rust MicroLP solver.       | No      |
| `ipm-ocl`  | Enable the OpenCL IPM solver.              | No      |
| `ipm-simd` | Enable the SIMD IPM solver.                | No      |

For schema validation and manipulation without the simulation engine, use `default-features = false`.
Solver availability in applications depends on their enabled features and exposed interfaces; OpenCL also requires
a suitable runtime and device.

### Pywr-cli

A command line interface for running Pywr models.

### Pywr-python

A Python extension (and package) for constructing and running Pywr models.

### Pywr-project

Schemas and composition tools for projects that assemble models from multiple files and options.

### Supporting crates

- `pywr-runner-engine`, `pywr-runner-service`, `pywr-runner-protocol` and `pywr-runner-transport`: local model execution
  service, protocol and transport.
- `coin-or-sys`: bindings to the bundled COIN-OR solvers.
- `ipm-common`, `ipm-simd` and `ipm-ocl`: interior-point solver implementations and shared utilities.
- `pywr-schema-macros`: procedural macros supporting the model schema.

<!-- RELEASE STATUS -->

## Release status

Pywr v2 is now at **2.0.0-rc1**, its first release candidate (Python version **2.0.0rc1**). The core modelling engine,
Python bindings, schema validation and redesigned output system are implemented. This marks the transition out of
the experimental stage towards the first stable v2.0 release. The release candidate is still a prerelease; use the
`--pre` installation command above until the stable release is published.

Release readiness does not imply complete feature parity or backwards compatibility with v1.x. Testing representative
models, checking migration results and reporting issues with the release candidate are especially valuable ahead of
the stable release.

See the [changelog](CHANGELOG.md), [releases](https://github.com/pywr/pywr-next/releases) and
[open issues](https://github.com/pywr/pywr-next/issues) for release notes, planned work and known limitations.

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- CONTRIBUTING -->

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any
contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also
simply open an issue with the tag "enhancement". Don't forget to give the project a star! Thanks again!

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>


<!-- LICENSE -->

## License

The Pywr code in this repository is dual-licensed under the [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) license.
Bundled third-party components have additional licensing terms, including EPL-2.0 for COIN-OR components in the Python
distribution. See [NOTICE](NOTICE) and the bundled license files for details.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTACT -->

## Contact

James Tomlinson - tomo.bbe@gmail.com

Project Link: [https://github.com/pywr/pywr-next](https://github.com/pywr/pywr-next)

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->

[contributors-shield]: https://img.shields.io/github/contributors/pywr/pywr-next.svg?style=for-the-badge

[contributors-url]: https://github.com/pywr/pywr-next/graphs/contributors

[forks-shield]: https://img.shields.io/github/forks/pywr/pywr-next.svg?style=for-the-badge

[forks-url]: https://github.com/pywr/pywr-next/network/members

[stars-shield]: https://img.shields.io/github/stars/pywr/pywr-next.svg?style=for-the-badge

[stars-url]: https://github.com/pywr/pywr-next/stargazers

[issues-shield]: https://img.shields.io/github/issues/pywr/pywr-next.svg?style=for-the-badge

[issues-url]: https://github.com/pywr/pywr-next/issues

[license-shield]: https://img.shields.io/github/license/pywr/pywr-next.svg?style=for-the-badge

[license-mit-url]: https://github.com/pywr/pywr-next/blob/main/LICENSE-MIT

[license-apache-url]: https://github.com/pywr/pywr-next/blob/main/LICENSE-APACHE

[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=for-the-badge&logo=linkedin&colorB=555

[linkedin-url]: https://linkedin.com/in/james-tomlinson-a465352b

[Rust]: https://img.shields.io/badge/rust-ef4a23?style=for-the-badge&logo=rust&logoColor=white

[Rust-url]: https://www.rust-lang.org/

[Python]: https://img.shields.io/badge/python-275277?style=for-the-badge&logo=python&logoColor=white

[Python-url]: https://www.python.org/

Copyright (C) 2020-2026 James Tomlinson Associates Ltd.
