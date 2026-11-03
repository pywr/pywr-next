# Sub-networks

A large model often covers several distinct areas, such as catchments or supply zones, each with its own
fragments and alternatives. A **sub-network** gives each area its own directory, with its own base network and
network sets, inside one project.

```json
"sub_networks": [
  {
    "name": "catchment",
    "network_sets": [{ "name": "inflows", "min": 1, "max": 1 }]
  },
  {
    "name": "upper",
    "dir": "areas/upper",
    "base_network": "base.json",
    "data_dir": "data",
    "network_sets": [{ "name": "options" }]
  }
]
```

| Field          | Description                                                                                    |
|----------------|------------------------------------------------------------------------------------------------|
| `name`         | The sub-network's name.                                                                        |
| `dir`          | Its directory, relative to the project directory. Defaults to the sub-network's name.          |
| `base_network` | A network fragment, relative to the sub-network directory, that the sub-network starts from.   |
| `data_dir`     | The directory, relative to the sub-network directory, that its files' data paths are relative to. Defaults to the sub-network directory. |
| `network_sets` | The sub-network's network sets, with directories relative to the sub-network directory.       |

## Qualified names

A sub-network's network sets are referred to by qualified names, made of the sub-network's name and the set's
name separated by `::`. The `inflows` set of the `catchment` sub-network is `catchment::inflows`:

```json
{ "set": "catchment::inflows", "files": ["historical.json"] }
```

Because `::` separates the parts of a qualified name, no name in a manifest may contain it.

## Activating sub-networks

A sub-network is **inactive** unless a model uses it, and a model can only select files from the sets of an
active sub-network. This stops a model from accidentally including part of an area without the area's base
network. A sub-network is activated by:

- listing it in a model definition's `sub_networks`; or
- naming it as the `base` of a network definition the model uses.

```json
"networks": [
  {
    "name": "historical",
    "base": "catchment",
    "include": [{ "set": "catchment::inflows", "files": ["historical.json"] }]
  }
],
"models": [
  { "name": "baseline-historical", "networks": ["historical"] },
  { "name": "upper-only", "sub_networks": ["upper"] }
]
```

A sub-network's base network is merged once, wherever it is first reached. The base networks of the
sub-networks a model lists come straight after the base model, in the order the sub-networks are declared. A
sub-network used as a network definition's `base` starts that definition's files. The `min_files` and `max_files`
limits of a sub-network's sets only apply to models in which it is active.

## Data paths

A sub-network's files give data paths, such as the path of a table or time series, relative to the
sub-network's data directory rather than the project directory. This keeps a sub-network self-contained: it
can be moved, or copied to another project, without editing its files.

When a sub-network's files are merged, each relative data path in them is rewritten to be relative to the
project directory by prefixing the sub-network's data directory. For the `upper` sub-network above, whose data
directory is `areas/upper/data`, a time series path of `flows.csv` becomes `areas/upper/data/flows.csv`.

Absolute paths are left unchanged. Output file names are not rewritten either, because they are relative to
the output directory, not the data directory.
