# Installation

Pywr is both a Rust library and a Python package.

## Rust

The Rust CLI is available from a checkout of the repository. Install a Rust toolchain, initialise the bundled solver
submodules, and run the following from the repository root:

```bash
git submodule update --init --recursive
cargo run --release -p pywr-cli -- --help
```

The Python package does not install the separate Rust CLI. See the
[README](https://github.com/pywr/pywr-next#compiling-from-source) for source-build requirements.

## Python

Pywr v2 requires Python 3.11 or later. The first release candidate is **2.0.0rc1** (not yet the stable v2 release).
Use a separate virtual environment if you also use v1.x. To remain on v1.x, install it with
`python -m pip install "pywr<2"` instead.

### Installing from PyPI

#### Using pip and venv

It is recommended to install Pywr into a virtual environment.

```bash
python -m venv .venv
source .venv/bin/activate  # On Windows use `.venv\Scripts\activate`
python -m pip install --upgrade --pre "pywr>=2.0.0rc1,<3"
```

#### Using uv

Alternatively, you can use `uv` to create and manage virtual environments:

```bash
uv init my-project
cd my-project
uv add --prerelease allow "pywr>=2.0.0rc1,<3"
```

The version constraint selects v2 rather than v1.x. Once the stable v2 release is published, install it with
`python -m pip install --upgrade "pywr>=2,<3"` (no `--pre` required).
Optional extras for data integrations include `pandas`, `polars`, `excel` and `hdf`;
for example, use `"pywr[pandas,excel]>=2.0.0rc1,<3"` with the pip command above.

### Installing from a wheel

Use PyPI as the primary source of release wheels. Wheel builds target Linux x86-64, Windows x64, and macOS Intel and
Apple Silicon. If you need a wheel from a development build, download the relevant artifact from GitHub
[Actions](https://github.com/pywr/pywr-next/actions), extract it, and install the `.whl` file compatible with your
Python version and platform using `python -m pip install /path/to/downloaded.whl`.
If no compatible wheel is available, installation requires a source build and native build tools.

## Checking the installation

To verify the installation, show the Python CLI help:

```bash
python -m pywr --help
```

# Running a model

Pywr is a modelling system for simulating water resources systems.
Models are defined using a JSON schema and can be run using the Python CLI.
Below is an example of a simple model definition `simple1.json`:

[//]: # (@formatter:off)

```json
{{#include ../../pywr-schema/tests/simple1.json}}
```
[//]: # (@formatter:on)

To run the model, use the Python CLI:

```bash
python -m pywr run simple1.json
```
