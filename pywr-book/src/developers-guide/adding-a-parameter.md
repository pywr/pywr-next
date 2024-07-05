# Adding a new parameter to Pywr.

This guide explains how to add a new parameter to Pywr.

## When to add a new parameter?

New parameters can be added to complement the existing parameters in Pywr.
These parameters should be generic and reusable across a wide range of models.
By adding them to Pywr itself other users are able to use them in their models without having to implement them
themselves.
They are also typically implemented in Rust, which means they are fast and efficient.

If the parameter is specific to a particular model or data set, it is better to implement it in the model itself
using a custom parameter.
Custom parameters can be added using, for example, the `PythonParameter`.

## Adding a new parameter

To add new parameter to Pywr you need to do two things:

- Add the implementation to the `pywr-core` crate, and
- Add the schema definition to the `pywr-schema` crate.

### Adding the implementation to `pywr-core`

The implementation of the parameter should be added to the `pywr-core` crate.
This is typically done by adding a new module to the `parameters` module in the `src` directory.
It is a good idea to follow the existing structure of the `parameters` module by making a new module for the new
parameter.
Developers can follow the existing parameters as examples.

In this example, we will add a new parameter called `MaxParameter` that calculates the maximum value of a metric.
Parameters can depend on other parameters or values from the model via the `MetricF64` type.
In this case the `metric` field stores a `MetricF64` that will be compared with the `threshold` field
to calculate the maximum value.
The threshold is a constant value that is set when the parameter is created.
Finally, the `meta` field stores the metadata for the parameter.
The `ParameterMeta` struct is used to store the metadata for all parameters and can be reused.

```rust,ignore
{{#rustdoc_include ../../listings/adding-a-parameter/src/main.rs:parameter}}
```

To allow the parameter to be used in the model it is helpful to add a `new` function that creates a new instance of the
parameter. This will be used by the schema to create the parameter when it is loaded from a model file.

```rust,ignore
{{#rustdoc_include ../../listings/adding-a-parameter/src/main.rs:impl-new}}
```

Finally, the minimum implementation of the `Parameter` trait should be added for `MaxParameter`.
This trait requires the `meta` function to return the metadata for the parameter, and the `compute` function to
calculate the value of the parameter at a given timestep and scenario.
In this case the `compute` function calculates the maximum value of the metric and the threshold.
The value of the metric is obtained from the model using the `get_value` function.

```rust,ignore
{{#rustdoc_include ../../listings/adding-a-parameter/src/main.rs:impl-parameter}}
```

### Adding the schema definition to `pywr-schema`

The schema definition for the new parameter should be added to the `pywr-schema` crate.
Again, it is a good idea to follow the existing structure of the schema by making a new module for the new parameter.
Developers can also follow the existing parameters as examples.
As with the `pywr-core` implementation, the `meta` field is used to store the metadata for the parameter and can
use the `ParameterMeta` struct (NB this is from `pywr-schema` crate).
The rest of the struct looks very similar to the `pywr-core` implementation, but uses `pywr-schema`
types for the fields.
The struct should also derive `serde::Deserialize`, `serde::Serialize`, `Debug`, `Clone`, `JsonSchema`,
and `PywrVisitAll` to be compatible with the rest of Pywr.

> Note: The `PywrVisitAll` derive is not shown in the listing as it can not currently be used outside
> the `pywr-schema` crate.

```rust,ignore
{{#rustdoc_include ../../listings/adding-a-parameter/src/main.rs:schema}}

```

Next, the parameter needs a method to add itself to a network.
This is typically done by implementing a `add_to_model` method for the parameter.
This method should be feature-gated with the `core` feature to ensure it is only available when the `core` feature is
enabled.
The method should take a mutable reference to the network and a reference to the `LoadArgs` struct.
The method should load the metric from the model using the `load` method, and then create a new `MaxParameter` using
the `new` method implemented above.
Finally, the method should add the parameter to the network using the `add_parameter` method.

```rust,ignore
{{#rustdoc_include ../../listings/adding-a-parameter/src/main.rs:schema-impl}}
```

Finally, the schema definition should be added to the `Parameter` enum in the `parameters` module.
This will require ensuring the new variant is added to all places where that enum is used.
The borrow checker can be helpful in ensuring all places are updated.
