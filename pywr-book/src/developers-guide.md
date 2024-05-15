# Developers guide

This guide is intended for developers who wish to contribute to the Pywr project.

## Adding a new node to the schema

To add new a new node to the schema you need to create a new struct that contains the required data for the node.
This new struct will need to be added as a variant in the `Node` enum.
This will then require the variant to be added to several `match` statements in the codebase (the compiler or Clippy
will guide you to where the problems are).
To complete those changes, you will need to implement several methods.
Currently, these methods are not part of a single trait, but that may change in the future.

To facilitate the "core" feature of the `pywr-schema` crate the methods are typically divided into two groups.
The first group are *not* feature gated and are built every time the crate is built.
The second group are only required if the "core" feature is enabled.
This feature adds a dependency on the `pywr-core` crate which contains the core functionality of the Pywr model.
The methods in the second group are therefore related to constructing a model from the schema.
In general, it is best to use one of the existing nodes as a template for your new node.

Most nodes should follow a pattern of adding a default version of themselves to the schema at the start of
`add_to_model`, and then load and set the constraints by retrieving a mutable reference to the newly added node.
This is important to ensure that node constraints can depend on the node itself.
