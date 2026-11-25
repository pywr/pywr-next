# Variables

Network sets handle variants that differ in structure, where one variant has different nodes, parameters or
connections from another. Many variants differ only in a value instead: which directory of inflow data to
read, or which column of a table to look up. A network set can still handle these, but it needs one fragment per
value, and the fragments are identical apart from a path or a name. With ten time series and five datasets,
that is fifty near-identical entries.

A **variable** handles this kind of variant. Files refer to a variable with `${name}` inside a string, and model
definitions choose its value.

| Kind of variant                                             | Use                                                  |
|-------------------------------------------------------------|------------------------------------------------------|
| A different component type, data source or connection       | [network sets](./network-sets-and-models.md) and [network definitions](./network-definitions.md) |
| A different directory, column name or row key               | variables                                            |

## Declaring variables

Variables are declared in the manifest's `variables` section:

```json
"variables": [
  {
    "name": "inflow_dataset",
    "type": "path",
    "description": "The inflow dataset to read.",
    "default": "historical",
    "choices": {
      "historical": "data/inflows/historical",
      "stoch-01": "data/inflows/stochastic-01",
      "stoch-02": "data/inflows/stochastic-02"
    }
  },
  { "name": "demand_column", "type": "string", "default": "baseline" }
]
```

| Field         | Description                                                                                   |
|---------------|-----------------------------------------------------------------------------------------------|
| `name`        | The variable's name, made of ASCII letters, digits, `_` and `-`. Unique within the project.   |
| `type`        | `string` or `path`.                                                                           |
| `description` | An optional description.                                                                      |
| `default`     | The value used when nothing binds the variable. Optional.                                     |
| `choices`     | An optional map of choice names to values. If given, bindings and the default name a choice.  |

A variable without `choices` takes its value directly, so `"default": "baseline"` above is the value itself.
A variable with `choices` takes the name of a choice, which keeps model definitions short and gives tooling a
list of the values that are allowed.

### Path variables

A `path` variable's value is a relative path within the project directory, such as `data/inflows/historical`.
It must be a plain relative path, without `..`, it must exist, and it must not lead outside the project
directory, including through symbolic links. These are the same rules as for paths in the manifest.

Validating a project checks every choice of a path variable, not only the ones in use. A typo in the path of a
dataset that no model uses yet is still caught. Composing a model checks only the values that the model
actually uses, so if one dataset is missing, for example because it has not been downloaded, models that do
not use it still run.

## Using variables in files

Any string value in a network fragment or base model can refer to a variable with `${name}`:

```json
"time_series": [
  {
    "meta": { "name": "inflows" },
    "type": "Arrow",
    "format": "CSV",
    "time_col": "date",
    "path": "${inflow_dataset}/flows.csv"
  }
],
"nodes": [
  {
    "meta": { "name": "demand1" },
    "type": "Output",
    "max_flow": {
      "type": "Table",
      "table": "demands",
      "row": "demand1",
      "column": "${demand_column}"
    }
  }
]
```

A string can contain several variables and other text, such as `"${scenario}-${year}"`. The following rules
apply:

- **Path variables start their string.** `"${inflow_dataset}/flows.csv"` is allowed, but
  `"data/${inflow_dataset}/flows.csv"` is not. A path variable is always relative to the project directory, so
  that it works wherever the file is. This holds even for files in a [sub-network](./sub-networks.md) with
  its own `data_dir`.
- **Some strings cannot use variables:** object keys, `type` values and the names in `meta.name`. These say
  what a component is and how it is referred to, so they must be fixed for a file to be checked.
- **`$${` writes a literal `${`.** For example, `"$${not_a_variable}"` becomes `"${not_a_variable}"`. A `$`
  that is not followed by `{` needs no escaping.
- A value is inserted as it is. A value containing `${...}` is not searched for further variables.

Variables can only be used in strings, so they cannot replace a number or an object. The manifest itself
never uses variables. Fields such as `dir`, `files` and `base_model` are fixed, so a variable never changes which
files a model is built from. Choosing between files is the job of network sets.

A file can only use the variables of the project it belongs to. Files of a sub-network use the variables of
the sub-network's project.

## Binding variables

A variable gets its value from a **binding**. Network definitions, model definitions and the networks of a
multi-model can bind variables in a `variables` map:

```json
"networks": [
  {
    "name": "stochastic",
    "include": [{ "set": "inflows", "files": ["inflows.json"] }],
    "variables": { "inflow_dataset": "stoch-01" }
  }
],
"models": [
  { "name": "baseline", "networks": ["stochastic"] },
  {
    "name": "baseline-stoch-02",
    "networks": ["stochastic"],
    "variables": { "inflow_dataset": "stoch-02" }
  },
  {
    "name": "demand-2030",
    "networks": ["stochastic"],
    "variables": { "demand_column": "2030" }
  }
]
```

`baseline` reads `data/inflows/stochastic-01`, as the `stochastic` network definition binds it.
`baseline-stoch-02` binds `inflow_dataset` itself, so it reads `data/inflows/stochastic-02`. `demand-2030`
keeps the network definition's dataset, and changes the demand column.

