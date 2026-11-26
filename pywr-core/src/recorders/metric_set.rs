use crate::metric::{MetricConsumerPhase, MetricF64, MetricF64Error, MetricF64ResolutionError, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::node::{CoreNodeAttribute, Node, UnresolvedNode};
use crate::recorders::aggregator::{Aggregator, AggregatorState, PeriodValue};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use pywr_output_metadata::MetricSource;
use thiserror::Error;

/// A container for a [`MetricF64`] that retains additional information from the schema.
///
/// This is used to store the source and attribute of the metric so that it can be output in
/// a context that is relevant to the originating schema, and therefore more meaningful to the user.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputMetric {
    source: MetricSource,
    /// The source's label, kept as outputs read it for every value.
    name: String,
    attribute: String,
    metric: MetricF64,
}

impl OutputMetric {
    pub fn get_value(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.metric.get_value(network, state)
    }

    /// What the metric reads from.
    pub fn source(&self) -> &MetricSource {
        &self.source
    }

    /// The single-string label of the source; see [`MetricSource::label`].
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn attribute(&self) -> &str {
        &self.attribute
    }

    /// Get a fully qualified name for the metric that includes the kind and type (if any) of its
    /// source and its attribute.
    pub fn fully_qualified_name(&self) -> String {
        match self.source.sub_kind() {
            Some(sub_kind) => format!("{}.{}.{}.{}", self.source.kind(), sub_kind, self.name, self.attribute),
            None => format!("{}.{}.{}", self.source.kind(), self.name, self.attribute),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnresolvedOutputMetric {
    source: MetricSource,
    attribute: String,
    metric: UnresolvedMetricF64,
}

impl UnresolvedOutputMetric {
    pub fn new(source: MetricSource, attribute: &str, metric: UnresolvedMetricF64) -> Self {
        Self {
            source,
            attribute: attribute.to_string(),
            metric,
        }
    }

    pub fn resolve(self, resolution_maps: &ResolutionMaps) -> Result<OutputMetric, MetricF64ResolutionError> {
        let metric = self.metric.resolve(resolution_maps, MetricConsumerPhase::After)?;
        Ok(OutputMetric {
            name: self.source.label(),
            source: self.source,
            attribute: self.attribute,
            metric,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MetricSetState {
    // Populated with any yielded values from the last processing.
    current_values: Option<Vec<PeriodValue<f64>>>,
    // If the metric set aggregates then this state tracks the aggregation of each metric
    aggregation_states: Option<Vec<AggregatorState>>,
}

impl MetricSetState {
    pub fn current_values(&self) -> Option<&[PeriodValue<f64>]> {
        self.current_values.as_deref()
    }
}

#[derive(Debug, Error)]
pub enum MetricSetSaveError {
    #[error("Metric error.")]
    MetricF64Error(#[from] MetricF64Error),
}

/// A set of metrics with an optional aggregator
#[derive(Clone, Debug)]
pub struct MetricSet {
    name: String,
    aggregator: Option<Aggregator>,
    metrics: Vec<OutputMetric>,
}

impl MetricSet {
    /// The name of the [`MetricSet`].
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn metrics(&self) -> &[OutputMetric] {
        &self.metrics
    }

    /// Setup a new [`MetricSetState`] for this [`MetricSet`].
    pub fn setup(&self) -> MetricSetState {
        MetricSetState {
            current_values: None,
            aggregation_states: self
                .aggregator
                .as_ref()
                .map(|a| self.metrics.iter().map(|_| a.setup()).collect()),
        }
    }

    pub fn save(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut MetricSetState,
    ) -> Result<(), MetricSetSaveError> {
        // Combine all the values for metric across all of the scenarios
        let values: Vec<PeriodValue<f64>> = self
            .metrics
            .iter()
            .map(|metric| {
                let value = metric.get_value(network, state)?;
                Ok::<PeriodValue<f64>, MetricF64Error>(PeriodValue::new(timestep.date, timestep.duration, value))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(aggregator) = &self.aggregator {
            // Apply aggregation if required

            // TODO: make this a `PywrError`.
            let aggregation_states = internal_state
                .aggregation_states
                .as_mut()
                .expect("Aggregation state expected for metric set with aggregator!");

            // Collect any aggregated values. This will remain empty if the aggregator yields
            // no values. However, if there are values we will expect the same number of aggregated
            // values as the input values / metrics.
            let mut agg_values = Vec::with_capacity(values.len());
            // Use a for loop instead of using an iterator because we need to execute the
            // `append_value` method on all aggregators.
            for (value, current_state) in values.iter().zip(aggregation_states.iter_mut()) {
                if let Some(agg_value) = aggregator.append_value(current_state, *value) {
                    agg_values.push(agg_value);
                }
            }

            let agg_values = if agg_values.is_empty() {
                None
            } else if agg_values.len() == values.len() {
                Some(agg_values)
            } else {
                // This should never happen because the aggregator should either yield no values
                // or the same number of values as the input metrics.
                unreachable!("Some values were aggregated and some were not!");
            };

            internal_state.current_values = agg_values;
        } else {
            internal_state.current_values = Some(values);
        }

        Ok(())
    }

    pub fn finalise(&self, internal_state: &mut MetricSetState) {
        if let Some(aggregator) = &self.aggregator {
            let aggregation_states = internal_state
                .aggregation_states
                .as_mut()
                .expect("Aggregation state expected for metric set with aggregator!");

            let final_values = aggregation_states
                .iter_mut()
                .map(|current_state| aggregator.finalise(current_state))
                .collect::<Option<Vec<_>>>();

            internal_state.current_values = final_values;
        } else {
            internal_state.current_values = None;
        }
    }
}

#[derive(Error, Debug)]
pub enum MetricSetBuilderError {
    #[error("Could not resolve output f64 metric `{name}`.")]
    ResolveMetricF64Error {
        name: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("No nodes are named `{name}`.")]
    NodeFamilyNotFound { name: String },
}

/// What [`MetricSetBuilder::build`] does with a metric whose parameter does not calculate the
/// phase it asks for.
#[derive(Debug)]
enum IfNotCalculated {
    Fail,
    Drop,
}

/// An entry of a [`MetricSetBuilder`], kept in the order it was added.
#[derive(Debug)]
enum MetricSetEntry {
    Metric(UnresolvedOutputMetric, IfNotCalculated),
    /// The attributes of every node with a name, whatever its sub-name.
    NodeFamily {
        name: String,
        attributes: Vec<CoreNodeAttribute>,
    },
}

#[derive(Debug)]
pub struct MetricSetBuilder {
    name: String,
    aggregator: Option<Aggregator>,
    entries: Vec<MetricSetEntry>,
}

impl MetricSetBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            aggregator: None,
            entries: Vec::new(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn aggregator(&mut self, aggregator: Aggregator) -> &mut Self {
        self.aggregator = Some(aggregator);
        self
    }

    pub fn metric(&mut self, metric: UnresolvedOutputMetric) -> &mut Self {
        self.entries.push(MetricSetEntry::Metric(metric, IfNotCalculated::Fail));
        self
    }

    /// Add a metric that is dropped, rather than failing the build, if its parameter does not
    /// calculate the phase it asks for.
    pub fn metric_if_calculated(&mut self, metric: UnresolvedOutputMetric) -> &mut Self {
        self.entries.push(MetricSetEntry::Metric(metric, IfNotCalculated::Drop));
        self
    }

    /// Add the `attributes` of every node named `name`, whatever its sub-name, in the order the
    /// nodes were added to the network.
    ///
    /// Each node gives the attributes its type has (see [`CoreNodeAttribute::metric`]), named by
    /// the node's full name (e.g. `reservoir[spill]`), with the type `CoreNode` and the node's
    /// type as the subtype. The build fails if no node has the name.
    pub fn node_family(&mut self, name: &str, attributes: &[CoreNodeAttribute]) -> &mut Self {
        self.entries.push(MetricSetEntry::NodeFamily {
            name: name.to_string(),
            attributes: attributes.to_vec(),
        });
        self
    }

    pub fn build(self, resolution_maps: &ResolutionMaps, nodes: &[Node]) -> Result<MetricSet, MetricSetBuilderError> {
        use MetricF64ResolutionError::ParameterNotRegisteredInCorrectPhase;

        let mut metrics = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            match entry {
                MetricSetEntry::Metric(metric, if_not_calculated) => {
                    let name = metric.source.label();
                    match (metric.resolve(resolution_maps), if_not_calculated) {
                        (Ok(metric), _) => metrics.push(metric),
                        (Err(ParameterNotRegisteredInCorrectPhase { .. }), IfNotCalculated::Drop) => {}
                        (Err(source), _) => {
                            return Err(MetricSetBuilderError::ResolveMetricF64Error { name, source });
                        }
                    }
                }
                MetricSetEntry::NodeFamily { name, attributes } => {
                    let family: Vec<&Node> = nodes.iter().filter(|node| node.name() == name).collect();
                    if family.is_empty() {
                        return Err(MetricSetBuilderError::NodeFamilyNotFound { name });
                    }

                    for node in family {
                        let (node_name, sub_name) = node.full_name();
                        let unresolved = UnresolvedNode::new(node_name, sub_name);
                        let output_name = unresolved.to_string();
                        let source = MetricSource::CoreNode {
                            node: node_name.to_string(),
                            sub_name: sub_name.map(ToString::to_string),
                            node_type: node.node_type().to_string(),
                        };

                        for attribute in &attributes {
                            let Some(metric) = attribute.metric(unresolved.clone(), node.node_type()) else {
                                continue;
                            };
                            let output = UnresolvedOutputMetric::new(source.clone(), &attribute.to_string(), metric);
                            let metric = output.resolve(resolution_maps).map_err(|source| {
                                MetricSetBuilderError::ResolveMetricF64Error {
                                    name: output_name.clone(),
                                    source,
                                }
                            })?;
                            metrics.push(metric);
                        }
                    }
                }
            }
        }

        let ms = MetricSet {
            name: self.name,
            aggregator: self.aggregator,
            metrics,
        };

        Ok(ms)
    }
}

#[cfg(test)]
mod tests {
    use super::{MetricSetBuilder, MetricSource, UnresolvedOutputMetric};
    use crate::metric::UnresolvedMetricF64;
    use crate::network::NetworkBuilder;
    use crate::parameters::ConstantParameterBuilder;
    use crate::test_utils::default_domain;
    use std::collections::HashMap;

    /// Asking for a constant parameter's after value, which it does not calculate, drops a metric
    /// added by `metric_if_calculated` and fails the build for one added by `metric`.
    #[test]
    fn test_metric_if_calculated() {
        let output_metric = |metric| {
            let source = MetricSource::Parameter {
                name: "p".to_string(),
                parent: None,
                parameter_type: "Constant".to_string(),
            };
            UnresolvedOutputMetric::new(source, "value", metric)
        };
        let build = |metric_set| {
            let mut builder = NetworkBuilder::default();
            builder
                .parameters()
                .f64(Box::new(ConstantParameterBuilder::new("p".into(), 1.0)));
            builder.metric_set(metric_set);
            builder.build(&default_domain(), &HashMap::new())
        };

        let mut metric_set = MetricSetBuilder::new("metric-if-calculated");
        metric_set
            .metric(output_metric(UnresolvedMetricF64::new_parameter_before("p")))
            .metric_if_calculated(output_metric(UnresolvedMetricF64::new_parameter_after("p")));
        let (network, _) = build(metric_set).unwrap();
        assert_eq!(network.metric_sets()[0].metrics().len(), 1);

        let mut metric_set = MetricSetBuilder::new("metric");
        metric_set.metric(output_metric(UnresolvedMetricF64::new_parameter_after("p")));
        assert!(build(metric_set).is_err());
    }
    /// A node family gives each of its nodes' attributes that their type has, in the order the
    /// nodes were added, and fails the build if no node has its name.
    #[test]
    fn test_node_family() {
        use crate::node::{CoreNodeAttribute, NodeBuilder, UnresolvedNode, UnresolvedStorageInitialVolume};

        let build = |metric_set| {
            let mut builder = NetworkBuilder::default();
            let mut storage = NodeBuilder::storage("res");
            storage
                .initial_volume(UnresolvedStorageInitialVolume::Absolute(0.0))
                .max_volume(10.0.into());
            builder.node(storage);
            let mut spill = NodeBuilder::link("res");
            spill.sub_name("spill");
            builder.node(spill);
            builder.node(NodeBuilder::output("other"));
            let spill_name = UnresolvedNode::new("res", Some("spill"));
            builder.connect("res", spill_name.clone());
            builder.connect(spill_name, "other");
            builder.metric_set(metric_set);
            builder.build(&default_domain(), &HashMap::new())
        };

        let mut metric_set = MetricSetBuilder::new("family");
        metric_set.node_family(
            "res",
            &[
                CoreNodeAttribute::Volume,
                CoreNodeAttribute::MaxFlow,
                CoreNodeAttribute::Cost,
            ],
        );
        let (network, _) = build(metric_set).unwrap();

        let metrics: Vec<_> = network.metric_sets()[0]
            .metrics()
            .iter()
            .map(|m| (m.name(), m.attribute(), m.source().kind(), m.source().sub_kind()))
            .collect();
        assert_eq!(
            metrics,
            vec![
                ("res", "Volume", "CoreNode", Some("Storage")),
                ("res", "Cost", "CoreNode", Some("Storage")),
                ("res[spill]", "MaxFlow", "CoreNode", Some("Link")),
                ("res[spill]", "Cost", "CoreNode", Some("Link")),
            ]
        );
        assert_eq!(
            network.metric_sets()[0].metrics()[2].source(),
            &MetricSource::CoreNode {
                node: "res".to_string(),
                sub_name: Some("spill".to_string()),
                node_type: "Link".to_string(),
            }
        );

        let mut metric_set = MetricSetBuilder::new("missing");
        metric_set.node_family("missing", &[CoreNodeAttribute::Inflow]);
        assert!(build(metric_set).is_err());
    }
}
