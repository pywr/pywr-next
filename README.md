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
[![MIT License][license-shield]][license-url]
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
    This is repository contains the current work-in-progress for the next major revision to
    <a href="https://github.com/pywr/pywr">Pywr.</a> It uses Rust as a backend instead of Cython. It
    is currently not ready for use beyond development and experimentation. Comments and discussions are welcome.
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
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>



<!-- ABOUT THE PROJECT -->

## About The Project

Pywr-1.x is a Python library which utilises Cython for performance. Over time this has resulted in a "core"
set of data structures and objects that are written in Cython to gain maximum performance. Cython has the nice
benefit of making it easy to extend that core functionality using regular Python. However, the border between what
is Python and what is Cython is a bit blurred and not well designed in certain places.

One option for the future development of Pywr (e.g. Pywr-2.x) would be a more explicit separation between the compute
"core" and higher level functionality. Rust is a candidate for writing that core largely independent of Python, and
possibly offers the benefits of (1) greater performance than Cython, and (2) easier maintenance in the future.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

### Requirements

Any major revision to Pywr will have the following feature requirements:

- Retain the "Parameter" system from Pywr-1.x - this is core functionality that makes Pywr really flexible.
- Extendable in Python space.
- An improved approach for outputting data and metrics.
- Better error handling.
- Cross-platform.
- Faster!
- Strong input file (JSON) schema.

### Built With

[![Rust][Rust]][Rust-url]
[![Python][Python]][Python-url]


<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- GETTING STARTED -->

### Getting started

#### Installing pre-compiled wheels

See instructions in the [Pywr book](https://pywr.github.io/pywr-next/getting_started.html).

#### Compiling from source

This repository contains a version of Clp using Git submodules. In order to build those submodules
must be initialised first.

```bash
git submodule init
git submodule update
```

Rust is required for installation of the Python extension. To create a Python development installation
requires first compiling the Rust library and then the Python extension. The following example uses
a virtual environment to install the Python dependencies, compile the Pywr extension and run the Pywr Python CLI.

```bash
python -m venv .venv # create a new virtual environment
source .venv/bin/activate # activate the virtual environment (linux)
# .venv\Scripts\activate # activate the virtual environment (windows)
pip install maturin  # install maturin for building the Python extension
maturin develop # compile the Pywr Python extension
python -m pywr  # run the Pywr Python CLI
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- USAGE EXAMPLES -->

## Usage

### Rust CLI

A basic command line interface is included such that you can use this version of Pywr without Python.
This CLI is in the `pywr-cli` crate.

To see the CLI commands available run the following:

```bash
cargo run -p pywr-cli -- --help
```

To run a Pywr v2 model use the following:

```bash
cargo run -p pywr-cli -- run tests/models/simple1.json
```

### Python CLI

If the Python extension has been compiled using the above instructions a model can be run using the basic Python
CLI.

```bash
python -m pywr run tests/models/simple1.json
```

## Porting a Pywr v1.x model to v2.x

This version of Pywr is not backward compatible with Pywr v1.x. One of the major reasons for this version is the
lack of a strong schema in the Pywr v1.x JSON files. Pywr v2.x uses an updated JSON schema that is defined in this
repository. Therefore, v1.x JSON files must be converted to the v2.x JSON schema. This conversion can be undertaken
manually, but there is also a work-in-progress conversion tool. The conversion tool uses a v1.x schema defined in
the [pywr-schema](https://github.com/pywr/pywr-schema) project.

**Please note that conversion from Pywr v1.x to v2.x is experimental and not all features of Pywr are implemented
in `pywr-schema` or have been implemented in Pywr v2.x yet. Due to the changes between these versions it is very
likely an automatic conversion will not completely convert your model, and it _WILL_ require manual testing and
checking.**

```bash
cargo run --no-default-features -- convert /path/to/my/v1.x/model.json
```

Feedback on porting models is very welcome, so please open an issue with any questions or problems.


<!-- _For more examples, please refer to the [Documentation](https://example.com)_ -->

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CRATES -->

## Crates

This repository contains the following crates:

### Pywr-core

A low-level Rust library for constructing network models. This crate interfaces with linear program solvers.

Feature flags:

| Feature    | Description                                      | Default |
|------------|--------------------------------------------------|---------|
| `pyo3`     | Enable the Python bindings.                      | True    |
| `highs`    | Enable the HiGHS LP solver.                      | False   |
| `ipm-ocl`  | Enable the OpenCL IPM solver (requires nightly). | False   |
| `ipm-simd` | Enable the AVX IPM solver (requires nightly).    | False   |
| `cbc`      | Enable the CBC MILP solver.                      | False   |

### Pywr-schema

A Rust library for validating Pywr JSON files against a schema, and then building a model from the schema
using `pywr-core`.

Feature flags:

| Feature    | Description                                                                                                                                                                                                                                          | Default |
|------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|
| `core`     | Enable building models from the schema with `pywr-core`. This feature is enabled by default, but requires a lot of dependencies. If you only require schema validation and manipulation consider building this crate with `default-features = false` | True    |
| `pyo3`     | Enable the Python bindings.                                                                                                                                                                                                                          | True    |
| `highs`    | Enable the HiGHS LP solver.                                                                                                                                                                                                                          | False   |
| `ipm-ocl`  | Enable the OpenCL IPM solver (requires nightly).                                                                                                                                                                                                     | False   |
| `ipm-simd` | Enable the AVX IPM solver (requires nightly).                                                                                                                                                                                                        | False   |
| `cbc`      | Enable the CBC MILP solver.                                                                                                                                                                                                                          | False   |

### Pywr-cli

A command line interface for running Pywr models.

### Pywr-python

A Python extension (and package) for constructing and running Pywr models.


<!-- ROADMAP -->

## Roadmap

- [x] Proof-of-concept - demonstrate the benefits of the RIIR approach.
- [ ] Redesign of outputs & metrics.
- [ ] Redesign of variable API for integration with external optimisation algorithms.
- [ ] Implement outstanding `Parameters` from Pywr v1.x
- [ ] Design & implement Python API using Rust extension.
- [ ] Release Pywr v2.x beta

See the [open issues](https://github.com/pywr/pywr-next/issues) for a full list of proposed features (and known issues).

<p align="right">(<a href="#readme-top">back to top</a>)</p>



<!-- CONTRIBUTING -->

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any
contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also
simply open an issue with the tag "enhancement".
Don't forget to give the project a star! Thanks again!

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>


<!-- LICENSE -->

## License

Distributed under the Apache 2.0 or MIT License. See `LICENSE.txt` for more information.

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

[license-url]: https://github.com/pywr/pywr-next/blob/main/LICENSE

[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=for-the-badge&logo=linkedin&colorB=555

[linkedin-url]: https://linkedin.com/in/james-tomlinson-a465352b

[Rust]: https://img.shields.io/badge/rust-ef4a23?style=for-the-badge&logo=rust&logoColor=white

[Rust-url]: https://www.rust-lang.org/

[Python]: https://img.shields.io/badge/python-275277?style=for-the-badge&logo=python&logoColor=white

[Python-url]: https://www.python.org/

Copyright (C) 2020-2023 James Tomlinson Associates Ltd.
