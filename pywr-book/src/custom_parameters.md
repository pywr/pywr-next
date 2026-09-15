# Extending functionality with custom parameters

Parameters are a core part of Pywr, allowing you to define how your model behaves. While Pywr comes with a wide range of
built-in parameters, you may find that you need to create custom parameters to suit your specific modelling needs. This
guide will walk you through the process of creating custom parameters in Pywr.

Currently, Pywr supports custom parameters that are defined in Python. If your parameter is general enough, you may want
to consider contributing it to the Pywr project. If you do, please see
the [Developers Guide](./developers-guide/adding-a-parameter.md) for more information on how to do this.

## Python functions

The simplest way to create a custom parameter is to define a Python function. This function should accept at least one
argument, which is a `ParameterInfo` object. This read-only object exposes the current `timestep`, `scenario_index`,
and any declared metric and index dependencies. Additional arguments can also be passed to the function. Functions are
evaluated in the `before` phase, before resource allocation.

`info.timestep` provides properties such as `index`, `date`, `day`, `month`, `year`, `days`, and `is_first`.
`info.scenario_index` provides `simulation_id` and `simulation_indices`. Declared floating-point metrics and unsigned
integer indices can be read using `info.get_metric(key)` and `info.get_index(key)` respectively. Requesting an
undeclared key raises `KeyError`.

Here is an example of a simple custom parameter that returns the current time step:

```python
# custom_parameters.py
from pywr import ParameterInfo


def current_time_step(info: ParameterInfo) -> float:
    """Return the current time step."""
    return info.timestep.index
```

To use this custom parameter in your model it must be defined as a `Parameter` in your model's JSON file. Below is an
example of how to define the `current_time_step` parameter in your model's JSON file. The `source` field specifies the
path to the Python file containing the function, and the `object` field specifies the name of the function to call.

```json
{
  "parameters": [
    {
      "meta": {
        "name": "current-time-step"
      },
      "type": "Python",
      "source": {
        "type": "Path",
        "path": "custom_parameters.py"
      },
      "object": {
        "type": "Function",
        "function": "current_time_step"
      },
      "args": [],
      "kwargs": {}
    }
  ]
}
```

### Constant arguments

In reality, your function will likely need to accept additional arguments. These arguments might be constants that
change the behaviour of the function, but do *not* change over time or result from the model's simulation state. In
this case they can be defined as `args` or `kwargs` in the parameter definition. Any JSON value can be used, including
`null`, arrays, and objects; these are passed to Python as `None`, lists, and dictionaries. Non-JSON Python objects
cannot be supplied this way. By parameterising these values, you can easily change them without modifying the Python
code or reuse the same function with different values in different parts of the model.

```python
# custom_parameters.py
from pywr import ParameterInfo


def current_timestep(
        info: ParameterInfo, a: float, b: float, some_condition: str = "foo"
) -> float:
    """Return the current time step."""
    match some_condition:
        case "foo":
            return info.timestep.index + a
        case "bar":
            return info.timestep.index + b
        case _:
            raise ValueError(f"Invalid condition: {some_condition}")
```

To pass these arguments to the function, you can define them in the model's JSON file as follows:

```json
{
  "parameters": [
    {
      "meta": {
        "name": "current-time-step"
      },
      "type": "Python",
      "source": {
        "type": "Path",
        "path": "custom_parameters.py"
      },
      "object": {
        "type": "Function",
        "function": "current_timestep"
      },
      "args": [
        1.0,
        2.0
      ],
      "kwargs": {
        "some_condition": "foo"
      }
    }
  ]
}
```

### Metrics from the model

More complex parameters will need information from the model, such as the current volume of a reservoir, or the value of
another parameter, etc. These values need to be requested in the JSON definition of parameter, and then they can be
accessed in the function using the `ParameterInfo` object.

```python
# custom_parameters.py
from pywr import ParameterInfo


def factor_volume(info: ParameterInfo, factor: float) -> float:
    """Return the current volume of a reservoir scaled by `factor`."""
    volume = info.get_metric("volume")
    return factor * volume
```

The JSON definition of the parameter needs to include a `metrics` and/or `indices` field that specifies which model
dependencies to request. Both fields are dictionaries: their keys are used to retrieve the values from the
`ParameterInfo` object, and their values specify the metric or index to retrieve. `metrics` provide floating-point
values accessed using `get_metric(key)`, while `indices` provide unsigned integer values accessed using
`get_index(key)`.

```json
{
  "parameters": [
    {
      "meta": {
        "name": "factor-volume"
      },
      "type": "Python",
      "source": {
        "type": "Path",
        "path": "custom_parameters.py"
      },
      "object": {
        "type": "Function",
        "function": "factor_volume"
      },
      "args": [
        2.0
      ],
      "metrics": {
        "volume": {
          "type": "Node",
          "name": "a-reservoir",
          "attribute": "Volume"
        }
      }
    }
  ]
}

```

## Python classes & stateful parameters

If your parameter needs to maintain state between calls, you can define it as a Python class. This class should
implement an `__init__` method that sets up the parameter, including any initial state. The `__init__` method is
passed the `args` and `kwargs` defined in the JSON file. Pywr will create an instance of the class for every scenario in
a simulation. These instances will be reused for each time step in the scenario, allowing you to maintain state across
time steps.

> **Note**: Unlike Pywr v1.x a separate instance of the class is created for each scenario.
> This means you do not have to worry about state being shared between scenarios, and do *not* need to implement
> state for each scenario yourself.

The class must implement at least one of `before` or `after`. These methods accept a `ParameterInfo` object as their
only argument and should return the parameter's configured value type. `before` is called before resource allocation,
and `after` is called after allocation. A class can implement either method or both.

