# Penalty costs

## Virtual storage costs

A `VirtualStorage` node can add a cost to the nodes it draws on. This is useful for a licence that
should become more expensive to use as it runs low: give it a `cost` that depends on its own
volume, such as a control curve parameter. The cost only applies when the virtual storage also
has a `cost_aggregation`, which sets how each of its nodes combines the cost with its own:

- `Sum`: the costs are added together.
- `Max`: the largest cost is used.
- `Min`: the smallest cost is used.

```json
{
  "meta": { "name": "licence" },
  "type": "VirtualStorage",
  "nodes": [{ "name": "abstraction" }],
  "max_volume": { "type": "Literal", "value": 1000.0 },
  "initial_volume": { "type": "Proportional", "proportion": 1.0 },
  "cost": { "type": "Parameter", "name": "licence-cost" },
  "cost_aggregation": "Sum"
}
```

For each node:

- each virtual storage's cost is multiplied by the node's factor in that virtual storage;
- a node without a cost of its own combines only the virtual storage costs;
- a virtual storage outside its active period (a `Seasonal` reset) adds no cost;
- if nothing is left to combine, the node's cost is zero.

A node has one aggregation for all the virtual storage nodes it belongs to. If two virtual storage
nodes give the same node different aggregations, the model is invalid.

Models converted from pywr v1 keep each virtual storage `cost` but have no `cost_aggregation`,
because pywr v1 never applied these costs. Add a `cost_aggregation` to make them apply.
