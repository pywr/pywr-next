# Dependencies

Sometimes the parts of a model belong to different projects. Two organisations might each maintain a model of
their own system, and a joint study needs both. Rather than copying files between projects, a project can
**depend** on another and use its definitions directly. Each project keeps its own files, history and
releases, and the joint project always uses the current versions.

```json
"dependencies": [
  { "name": "neighbour", "manifest": "../neighbour/pywr-project.json" }
]
```

| Field      | Description                                                                                   |
|------------|-----------------------------------------------------------------------------------------------|
| `name`     | The name this project uses for the dependency.                                                |
| `manifest` | The path to the dependency's manifest, relative to this project's directory.                  |
| `sha256`   | The expected SHA-256 hash of the dependency's manifest file, in hexadecimal. Optional.        |
| `data_dir` | The directory, relative to the dependency's directory, that its files' data paths are relative to. Defaults to the dependency's directory. |

The `manifest` path is the only path in a manifest that may lead outside the project directory, through `..`.
Everything else in a dependency, including its own paths, must stay inside the dependency's directory.

## Using a dependency's definitions

A dependency's items are referred to by qualified names that start with the dependency's name, in the same
way as [sub-networks](./sub-networks.md). Sub-networks and dependencies share these names, so a dependency
cannot have the same name as a sub-network.

| Item                         | Example                                     | Used in                                       |
|------------------------------|---------------------------------------------|-----------------------------------------------|
| Network definition           | `"org::river"`                              | `networks`, `extends`, multi-model `network`  |
| Network set                  | `{ "set": "org::licences", ... }`           | `include`                                     |
| Network set of a sub-network | `{ "set": "org::upper::options", ... }`     | `include`                                     |
| Sub-network                  | `"org::upper"`                              | a network definition's `base`, `sub_networks` |
| Model definition             | `{ "model": "org::baseline" }`              | a multi-model network's `network`             |

A dependency's items are used as if they were local, with three differences:

- **Time domain:** a dependency's model definitions can only be used for their networks, as the networks of a
  multi-model. A model's or multi-model's time domain, scenarios and metadata always come from the project
  being run, so there is never a question of which project's time domain applies.
- **Data paths:** relative data paths in a dependency's files are rewritten to be relative to the root
  project, in the same way as for sub-networks. A time series at `data/demand.csv` in the `neighbour`
  project becomes `../neighbour/data/demand.csv`. If no relative path exists, as between drives on Windows,
  the absolute path is used.
- **Set limits:** the `min_files` and `max_files` limits of a dependency's network sets only apply to the sets
  a model selects from. The dependency's other sets may have limits meant for its own models.

## Exports

A project can declare what other projects may use from it with `exports`. Anything not listed is private, so
the project can reorganise it without breaking the projects that depend on it.

[//]: # (@formatter:off)

```json
{{#include ../../../examples/neighbour/pywr-project.json}}
```
[//]: # (@formatter:on)

`exports` has four lists: `networks`, `models`, `network_sets` and `sub_networks`. Network sets of a
sub-network are listed by their qualified name, such as `upper::options`. A project without `exports` exports
everything. `exports` has no effect on the project's own definitions.

## Dependencies of dependencies

Dependencies are loaded transitively: a dependency's own dependencies are loaded relative to its directory.
Their items can be referred to through both names, such as `org::shared::network`, but only if the dependency in
the middle has no `exports`. A project that needs something from a dependency of an exporting project can
depend on that project directly.

A project that is reached by more than one route is loaded once, and a file reached by more than one route is
merged once. Projects that depend on each other in a cycle are reported as an error, with the manifests that
form it.

## Problems in dependencies

Problems found in a dependency are reported as being in it, for example:

```text
In the dependency `org`: The definition `river` selects the network set `missing`, which does not exist.
```

When a project is validated, each dependency's manifest is checked as a whole, and its definitions are checked
as far as the project uses them.

## Security

Without dependencies, a manifest cannot read anything outside its own directory, which makes it reasonably safe
to validate a manifest from an untrusted source. Dependencies relax this, so their use can be restricted when
opening a project from Rust:

```rust,ignore
use pywr_project::{DependencyPolicy, Project, ResolveOptions};

let options = ResolveOptions {
    dependency_policy: DependencyPolicy::AllowWithin("/srv/projects".into()),
};
let project = Project::open_with("/srv/projects/joint/pywr-project.json", options)?;
```

| Policy                    | Effect                                                         |
|---------------------------|----------------------------------------------------------------|
| `Allow`                   | Any dependency may be loaded. This is the default.             |
| `Deny`                    | No dependencies may be loaded.                                 |
| `AllowWithin(directory)`  | Only dependencies whose manifests are inside the directory.    |

A dependency's `sha256` pins its manifest to a known version: if the manifest's hash differs, the dependency
is not loaded. The hash covers only the manifest file, not the files it refers to.