For a parameter that needs only a post-allocation state update, a class with `before` may instead
implement `after_hook`. It is called after allocation, must return `None`, and cannot be used together with `after`.

Here is an example of a simple stateful parameter that counts the number of time steps:

```python
# custom_parameters.py
from pywr import ParameterInfo


class TimeStepCounter:
    """A parameter that counts the number of time steps."""

    def __init__(self, initial_value: int = 0):
        self.count = initial_value

    def before(self, _info: ParameterInfo) -> float:
        """Return the current time step count."""
        # Note that `_info` is not used, but it is required by the interface.
        self.count += 1
        return float(self.count)
```

To use this custom parameter in your model, you can define it in the JSON file as follows:

```json
{
  "parameters": [
    {
      "meta": {
        "name": "time-step-counter"
      },
      "type": "Python",
      "source": {
        "type": "Path",
        "path": "custom_parameters.py"
      },
      "object": {
        "type": "Class",
        "class": "TimeStepCounter"
      },
      "args": [
        0
      ],
      "kwargs": {}
    }
  ]
}
```

## Using modules instead of files

It might be more convenient to define your custom parameters in a Python module instead of a file. This allows you to
integrate your custom parameters with other Python code, such as unit tests or other utility functions. To do this, you
can use the `source` field to specify the module name instead of a file path.

Here is an example of how to define a custom parameter in a module (in this case `my_model.parameters`):

```json
{
  "parameters": [
    {
      "meta": {
        "name": "current-time-step"
      },
      "type": "Python",
      "source": {
        "type": "Module",
        "module": "my_model.parameters"
      },
      "object": {
        "type": "Function",
        "function": "current_time_step"
      },
      "args": [],
      "kwargs": {}
    }
  ]
}
```

## Returning integers or multiple values

In the examples above the custom parameter functions return a single floating point value. You can also return unsigned
integers or multiple values by specifying `return_type`. `"Int"` creates an unsigned integer (index) parameter, so the
returned value must be a non-negative integer that fits in a 64-bit unsigned integer.

Set `return_type` to `"Dict"` to return multiple named values. Each dictionary value must be a Python `float` or a
non-negative Python `int`: floats become named floating-point values and integers become named unsigned index values.
Other value types, including negative integers, are not supported.

An example of a custom parameter that returns multiple values is shown below:

```python
# custom_parameters.py
from pywr import ParameterInfo


def multiple_values(info: ParameterInfo, factor: float) -> dict[str, float | int]:
    """Return multiple values."""
    return {"value1": info.timestep.index, "value2": info.get_metric("volume") * factor}
```

The corresponding JSON for this parameter would look like this:

```json
{
  "parameters": [
    {
      "meta": {
        "name": "multiple_values"
      },
      "type": "Python",
      "source": {
        "type": "Module",
        "module": "my_model.parameters"
      },
      "object": {
        "type": "Function",
        "function": "multiple_values"
      },
      "return_type": "Dict",
      "args": [
        2.0
      ],
      "metrics": {
        "volume": {
          "type": "Node",
          "name": "a-reservoir",
          "attribute": "Volume"
        }
      }
    }
  ]
}
```

The returned values can be accessed in the model using the keys defined in the dictionary. For example:

```json
{
  "type": "Parameter",
  "name": "multiple_values",
  "key": "value1"
}
```

or

```json
{
  "type": "Parameter",
  "name": "multiple_values",
  "key": "value2"
}
```

## Before and after methods

The majority of parameters will only need to implement the `before` method, which is called before the resource
allocation is performed for the time step[^before_v1]. However, in some cases it may be necessary to perform some
calculations after allocation, such as accessing allocated flow or new reservoir volume. A class parameter can implement
an `after` method to return a post-allocation parameter value, or `after_hook` to update state without returning a
value.

The example below lists a custom parameter that implements both `before` and `after` methods. It is a simple crop water
requirement parameter that calculates the water requirement for a crop based on the current month in `before`, and then
computes a crop yield in `after` based on the allocated water and the water requirement.

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/agri-parameter/agri_parameter.py}}
```

When referring to a parameter in the model, the `return_value` field on the *parameter reference* (not on the Python
parameter definition) selects the calculation phase. The default is `"Before"`; use `"After"` to consume the value
returned by `after`. The other supported reference values are `"AfterOrElseInitial"` and `"Both"`. The selected phase
must be compatible with the parameter implementation. The example below uses the `CropParameter` above in a metric set
and selects its `after` value.

[//]: # (@formatter:off)

```json,ignore
    "metric_sets": [
      {
        "name": "parameters",
        "aggregator": {
          "freq": {
            "type": "Annual"
          },
          "func": {
            "type": "Sum"
          }
        },
        "metrics": [
          {
            "type": "Parameter",
            "name": "crop1",
            "return_value": "After"
          }
        ]
      }
    ],
```


[^before_v1]: This also is the same as Pywr v1.x where the `before` method was the only method that could be
implemented.

## Cython (and other compiled languages)

Cython functions and classes can be used in Pywr as long as they accessible from Python, and can be
imported by Pywr at runtime. In this case using a module for locating the custom parameter is recommended.
Otherwise, there is no difference in how you define the custom parameter in the model's JSON file.

Other compiled languages can also be used, but you will need to ensure that the compiled code is accessible from Python.
This can be done by using a Python wrapper around the compiled code, or by using a foreign function interface (FFI)
such as [`ctypes`](https://docs.python.org/3/library/ctypes.html) or [`cffi`](https://cffi.readthedocs.io/en/stable/).
