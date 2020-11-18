# Pywr-next

This repository is an experimental repository exploring ideas for a major revision to Pywr using Rust as a backend. It
is currently not ready for use beyond development and experimentation. Comments and discussions are welcome.

## Motivation

Pywr-1.x is a Python library which utilises Cython for performance. Over time this has resulted in a "core"
set of data structures and objects that are written in Cython to gain maximum performance. Cython has the nice
benefit of making it easy to extend that core functionality using regular Python. However, the border between what
is Python and what is Cython is a bit blurred and not well designed in certain places.

One option for the future development of Pywr (e.g. Pywr-2.x) would be a more explicit separate between the compute
"core" and higher level functionality. Rust is a candidate for writing that core largely independent of Python, and
possibly offers the benefits of (1) greater performance than Cython, and (2) easier maintenance in the future. 

## Requirements

Any major revision to Pywr will have the following feature requirements:

 - Retain the "Parameter" system from Pywr-1.x - this is core functionality that makes Pywr really flexible.
 - Extendable in Python space. 
 - An improved approach for outputting data and metrics.
 - Better error handling.
 - Cross-platform.
 - Faster!
 - Strong input file (JSON) schema.

## Development installation instructions

Rust and GLPK are required for installation. To build the shared library run: 

```bash
cargo build
```

This will create a shared library (.so or .dll) or `target/debug` (e.g. `libpywr.so`). To use the Python interface in
 development symlink or copy this shared library to the `pywr` folder as `pywr.so` or `pywr.pyd`. See the 
 [Pyo3 instructions](https://pyo3.rs/v0.12.3/index.html) for more information.
 
Once this is complete the following will run a simple test script of some basic models via Python.
 
```bash
python -m pywr
```


Copyright (C) 2020 James Tomlinson Associates Ltd.
