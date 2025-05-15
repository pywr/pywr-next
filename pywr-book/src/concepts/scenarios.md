# Scenarios

Pywr has built-in support for running multiple scenarios. Scenarios are a way to define different sets of input data
or parameters that can be used to run a model. This is often useful for running sensitivity analysis, stochastic
hydrological data, or climate change scenarios. Pywr's scenario system is used to define a set of simulations that
are, by default, all run in together. This requires that all scenarios simulate the same time period and have the same
time step. However, it means that Pywr can take advantage of efficiencies by running through the same time-domain once.
For example, the majority of the data required for the model can be loaded once and then shared between the scenarios.
Pywr v2.x system is more flexible and crucially allows for scenarios to be run in parallel without the need for
multiprocessing (which duplicates memory usage).

In this section, we will cover how to define scenarios in Pywr and how to run them.

## Defining Scenarios

Scenarios are defined in the `scenarios` section of the model configuration file. A model can have multiple scenario
groups, each defining a set of scenarios. By default, Pywr will run the full combination of all scenarios in all
groups. If no scenarios are defined, Pywr will run a single scenario.

The simplest scenario definition contains a `groups` list of a single scenario group with a name and size. The following
example defines such scenario domain with a single group containing 5 scenarios. If this Pywr model is run, it will
run 5 scenarios.

[//]: # (@formatter:off)

```json
{{#include ../../../pywr-schema/src/doc_examples/scenario_domain1.json}}
```
[//]: # (@formatter:on)

By default, the scenarios in a group will be given a numeric label starting from 0. However, it is possible to define
a `labels` list to give scenarios more meaningful names. The following example defines a scenario group with 5
scenarios using Roman numerals as labels.

[//]: # (@formatter:off)

```json
{{#include ../../../pywr-schema/src/doc_examples/scenario_domain2.json}}
```
[//]: # (@formatter:on)

Additional scenario groups can be defined by adding them to the `groups` list. The following example
groups, "A" and "B", with sizes 5 and 3 respectively. This domain would create 15 simulations.

[//]: # (@formatter:off)

```json
{{#include ../../../pywr-schema/src/doc_examples/scenario_domain3.json}}
```
[//]: # (@formatter:on)

## Running subsets of scenarios

It is often useful to run only a subset of the scenarios defined in a model. This can be done by either specifying
the specific scenarios in each group to run, or by providing specific combinations of scenarios to run.

> **Note**: These approaches are mutually exclusive.

### Subsetting groups

To run only a subset of scenarios in a group, the `subset` key can be used. The following examples shows three
groups, each with 5 scenarios. The `subset` key is used to specify the scenarios to run in each group. The
first group is subset using the scenario group's labels, the second group is subset using the scenario group's
indices, and the third group is subset using a slice. In all cases the subset will mean the 2nd, 3rd and 4th scenarios
are run. This will result in 9 (3 x 3 x 3) simulations using the product of the subsets.

> **Note**: The indices and slice are zero-based.

[//]: # (@formatter:off)

```json
{{#include ../../../pywr-schema/src/doc_examples/scenario_domain4.json}}
```
[//]: # (@formatter:on)

### Specifying specific combinations

To run specific combinations of scenarios, the `combinations` key can be used. The following examples shows three
groups, each with 5 scenarios. The `combinations` key is used to specify the exact scenarios to run. Each combination
is a list of scenario indices to run. The example shows that the 1st, 3rd and 5th scenarios in each group are run.
This will result in 3 simulations.


[//]: # (@formatter:off)

```json
{{#include ../../../pywr-schema/src/doc_examples/scenario_domain5.json}}
```
[//]: # (@formatter:on)
