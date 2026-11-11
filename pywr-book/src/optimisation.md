# Optimisation

Pywr models can be used with optimisation algorithms, such as multi-objective evolutionary
algorithms, to search for good values of some of a model's parameters. The algorithm repeatedly
chooses values for these _variable_ parameters, runs the model with them, and uses the results to
calculate the _objectives_ (values to minimise or maximise) and _constraints_ (values that must be
within bounds).

Pywr does not include any optimisation algorithms. Instead, it provides a way to define a problem
for a model, and to evaluate the problem for given variable values, which can be used with an
optimisation library of your choice.

## Defining a problem

A problem is defined in a JSON file that is separate from the model, so that a model can be used
with different problems. The file has three sections:

- `variables`: the parameters whose values can be varied, and how. The values of the variables
  are given as a list, and are split between the parameters in the order they are listed here.
- `objectives`: the outputs whose aggregated values are minimised or maximised.
- `constraints` (optional): the outputs whose aggregated values must be within a lower and/or
  upper bound.

Objectives and constraints refer to outputs by name. The outputs must be able to aggregate their
results to a single value, such as a `Memory` output with a time and scenario aggregation.

The example below defines a problem for a model with a single demand. The demand is variable
between 0.0 and 20.0. The objectives are to minimise the mean demand and to maximise the mean
delivered flow, and a constraint requires the mean delivered flow to be at least 5.0.

```json
{{#include ../py-listings/optimisation/problem.json}}
```

### Variable parameters

Each variable parameter has a `config` whose `type` depends on the type of the parameter:

| Config type          | Parameter types        | Variable values                                                           |
|----------------------|------------------------|---------------------------------------------------------------------------|
| `ActivationFunction` | `Constant`, `Offset`   | One value, transformed by the activation function.                        |
| `MonthlyProfile`     | `MonthlyProfile`       | Twelve values, one for each month, within the given bounds.               |
| `RbfProfile`         | `RbfProfile`           | One value per point, and optionally one day of the year per point.        |

Activation functions transform the variable value before it is used by the parameter. For
example, the `Unit` function uses the value directly within its `min` and `max`, while the
`BinaryStep` function returns one of two values depending on whether the variable value is
positive. This allows, for example, a binary-like decision to be represented by a continuous
variable.

Most variable values are floats. The day of the year of an `RbfProfile`'s points are integers,
and are given separately from the float values.

## Evaluating a problem in Python

A problem is built from a model schema and a `ProblemConfig`:

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/optimisation/random_search.py:build}}
```

[//]: # (@formatter:on)

The `ModelProblem` gives the number of variable values (`size`), their bounds (`lower_bounds()`
and `upper_bounds()`), and the names and senses of the objectives and constraints. Its
`evaluate` method runs the model with the given variable values, and returns an `Evaluation` of
the objective and constraint values. `evaluate_batch` evaluates several sets of values in
parallel, which is useful for population-based algorithms.

The objective values are returned as calculated, so objectives that are maximised are not
negated. `Evaluation.constraint_violations` gives the amount by which each constraint is outside
its bounds, which is zero when the constraint is satisfied.

The listing below uses a simple random search to find the values that give the best trade-off
between the two objectives.

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/optimisation/random_search.py:search}}
```

[//]: # (@formatter:on)

> **Note:** Each evaluation sets up and runs the model from scratch, including all of its outputs.
> Outputs that write files, such as CSV or HDF5 outputs, will rewrite their files on every
> evaluation. Models used for optimisation should normally only use `Memory` outputs.
