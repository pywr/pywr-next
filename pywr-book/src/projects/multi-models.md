# Multi-models

A multi-model runs several networks together over one time domain, passing values
between them through **transfers**. Each network is solved separately, which suits systems managed by
different organisations, or a large system split into parts. A project can compose multi-models from its
definitions with `multi_models`.

```json
"multi_models": [
  {
    "name": "joint-historical",
    "domain": { "type": "Model", "name": "baseline-historical" },
    "networks": [
      {
        "name": "company",
        "network": { "type": "Model", "name": "baseline-historical" }
      },
      {
        "name": "neighbour",
        "network": { "type": "Network", "name": "neighbour::neighbour" },
        "transfers": [
          {
            "type": "Float",
            "from_network": "company",
            "metric": { "type": "Node", "name": "link1" },
            "name": "company-link1-flow",
            "initial_value": 0.0
          }
        ]
      }
    ]
  }
]
```

| Field       | Description                                                                      |
|-------------|----------------------------------------------------------------------------------|
| `name`      | The multi-model's name. Models and multi-models share one namespace.             |
| `domain`    | Where the time domain, scenarios and metadata come from.                         |
| `overrides` | Replacements for the domain's `time` and `scenarios`.                            |
| `networks`  | The networks of the multi-model, each with the transfers into it.                |

## Domain

The `domain` sets the time domain, scenarios and metadata shared by every network. Its `type` is one of:

- `{ "type": "Model", "name": "<model>" }` uses the base model of a model definition in this project, with that
  model's overrides applied;
- `{ "type": "BaseModel", "path": "<path>" }` uses a model file, relative to the project directory, ignoring its
  network;
- `{ "type": "Inline", "time": ..., "scenarios": ... }` gives the time domain and, optionally, the scenarios
  inline.

The multi-model's own `overrides` are applied after those of the domain.

## Networks

Each entry in `networks` is one network of the multi-model:

| Field            | Description                                                                         |
|------------------|-------------------------------------------------------------------------------------|
| `name`           | The network's name, unique within the multi-model. Transfers use it.                |
| `network`        | What the network starts from (see below). Optional.                                 |
| `sub_networks`   | [Sub-networks](./sub-networks.md) to activate. Their base networks come first.      |
| `include`        | Further selections of files from network sets, merged after the starting point.     |
| `transfers`      | Transfers into this network from the others.                                        |
| `transfers_file` | A JSON file, relative to the project directory, with a list of further transfers.   |

A network can start from:

- `{ "type": "Network", "name": "<network definition>" }`, which uses that network definition's files, merged
  into an empty network; or
- `{ "type": "Model", "name": "<model definition>" }`, which uses the network part of a model definition: its
  base model's network and everything merged into it. The model's time domain, scenarios and overrides are not
  used.

Without `network`, a network is made only from its `include` selections.

Each network is checked like a model: the sub-networks it selects from must be active, and no placeholders
may remain. The limits of network sets apply a little differently, because each network is only one part of
the multi-model. A network is checked against the `min_files` and `max_files` limits of the project's network
sets only if it selects from them, so a set that every model must select from does not also bind an unrelated
network. The sets of the sub-networks a network activates are checked in full, so files that must always come
together belong in a sub-network. A network that uses a model definition as it is, without `sub_networks` or
`include`, is checked once, as that model.

## Transfers

A transfer makes a value from one network available in another. It reads a `metric`, such as the flow
through a node or the value of a parameter, from the network `from_network`, and gives it a `name`. The
receiving network uses the value through a metric of type `InterNetworkTransfer`:

[//]: # (@formatter:off)

```json
{{#include ../../../examples/neighbour/supply/base.json}}
```
[//]: # (@formatter:on)

Every transfer has a `type` of `Float` or `Index`, which says what kind of value it carries:

| `type`  | `metric`                                                  | `initial_value`        | Read by                                                       |
|---------|-----------------------------------------------------------|------------------------|---------------------------------------------------------------|
| `Float` | A float metric, such as a node's flow or a parameter.     | A number.              | A float metric of type `InterNetworkTransfer`.                |
| `Index` | An index metric, such as an index parameter or constant.  | A non-negative integer. | A float or an index metric of type `InterNetworkTransfer`.   |

An index transfer can be read as a float, just as an index parameter can, but a float transfer cannot be read
as an index: reading one through an index metric is an error. To pass a control state, such as a drought
level, from one network to another, give the transfer `"type": "Index"` and read an index parameter:

```json
{
  "type": "Index",
  "from_network": "company",
  "metric": { "type": "Parameter", "name": "drought-level" },
  "name": "company-drought-level",
  "initial_value": 0
}
```

`initial_value` is the value used in the first time step, before the source network has been solved. Each
transfer must come from another network of the same multi-model, and transfers into one network must have
unique names. Once the networks are composed, each transfer is also checked to read a node, virtual node or
parameter that exists in its source network, and each `InterNetworkTransfer` a network reads must be
declared as a transfer into it. A model that is not part of a multi-model may not read any. Two networks of
a multi-model may not write the same output file, which is easy to do by accident when they are built from
models that share a base model.

Transfers can be listed inline in `transfers`, kept in a separate file named by `transfers_file`, or both.
A transfers file contains a JSON list in the same format as `transfers`, which is useful when there are many.