### Precedence

When more than one binding applies to a variable, the value is chosen as follows:

1. A variable's `default` applies if nothing else binds it.
2. A definition's own bindings override every binding it pulls in. A definition pulls in bindings from the
   network definitions in its `extends` or `networks`, from the source of a multi-model network, and from the
   sub-networks it activates.
3. If two things that a definition pulls in bind a variable to different values, that is an error, unless the
   definition binds the variable itself to settle it.
4. Values given on the [command line](#on-the-command-line) override everything.

The same rule applies at every level. A network definition can override a value from a definition it extends.
A model that uses two network definitions that disagree about a variable must choose the value itself:

```text
The model definition `combined` uses definitions that bind the variable `inflow_dataset` to both `stoch-01` and `stoch-02`; bind it in the model definition `combined` to choose.
```

A variable has one value for the whole of a composed model. Every file that uses it sees the same value,
whichever network definition included the file. In a [multi-model](./multi-models.md), each network is bound
separately, so two networks can read different datasets:

```json
"multi_models": [
  {
    "name": "compare-datasets",
    "domain": { "type": "Model", "name": "baseline" },
    "networks": [
      {
        "name": "historical",
        "network": { "type": "Network", "name": "river" },
        "variables": { "inflow_dataset": "historical" }
      },
      {
        "name": "stochastic",
        "network": { "type": "Network", "name": "river" },
        "variables": { "inflow_dataset": "stoch-01" }
      }
    ]
  }
]
```

Binding a variable that none of the model's files use is not an error. A network definition often binds a
value that only some of the models using it need.

### On the command line

`run-project` takes `--var NAME=VALUE`, which can be given more than once:

```sh
pywr-cli run-project my-model/pywr-project.json --definition baseline --var inflow_dataset=stoch-02 --output-path results
```

A command-line value overrides every binding in the manifest, and applies to every network of a multi-model.
It follows the same rules as a binding in a manifest. A variable with `choices` is given by choice name. A
`path` variable without choices takes a path relative to the project directory. Two differences
help to catch typos: a variable given twice is an error, and so is a variable that no file of the model uses.

## Variables of dependencies

A [dependency](./dependencies.md) does not see the variables of the project that uses it. Each project's files
use only that project's own variables, and the dependency's own definitions can bind them. A project using the
dependency can override them by their qualified name, such as `org::demand_column`, but only if the dependency
exports them:

```json
"exports": {
  "networks": ["river"],
  "variables": ["demand_column", "inflow_dataset"]
}
```

```json
{ "name": "joint-2030", "networks": ["org::river"], "variables": { "org::demand_column": "2030" } }
```

The same qualified names work with `--var`, as in `--var org::demand_column=2030`.

A path value is relative to the project whose manifest gives it. The choices of a dependency's variable are
paths in the dependency. A value given directly by the project using the dependency, for a path variable
without choices, is a path in that project. Either way, the data path in the composed model is rewritten to be
relative to the project being run, as for any other data path of a dependency.

## Validation

Validating a project reads every network file and base model that its definitions use, and checks their
variables as well as the manifest. Problems include:

- a variable name that is duplicated or invalid, a `default` that is not one of the `choices`, and a `path`
  value or choice that does not exist or is outside the project;
- a binding of a variable that is not declared or not exported, or to a value that is not one of its choices;
- two bindings that conflict, as [described above](#precedence);
- a `${...}` that is not closed, has an invalid name, or is in a string that cannot use variables;
- a file that uses a variable its project does not declare, or a path variable that does not start its string;
- a model that reads a file using a variable with no binding and no default, or whose default cannot be used.

For example:

```text
The file `catchment/inflows/inflows.json` uses the path variable `inflow_dataset` at `/time_series/0/path` after the start of the string.
```

The location after `at` is a [JSON pointer](https://datatracker.ietf.org/doc/html/rfc6901) to the string
within the file.

A network definition often leaves a variable for the models that use it to bind. So when the whole project is
validated, a network definition's variables without a value are not reported. They are reported for each
model that leaves them without a value.

## Run metadata

A composed model contains no `${...}`: every variable has been replaced by its value. The metadata of a run
records the hashes of the project's files as they are on disk, with the variables still in them, so it also
records the value of each variable used. Each entry gives:

| Field     | Description                                                                                  |
|-----------|----------------------------------------------------------------------------------------------|
| `network` | The network of a multi-model the value applies to, if any.                                   |
| `name`    | The variable, named as the project being run refers to it, such as `org::demand_column`.     |
| `value`   | The choice name if the variable has choices, otherwise its value.                            |
| `path`    | For a `path` variable, the path used, relative to the project being run.                     |
| `source`  | Where the value came from: the variable's default, the command line, or a definition, with its kind and name. |

Only variables that the model's files actually use are recorded. The metadata is written with the model's
Arrow and Parquet outputs, and the Python package reads it as `VariableMetadata`.
