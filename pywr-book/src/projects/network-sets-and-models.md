# Network sets and models

The simplest project has a base model, one or more network sets, and model definitions that choose files from
those sets.

## Base model

`base_model` is the path, relative to the project directory, of an ordinary Pywr model file. It provides the
time domain, scenarios and metadata of every model in the project, and its network is the starting point that
fragments are merged into.

```json
{
  "format": "v1",
  "base_model": "models/base.json"
}
```

A model definition can give its own `base_model` instead, for example to use a different time domain. A
project without a top-level `base_model` must give one in every model definition.

## Network sets

A network set is a named directory of network fragments. Every `.json` file in the directory is a member of
the set, and is referred to by its filename.

```json
"network_sets": [
  { "name": "options" },
  { "name": "inflows", "dir": "hydrology/inflows", "min_files": 1, "max_files": 1 }
]
```

| Field       | Description                                                                      |
|-------------|----------------------------------------------------------------------------------|
| `name`      | The set's name, used by definitions to select from it.                           |
| `dir`       | The directory, relative to the project directory. Defaults to the set's name.    |
| `min_files` | The fewest files a model must select from the set. Also written `min`.           |
| `max_files` | The most files a model may select from the set. Also written `max`.              |

`min_files` and `max_files` describe how a set is meant to be used. A set of alternative inflows, where every
model needs exactly one, has both set to 1. A set of optional schemes, where any combination is allowed, has
neither. The limits apply to everything a model selects from the set, by whatever route, including through
[network definitions](./network-definitions.md).

## Model definitions

A model definition names a runnable model and says what it is made of.

```json
"models": [
  {
    "name": "baseline",
    "include": [
      { "set": "inflows", "files": ["historical.json"] }
    ]
  },
  {
    "name": "with-options",
    "include": [
      { "set": "inflows", "files": ["historical.json"] },
      { "set": "options", "include_all": true }
    ],
    "overrides": {
      "time": { "start": "2030-01-01", "end": "2059-12-31", "timestep": { "type": "Days", "days": 1 } }
    }
  }
]
```

| Field          | Description                                                                                 |
|----------------|---------------------------------------------------------------------------------------------|
| `name`         | The model's name. It is also used as the composed model's title.                            |
| `base_model`   | The model's own base model, instead of the project's.                                       |
| `include`      | Selections of files from network sets, merged in order.                                     |
| `networks`     | [Network definitions](./network-definitions.md) to merge before `include`.                  |
| `sub_networks` | [Sub-networks](./sub-networks.md) to activate.                                              |
| `overrides`    | Replacements for the base model's `time` and `scenarios`.                                   |

### Selecting files

Each entry in `include` selects files from one network set:

- `files` lists filenames in the set, which are merged in the order given;
- `include_all: true` selects every file in the set, in filename order, and takes precedence over `files`.

A definition may select from each set at most once. Selecting a file that is not in the set, or the same file
twice, is an error.

### Positioning fragments

A fragment drawn for one place in a schematic may need moving when it is merged into another model.
`file_meta` gives metadata for individual selected files, keyed by filename. Its `position_offset` moves the
file's nodes by a schematic offset, a geographic offset, or both:

```json
{
  "set": "options",
  "files": ["option1.json"],
  "file_meta": {
    "option1.json": { "position_offset": { "schematic": [200.0, -50.0] } }
  }
}
```

Metadata for a file that is not selected is an error, since it is most likely a typing mistake.

## Merge order

The files of a model are merged in this order:

1. the base model's network;
2. the base networks of any [sub-networks](./sub-networks.md) the model activates;
3. the files of each [network definition](./network-definitions.md) in `networks`, in order;
4. the files selected in `include`, in order.

A file reached by more than one route is merged once, at its first position. The order matters: it decides the
order of new nodes in the composed network, and which file is recorded as the origin of a component that
several files contribute to.
