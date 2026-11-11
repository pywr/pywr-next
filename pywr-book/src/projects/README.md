# Projects

A Pywr model is usually a single JSON file. As models grow, and as they are used for more studies, a single
file becomes hard to manage. Several variants of one model (different inflows, different options, different
future scenarios) end up as near-identical copies of the same file, and every change has to be repeated in
each copy.

A Pywr **project** solves this by building models from parts. A project is a directory containing a manifest,
`pywr-project.json`, and the JSON files it refers to. The manifest describes how to combine those files into
complete models. Each part is written once, and each model says which parts it uses.

Projects are handled by the `pywr-project` crate, and are run with the `run-project` command of the Pywr CLI.

## How a model is composed

A model in a project is built from:

- a **base model**: an ordinary Pywr model file with the time domain, scenarios, metadata and a starting
  network;
- **network fragments**: files that each contain part of a network, in the same format as the `network`
  section of a model file;
- **overrides**: optional changes to the base model's time domain or scenarios.

The fragments are merged into the base model's network in a defined order. Each merge adds the fragment's
nodes, edges, parameters, tables, time series, metric sets and outputs to the network. Names must be unique
across everything that is merged, with one exception: **placeholders**.

### Placeholders

A placeholder is a component of type `Placeholder` that stands in for a component defined in another file.
When a real component with the same name is merged, it replaces the placeholder, keeping its position in the
list of nodes. A placeholder merged on top of a real component is ignored.

Placeholders let a file refer to something it does not define. In the example below, the base model has a
placeholder node `supply1` that is connected to the rest of the network. Each inflow fragment defines
`supply1` as a real node with its own inflow, so whichever fragment is chosen fills the gap.

[//]: # (@formatter:off)

```json
{{#include ../../../examples/my-model/catchment/inflows/historical.json}}
```
[//]: # (@formatter:on)

Fragments can use placeholders in the same way. A fragment adding a new supply option declares a placeholder
for the existing node it connects to, so that it can add an edge to it:

[//]: # (@formatter:off)

```json
{{#include ../../../examples/my-model/options/option1.json}}
```
[//]: # (@formatter:on)

Placeholders can be used for nodes, virtual nodes, parameters, tables, time series and outputs. A model that
still contains a placeholder after every file has been merged is incomplete, and composing it fails with a
list of the placeholders left and the files they came from.

## Parts of a manifest

A manifest has the following sections. Only `format` is required; the others are covered in the following
pages.

| Section        | Purpose                                                                       |
|----------------|-------------------------------------------------------------------------------|
| `format`       | The manifest format version. Currently always `"v1"`.                         |
| `base_model`   | The base model used by model definitions that do not give their own.          |
| `network_sets` | Named directories of network fragments.                                       |
| `networks`     | Reusable network definitions, each a named selection of fragments.            |
| `models`       | Model definitions: the runnable models of the project.                        |
| `sub_networks` | Directories holding part of a wider model, each with its own network sets.    |
| `multi_models` | Multi-network models, whose networks run together and exchange values.        |
| `dependencies` | Other projects whose definitions this project uses.                           |
| `exports`      | What other projects may use from this one.                                    |

- [Network sets and models](./network-sets-and-models.md) covers the basics: `base_model`, `network_sets`
  and `models`.
- [Network definitions](./network-definitions.md) covers reusable `networks`.
- [Sub-networks](./sub-networks.md) covers organising a project into `sub_networks`.
- [Multi-models](./multi-models.md) covers `multi_models`.
- [Dependencies](./dependencies.md) covers using other projects with `dependencies` and `exports`.

## An example project

The repository contains an example project in `examples/my-model`. It builds variants of a small model from a
base model, a choice of catchment inflows and optional supply schemes. It also depends on a second project,
`examples/neighbour`, which holds a neighbouring supply system.

```text
my-model/
├── pywr-project.json      # the project manifest
├── models/
│   └── base.json          # base model: time domain, demand, outputs
├── catchment/             # sub-network
│   └── inflows/           # network set `catchment::inflows`: one inflow per model
│       ├── historical.json
│       └── stochastic.json
└── options/               # network set: optional supply schemes
    ├── option1.json
    └── option2.json

neighbour/                 # a separate project that my-model depends on
├── pywr-project.json
├── supply/                # network set: the neighbour's network
│   └── base.json
└── data/
    └── demand.csv
```

The pages of this chapter use parts of this example. Its full manifest is:

<details>
<summary>examples/my-model/pywr-project.json</summary>

[//]: # (@formatter:off)

```json
{{#include ../../../examples/my-model/pywr-project.json}}
```
[//]: # (@formatter:on)

</details>

## Running a project

The `run-project` command builds and runs one model or multi-model definition from a project:

```sh
pywr-cli run-project examples/my-model/pywr-project.json --definition baseline-option1 --output-path results
```

`--definition` names the model or multi-model to run. Model and multi-model names share one namespace, so the
name alone says which to run. The other options are the same as for `run`: `--solver`, `--threads`,
`--data-path` and `--output-path`.

Relative data paths in the composed model are resolved against `--data-path`, which defaults to the directory
of the manifest. Paths in a project's own files are therefore written relative to the project directory. Files
from [sub-networks](./sub-networks.md) and [dependencies](./dependencies.md) can have their data paths
rewritten when they are merged, as described on those pages.

## Validation

Composing a model checks the manifest as it goes, and stops at the first problem. A project can also be
validated as a whole, which reports every problem found:

- duplicate or invalid names;
- references to sets, files, network definitions, sub-networks, models or dependencies that do not exist;
- selections that break a network set's `min_files` or `max_files` limits;
- paths that are not plain relative paths, or that lead outside the project directory, including through
  symbolic links;
- cycles between network definitions or between projects.

Problems that can only be found once the files are read, such as duplicate component names, unresolved
placeholders, or a transfer reading a node that does not exist, are reported when the composed model is
loaded.

## Provenance

Every component of a composed model records where it came from, in its `meta.provenance`. The `origin` is the
file that defined the component, given relative to its project, together with the network set, sub-network,
network definition and dependency it was included through, where those apply. If a component was a
placeholder that was later replaced, or a metric set was combined from several files, the other files are
listed as `contributors`.

Provenance makes it possible to trace any part of a composed model back to the file to edit. It is written out
with the model, so it survives saving a composed model to JSON.

## Using projects from Rust

Projects can be composed from Rust with the `pywr-project` crate:

```rust,ignore
use pywr_project::{Composed, Project};
use pywr_schema::NetworkMergeOptions;

let project = Project::open("examples/my-model/pywr-project.json")?;
project.validate()?;

match project.compose("baseline-option1")? {
    Composed::Model(composed) => {
        let schema = composed.load()?.into_model_schema(&NetworkMergeOptions::default())?;
        // `schema` is an ordinary `ModelSchema`.
    }
    Composed::MultiModel(composed) => {
        let schema = composed.load()?.into_multi_model_schema(&NetworkMergeOptions::default())?;
        // `schema` is an ordinary `MultiNetworkModelSchema`.
    }
}
```

Composition happens in two steps. `compose` resolves the manifest into the list of files to merge, without
reading them, and `all_paths` lists those files, which is useful for detecting changes. `load` then reads the
files, and `into_model_schema` or `into_multi_model_schema` merges them.
