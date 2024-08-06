# Parameter traits and return types

The `pywr-core` crate defines a number of traits that are used to implement parameters. These traits are used to define
the behaviour of the parameter and how it interacts with the model. Each parameter must implement the `Parameter` trait
and one of the three compute traits: `GeneralParameter<T>`, `SimpleParameter<T>`, or `ConstParameter<T>`.

## The `Parameter` trait

The `Parameter` trait is the base trait for all parameters in Pywr. It defines the basic behaviour of the parameter and
how it interacts with the model. The minimum implementation requires returning the metadata for the parameter.
Additional methods can be implemented to provide additional functionality. Please refer to the documentation for
the `Parameter` trait for more information.

## The `GeneralParameter<T>` trait

The `GeneralParameter<T>` trait is used for parameters that depend on `MetricF64` values from the model. Because
`MetricF64` values can refer to other parameters, general model state or other information implementing this
traits provides the most flexibility for a parameter. The `compute` method is used to calculate the value of the
parameter at a given timestep and scenario. This method is resolved in order with other model components such
as nodes.

## The `SimpleParameter<T>` trait

The `SimpleParameter<T>` trait is used for parameters that depend on `SimpleMetricF64` or `ConstantMetricF64`
values only, or no other values at all. The `compute` method is used to calculate the value of the parameter at a given
timestep and scenario, and therefore `SimpleParameter<T>` can vary with time. This method is resolved in order with
other `SimpleParameter<T>` before `GeneralParameter<T>` and other model components such as nodes.

## The `ConstParameter<T>` trait

The `ConstParameter<T>` trait is used for parameters that depend on `ConstantMetricF64` values only and do
not vary with time. The `compute` method is used to calculate the value of the parameter at the start of the simulation
and is not resolved at each timestep. This method is resolved in order with other `ConstParameter<T>`.

## Implementing multiple traits

A parameter should implement the "lowest" trait in the hierarchy. For example, if a parameter depends on
a `SimpleParameter<T>` and a `ConstParameter<T>` value, it should implement the `SimpleParameter<T>` trait.
If a parameter depends on a `GeneralParameter<T>` and a `ConstParameter<T>` value, it should implement the
`GeneralParameter<T>` trait.

For some parameters it can be beneficial to implement multiple traits. For example, a parameter could be generic to the
metric type (e.g. `MetricF64`, `SimpleMetricF64`, or `ConstantMetricF64`) and implement each of the three
compute traits. This would allow the parameter to be used in the most efficient way possible depending on the
model configuration.

## Return types

While the compute traits are generic over the type `T`, the return type of the `compute` Pywr currently only supports
`f64`, `usize` and `MultiValue` types. The `MultiValue` type is used to return multiple values from the `compute`
method. This is useful for parameters that return multiple values at a given timestep and scenario. See the
documentation for the `MultiValue` type for more information. Implementations of the compute traits are usually for one
of these concrete types.
