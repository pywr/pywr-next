# Example multi-file project

This example builds several variants of one small model from a shared base model and
interchangeable network fragments, using a `pywr-project.json` manifest. It also depends on a
second project, `../neighbour`, which holds a neighbouring supply system.

## Layout

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
    └── demand.csv         # the neighbour's demand, read by `supply/base.json`
```

The base model has a `Placeholder` node, `supply1`, for the catchment inflow. Each file in
`catchment/inflows/` defines `supply1` as a real node, which replaces the placeholder when the
files are merged. The option files work the same way in reverse: each declares a placeholder
for the base model node it connects to (`demand1` or `link1`) and adds a new supply node.

## Manifest

The manifest has six parts:

- **`dependencies`** name other projects whose definitions this one may use, by a path to their
  manifest. This project depends on `../neighbour/pywr-project.json` under the name
  `neighbour`, so it refers to the network definition `neighbour` there as
  `neighbour::neighbour`. A project's own `exports` list what others may use; the neighbour
  project exports only that network definition.
- **`sub_networks`** declare directories that hold part of the model, each with an optional base
  network and its own network sets. The `catchment` sub-network has one set, `inflows`, referred
  to as `catchment::inflows`. It sets `min` and `max` to 1, so every model that uses the
  catchment must select exactly one inflow.
- **`network_sets`** name the project's own directories of fragments.
- **`networks`** are reusable network definitions. Each selects files from the network sets.
  `historical` and `stochastic` name `catchment` as their `base`, which activates the
  sub-network; a model cannot select from a sub-network's sets unless it is active. `option12`
  extends `option1` and `option2` to combine them.
- **`models`** are the runnable model definitions. Each starts from `base_model` and merges the
  networks it lists, in order.

| Model                 | Networks                 | Description                    |
|-----------------------|--------------------------|--------------------------------|
| `baseline-historical` | `historical`             | Historical inflows, no options |
| `baseline-stochastic` | `stochastic`             | Stochastic inflows, no options |
| `baseline-option1`    | `historical`, `option1`  | Adds supply option 1           |
| `baseline-option2`    | `historical`, `option2`  | Adds supply option 2           |
| `baseline-option12`   | `historical`, `option12` | Adds both supply options       |

- **`multi_models`** are runnable multi-network models, whose networks run together and pass
  values to each other through transfers. `joint-historical` takes its time domain from the
  `baseline-historical` model and has two networks:
    - `company`, the network of the `baseline-historical` model;
    - `neighbour`, the `neighbour::neighbour` network definition from the neighbour project. Its
      `import` node's maximum flow is the transfer `company-link1-flow`, which carries the flow
      through `link1` in `company`.

  Files from the neighbour project read their data relative to that project, so
  `data/demand.csv` in `supply/base.json` is read from `../neighbour/data/demand.csv`.

## Running

From the repository root:

```sh
cargo run -p pywr-cli -- run-project examples/my-model/pywr-project.json \
    --definition baseline-option1 --output-path /tmp/my-model
```

`--definition` takes the name of a model or a multi-model.

The base model writes its results to `outputs.h5` in the output path.


