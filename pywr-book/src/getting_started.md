# Installation

Pywr is both a Rust library and a Python package.

## Rust

TBC

## Python

Pywr requires Python 3.9 or later.
It is currently not available on PyPI, but wheels are available from the GitHub [actions](https://github.com/pywr/pywr-next/actions) page.
Navigate to the latest successful build, and download the archive and extract the wheel for your platform.

```bash
pip install pywr-2.0.0b0-cp312-none-win_amd64.whl
```
> **Note**: That current Pywr v2.x is in pre-release and may not be suitable for production use.
> If you require Pywr v1.x please use `pip install pywr<2`.


# Running a model

Pywr is a modelling system for simulating water resources systems.
Models are defined using a JSON schema, and can be run using the `pywr` command line tool.
Below is an example of a simple model definition `simple1.json`:

```json
{{#include ../../pywr-schema/src/test_models/simple1.json}}
```

To run the model, use the `pywr` command line tool:

```bash
python -m pywr run simple1.json
```
