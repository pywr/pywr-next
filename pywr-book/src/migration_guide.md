# Migrating from Pywr v1.x

This guide is intended to help users of Pywr v1.x migrate to Pywr v2.x. Pywr v2.x is a complete rewrite of Pywr with a
new API and new features. This guide will help you update your models to this new version.

## Overview of the process

Pywr v2.x includes a more strict schema for defining models. This schema, along with the
[pywr-v1-schema](https://crates.io/crates/pywr-v1-schema) crate, provide a way to convert models from v1.x to v2.x.
However, this process is not perfect and will more than likely require manual intervention to complete the migration.
The migration of larger and/or more complex models will require an iterative process of conversion and testing.

The overall process will follow these steps:

1. Convert the JSON from v1.x to v2.x using the provided conversion tool.
2. Handle any errors or warnings from the conversion tool.
3. Apply any other manual changes to the converted JSON.
4. (Optional) Save the converted JSON as a new file.
5. Load and run the new JSON file in Pywr v2.x.
6. Compare model outputs to ensure it behaves as expected. If necessary, make further changes to the above process and
   repeat.

## Converting a model

The example below is a basic script that demonstrates how to convert a v1.x model to v2.x. This process converts
the model at runtime, and does not replace the existing v1.x model with a v2.x definition.

> **Note**: This example is meant to be a starting point for users to build their own conversion process;
> it is not a complete generic solution.

The function in the listing below is an example of the overall conversion process.
The function takes a path to a JSON file containing a v1 Pywr model, and then converts it to v2.x.

1. The function reads the JSON, and applies the conversion function (`convert_model_from_v1_json_string`).
2. The conversion function that takes a JSON string and returns a tuple of the converted JSON string and a list of
   errors.
3. The function then handles these errors using the `handle_conversion_error` function.
4. After the errors are handled other arbitrary changes are applied using the `patch_model` function.
5. Finally, the converted JSON can be saved to a new file and run using Pywr v2.x.

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/convert.py:convert}}
```

[//]: # (@formatter:on)

### Handling conversion errors

The `convert_model_from_v1_json_string` function returns a list of errors that occurred during the conversion process.
These errors can be handled in a variety of ways, such as modifying the model definition, raising exceptions, or
ignoring them.
It is suggested to implement a function that can handle these errors in a way that is appropriate for your use case.
Begin by matching a few types of errors and then expand the matching as needed. By raising exceptions
for unhandled errors, you can ensure that all errors are eventually accounted for, and that new errors are not missed.

The example handles the `ComponentConversionError` by matching on the error subclass (either `Parameter()` or `Node()`),
and then handling each case separately.
These two classes will contain the name of the component and optionally the attribute that caused the error.
In addition, these types contain an inner error (`ConversionError`) that can be used to provide more detailed
information.
In the example, the `UnrecognisedType()` class is handled for `Parameter()` errors by applying the
`handle_custom_parameters` function.

This second function adds a Pywr v2.x compatible custom parameter to the model definition using the same name
and type (class name) as the original parameter.

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/convert.py:handle_conversion_error}}
```

[//]: # (@formatter:on)

### Other changes

The upgrade to v2.x may require other changes to the model.
For example, the conversion process does not currently handle recorders and other model outputs.
These will need to be manually added to the model definition.
Such manual changes can be applied using, for example a `patch_model` function.
This function will make arbitrary changes to the model definition.
The example, below updates the metadata of the model to modify the description.

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/convert.py:patch_model}}
```

[//]: # (@formatter:on)

### Full example

The complete example below demonstrates the conversion process for a v1.x model to v2.x:

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/convert.py}}
```

[//]: # (@formatter:on)

## Converting custom parameters

The main changes to custom parameters in Pywr v2.x are as follows:

1. Custom parameters are no longer required to be a subclass of `Parameter`. They instead can be simple Python
   functions, or classes
   that implement a `calc` method.
2. Users are no longer required to handle scenarios within custom parameters. Instead an instance of the custom
   parameter is created for each scenario in the simulation. This simplifies writing parameters and removes the risk of
   accidentally contaminating state between scenarios.
3. Custom parameters are now added to the model using the "Python" parameter type. I.e. the "type" field in the
   parameter definition should be set to "Python" (not the class name of the custom parameter). This parameter type
   requires that the user explicitly define which metrics the custom parameter requires.

For more information on custom parameters, see the
[Custom parameters](./custom_parameters.md) section of the documentation.

### Simple example

v1.x custom parameter:

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/v1_custom_parameter.py}}
```

[//]: # (@formatter:on)

v2.x custom parameter:

[//]: # (@formatter:off)

```python,ignore
{{ #include ../py-listings/model-conversion/v2_custom_parameter.py}}
```

[//]: # (@formatter:on)
