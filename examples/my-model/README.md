# Example multi-file project

This example builds several variants of one small model from a shared base model and
interchangeable network fragments, using a `pywr-project.json` manifest.

## Layout

```text
my-model/
├── pywr-project.json      # the project manifest
├── models/
│   └── base.json          # base model: time domain, demand, outputs
├── catchments/            # network set: one catchment inflow per model
│   ├── historical.json
│   └── stochastic.json
└── options/               # network set: optional supply schemes
    ├── option1.json
    └── option2.json
```

The base model has a `Placeholder` node, `supply1`, for the catchment inflow. Each file in
`catchments/` defines `supply1` as a real node, which replaces the placeholder when the files
are merged. The option files work the same way in reverse: each declares a placeholder for
the base model node it connects to (`demand1` or `link1`) and adds a new supply node.

## Manifest

The manifest has two parts:

- **`network_sets`** name the directories of fragments. `catchments` sets `min` and `max` to 1,
  so every model must select exactly one catchment.
- **`definitions`** are the runnable model definitions. Each starts from `base_model` and merges
  the files it selects from each network set, in order.

| Definition            | Catchment         | Options                        | Description                    |
|-----------------------|-------------------|--------------------------------|--------------------------------|
| `baseline-historical` | `historical.json` |                                | Historical inflows, no options |
| `baseline-stochastic` | `stochastic.json` |                                | Stochastic inflows, no options |
| `baseline-option1`    | `historical.json` | `option1.json`                 | Adds supply option 1           |
| `baseline-option2`    | `historical.json` | `option2.json`                 | Adds supply option 2           |
| `baseline-option12`   | `historical.json` | `option1.json`, `option2.json` | Adds both supply options       |

## Running

From the repository root:

```sh
cargo run -p pywr-cli -- run-project examples/my-model/pywr-project.json \
    --definition baseline-option1 --output-path /tmp/my-model
```

The base model writes its results to `outputs.h5` in the output path.


