# Installation

Pywr is both a Rust library and a Python package.

## Rust

TBC

## Python

Pywr requires Python 3.10 or later.
It is currently available on PyPI as a pre-release.

> **Note**: That current Pywr v2.x is in pre-release and may not be suitable for production use.
> If you require Pywr v1.x please use `pip install pywr<2`.

### Installing from PyPI (pre-release)

#### Using pip and venv

It is recommended to install Pywr into a virtual environment.

```bash
python -m venv .venv
source .venv/bin/activate  # On Windows use `.venv\Scripts\activate`
pip install pywr --pre
```

#### Using uv

Alternatively, you can use `uv` to create and manage virtual environments:

```bash
uv init my-project
cd my-project
uv add pywr --pre
```

### Installing from a wheel

Alternatively, wheels are available from the
GitHub [actions](https://github.com/pywr/pywr-next/actions) page.
Navigate to the latest successful build, and download the archive and extract the wheel for your platform.

```bash
pip install pywr-2.0.0b0-cp310-abi3-win_amd64.whl
```

## Checking the installation

To verify the installation, run to see the command line help:

```bash
python -m pywr --help
```

# Running a model

Pywr is a modelling system for simulating water resources systems.
Models are defined using a JSON schema, and can be run using the `pywr` command line tool.
Below is an example of a simple model definition `simple1.json`:

[//]: # (@formatter:off)

```json
{{#include ../../pywr-schema/tests/simple1.json}}
```
[//]: # (@formatter:on)

To run the model, use the `pywr` command line tool:

```bash
python -m pywr run simple1.json
```
