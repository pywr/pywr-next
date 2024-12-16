import json
from pathlib import Path

# ANCHOR: convert
from pywr import (
    convert_model_from_v1_json_string,
    ComponentConversionError,
    ConversionError,
    Schema,
)


def convert(v1_path: Path):
    with open(v1_path) as fh:
        v1_model_str = fh.read()
    # 1. Convert the v1 model to a v2 schema
    schema, errors = convert_model_from_v1_json_string(v1_model_str)

    schema_data = json.loads(schema.to_json_string())
    # 2. Handle any conversion errors
    for error in errors:
        handle_conversion_error(error, schema_data)

    # 3. Apply any other manual changes to the converted JSON.
    patch_model(schema_data)

    schema_data_str = json.dumps(schema_data, indent=4)
    # 4. Save the converted JSON as a new file (uncomment to save)
    # with open(v1_path.parent / "v2-model.json", "w") as fh:
    #     fh.write(schema_data_str)
    print("Conversion complete; running model...")
    # 5. Load and run the new JSON file in Pywr v2.x.
    schema = Schema.from_json_string(schema_data_str)
    model = schema.build(Path(__file__).parent, None)
    model.run("clp")
    print("Model run complete 🎉")


# ANCHOR_END: convert
# ANCHOR: handle_conversion_error
def handle_conversion_error(error: ComponentConversionError, schema_data):
    """Handle a schema conversion error.

    Raises a `RuntimeError` if an unhandled error case is found.
    """
    match error:
        case ComponentConversionError.Parameter():
            match error.error:
                case ConversionError.UnrecognisedType() as e:
                    print(
                        f"Patching custom parameter of type {e.ty} with name {error.name}"
                    )
                    handle_custom_parameters(schema_data, error.name, e.ty)
                case _:
                    raise RuntimeError(f"Other parameter conversion error: {error}")
        case ComponentConversionError.Node():
            raise RuntimeError(f"Failed to convert node `{error.name}`: {error.error}")
        case _:
            raise RuntimeError(f"Unexpected conversion error: {error}")


def handle_custom_parameters(schema_data, name: str, p_type: str):
    """Patch the v2 schema to add the custom parameter with `name` and `p_type`."""

    # Ensure the network parameters is a list
    if schema_data["network"]["parameters"] is None:
        schema_data["network"]["parameters"] = []

    schema_data["network"]["parameters"].append(
        {
            "meta": {"name": name},
            "type": "Python",
            "source": {"path": "v2_custom_parameter.py"},
            "object": p_type,  # Use the same class name in v1 & v2
            "args": [],
            "kwargs": {},
        }
    )


# ANCHOR_END: handle_conversion_error
# ANCHOR: patch_model
def patch_model(schema_data):
    """Patch the v2 schema to add any additional changes."""
    # Add any additional patches here
    schema_data["metadata"]["description"] = "Converted from v1 model"


# ANCHOR_END: patch_model

if __name__ == "__main__":
    pth = Path(__file__).parent / "v1-model.json"
    convert(pth)
