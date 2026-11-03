# Network definitions

Model definitions often share the same selections. Several models might all use the historical inflows, or the
same group of options. A **network definition** gives a selection of files a name, so that models can refer to
it instead of repeating it.

```json
"networks": [
  {
    "name": "historical",
    "description": "Historical catchment inflows.",
    "include": [{ "set": "inflows", "files": ["historical.json"] }]
  },
  {
    "name": "option1",
    "include": [{ "set": "options", "files": ["option1.json"] }]
  },
  {
    "name": "option2",
    "include": [{ "set": "options", "files": ["option2.json"] }]
  },
  {
    "name": "option12",
    "description": "Both supply options.",
    "extends": ["option1", "option2"]
  }
]
```

| Field         | Description                                                                         |
|---------------|-------------------------------------------------------------------------------------|
| `name`        | The network definition's name.                                                      |
| `description` | An optional description.                                                            |
| `base`        | A [sub-network](./sub-networks.md) whose base network is the starting point.        |
| `extends`     | Other network definitions whose files come next, in order.                          |
| `include`     | Selections of files from network sets, as in a model definition.                    |

A model definition lists the network definitions it uses in `networks`:

```json
"models": [
  { "name": "baseline-historical", "networks": ["historical"] },
  { "name": "baseline-option12", "networks": ["historical", "option12"] }
]
```

## Extending network definitions

A network definition can build on others with `extends`. Its files are, in order:

1. the base network of its `base` sub-network, if it has one;
2. the files of each network definition in `extends`, in order;
3. its own `include` selections.

Extension can be nested to any depth. A file reached by several routes, for example when two extended
definitions share a parent, is merged once. A network definition cannot extend itself, directly or through
others; such a cycle is reported with the chain of definitions that forms it.

## Limits apply to the whole model

A network set's `min_files` and `max_files` are checked against the complete model, not against each network
definition. A set of inflows with `max_files: 1` lets each of two network definitions select an inflow, but not
a model that uses both of them.

## Composing a network on its own

A network definition is not a model, since it has no time domain, and cannot be run. It can be composed on its
own from Rust with `Project::compose_network`, which merges its files into an empty network, for example to
inspect or reuse it. Unlike a model, a network composed this way may still contain placeholders.
