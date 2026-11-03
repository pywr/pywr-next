# Multi-models

A multi-model runs several networks together over one time domain, passing values
between them through **transfers**. Each network is solved separately, which suits systems managed by
different organisations, or a large system split into parts. A project can compose multi-models from its
definitions with `multi_models`.

```json
"multi_models": [
  {
    "name": "joint-historical",
    "domain": { "model": "baseline-historical" },
    "networks": [
      {
        "name": "company",
        "network": { "model": "baseline-historical" }
      },
      {
        "name": "neighbour",
        "network": { "network": "neighbour::neighbour" },
        "transfers": [
          {
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

The `domain` sets the time domain, scenarios and metadata shared by every network. It takes one of three
forms:

- `{ "model": "<model>" }` uses the base model of a model definition in this project, with that model's
  overrides applied;
- `{ "base_model": "<path>" }` uses a model file, relative to the project directory, ignoring its network;
- `{ "time": ..., "scenarios": ... }` gives the time domain and, optionally, the scenarios inline.

The multi-model's own `overrides` are applied after those of the domain.

## Networks

Each entry in `networks` is one network of the multi-model:

| Field            | Description                                                                         |
|------------------|-------------------------------------------------------------------------------------|
| `name`           | The network's name, unique within the multi-model. Transfers use it.                |
| `network`        | What the network starts from (see below). Optional.                                 |
| `include`        | Further selections of files from network sets, merged after the starting point.     |
| `transfers`      | Transfers into this network from the others.                                        |
| `transfers_file` | A JSON file, relative to the project directory, with a list of further transfers.   |

A network can start from:

- `{ "network": "<network definition>" }`, which uses that network definition's files, merged into an empty
  network; or
- `{ "model": "<model definition>" }`, which uses the network part of a model definition: its base model's
  network and everything merged into it. The model's time domain, scenarios and overrides are not used.

Without `network`, a network is made only from its `include` selections. The rules for a model apply to each
network separately: the `min_files` and `max_files` limits of network sets, the activation of sub-networks,
and the requirement that no placeholders remain.

## Transfers

A transfer makes a value from one network available in another. It reads a `metric`, such as the flow
through a node or the value of a parameter, from the network `from_network`, and gives it a `name`. The
receiving network uses the value through a metric of type `InterNetworkTransfer`:

[//]: # (@formatter:off)

```json
{{#include ../../../examples/neighbour/supply/base.json}}
```
[//]: # (@formatter:on)

`initial_value` is the value used in the first time step, before the source network has been solved. Each
transfer must come from another network of the same multi-model, and transfers into one network must have
unique names. Once the networks are composed, each transfer is also checked to read a node, virtual node or
parameter that exists in its source network.

Transfers can be listed inline in `transfers`, kept in a separate file named by `transfers_file`, or both.
A transfers file contains a JSON list in the same format as `transfers`, which is useful when there are many.
