use crate::agg_funcs::AggFunc;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::NamedMeta;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::metric::{EdgeReference, ParameterReferenceBuilder, ParameterReturnValue, VirtualNodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::parameters::{Parameter, PythonReturnType};
use crate::validation::MetricSetProblem;
use crate::visit::{Reference, ReferenceMut, VisitReferences};
use pywr_schema_macros::{PywrVisitAll, PywrVisitPaths, PywrVisitReferences, skip_serializing_none};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroI64;
#[cfg(feature = "core")]
use std::path::Path;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Copy,
    Clone,
    JsonSchema,
    PywrVisitPaths,
    PywrVisitReferences,
    Display,
    EnumDiscriminants,
)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(MetricAggFrequencyType))]
pub enum MetricAggFrequency {
    Monthly,
    Annual,
    Days { days: NonZeroI64 },
}

/// An attribute of a single core node, which is one of the nodes a node of the schema builds.
///
/// Flow attributes and bounds belong to input, output and link core nodes, volumes and their
/// bounds to storage core nodes; `Inflow`, `Outflow` and `Cost` to all of them. A core node
/// without the attribute gives no value for it.
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, PartialEq, Eq, Display, EnumIter,
)]
pub enum CoreNodeAttribute {
    Inflow,
    Outflow,
    /// The minimum flow constraint.
    MinFlow,
    /// The maximum flow constraint. An unconstrained core node gives the largest float.
    MaxFlow,
    Volume,
    ProportionalVolume,
    /// The minimum volume constraint.
    MinVolume,
    /// The maximum volume constraint.
    MaxVolume,
    /// The cost per unit of flow, or per unit of net increase in volume for a storage, including
    /// the costs of any virtual storage nodes.
    Cost,
}

#[cfg(feature = "core")]
impl From<CoreNodeAttribute> for pywr_core::node::CoreNodeAttribute {
    fn from(value: CoreNodeAttribute) -> Self {
        match value {
            CoreNodeAttribute::Inflow => Self::Inflow,
            CoreNodeAttribute::Outflow => Self::Outflow,
            CoreNodeAttribute::MinFlow => Self::MinFlow,
            CoreNodeAttribute::MaxFlow => Self::MaxFlow,
            CoreNodeAttribute::Volume => Self::Volume,
            CoreNodeAttribute::ProportionalVolume => Self::ProportionalVolume,
            CoreNodeAttribute::MinVolume => Self::MinVolume,
            CoreNodeAttribute::MaxVolume => Self::MaxVolume,
            CoreNodeAttribute::Cost => Self::Cost,
        }
    }
}

/// The attributes of each core node that a node builds.
///
/// This shows the parts of a node that its own attributes combine or leave out, such as the
/// spill of a reservoir or the steps of a piecewise link. Each core node gives a value named by
/// the node's name and, for all but its main core node, a sub-name in brackets (e.g.
/// `reservoir1[spill]`). These sub-names come from how a node is built, so they may change
/// between versions.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
#[serde(deny_unknown_fields)]
pub struct CoreNodesReference {
    /// The name of the node.
    pub node: String,
    /// The attributes to give for each of its core nodes.
    pub attributes: Vec<CoreNodeAttribute>,
}

/// Names a node without reading one of its attributes.
impl VisitReferences for CoreNodesReference {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        visitor(Reference::Node {
            name: &self.node,
            attribute: None,
            metric: None,
        });
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        visitor(ReferenceMut::Node(&mut self.node));
    }
}

#[cfg(feature = "core")]
impl From<MetricAggFrequency> for pywr_core::recorders::AggregationFrequency {
    fn from(value: MetricAggFrequency) -> Self {
        match value {
            MetricAggFrequency::Monthly => pywr_core::recorders::AggregationFrequency::Monthly,
            MetricAggFrequency::Annual => pywr_core::recorders::AggregationFrequency::Annual,
            MetricAggFrequency::Days { days } => pywr_core::recorders::AggregationFrequency::Days(days),
        }
    }
}

/// A set of metrics that can be output from a model run.
///
/// A metric set can optionally have an aggregator, which will apply an aggregation function
/// over the metrics in the set. If an aggregation frequency is provided then the aggregation
/// will be performed over each period implied by that frequency. For example, if the frequency
/// is monthly then the aggregation will be performed over each month in the model run.
///
/// If the metric set has a child aggregator then the aggregation will be performed over the
/// aggregated values of the child aggregator.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitPaths, PywrVisitReferences)]
#[serde(deny_unknown_fields)]
pub struct MetricAggregator {
    /// Optional aggregation frequency.
    pub freq: Option<MetricAggFrequency>,
    /// Aggregation function to apply over metric values.
    pub func: AggFunc,
    /// Optional child aggregator.
    pub child: Option<Box<MetricAggregator>>,
}

#[cfg(feature = "core")]
impl MetricAggregator {
    fn load(&self, data_path: Option<&Path>) -> Result<pywr_core::recorders::Aggregator, SchemaError> {
        let child = self.child.as_ref().map(|a| a.load(data_path)).transpose()?;

        Ok(pywr_core::recorders::Aggregator::new(
            self.freq.map(|p| p.into()),
            self.func.load(data_path)?,
            child,
        ))
    }
}

/// Which attributes a filter adds for each node it selects.
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Copy, Clone, JsonSchema, PywrVisitAll, PartialEq, Eq, Display, EnumIter,
)]
pub enum FilterAttributes {
    /// The node's default attribute.
    Default,
    /// Every attribute the node has, such as its flows, bounds and cost. An attribute that
    /// requires an optional field, such as `MaxFlow` requiring `max_flow`, is only added when that
    /// field is set.
    All,
}

/// Filters that allow multiple metrics to be added to a metric set.
///
/// The filters allow metrics for all nodes, virtual nodes, parameters and/or edges in a model to
/// be added to a metric set. The node filters add either the default attribute or every attribute
/// of each node (see [`FilterAttributes`]). A parameter adds each value it calculates, before
/// and/or after.
///
/// For example, `{"all_nodes": "All", "all_virtual_nodes": "Default"}` adds every attribute of
/// every node and the default attribute of every virtual node.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, Default, PywrVisitPaths, PywrVisitReferences)]
#[serde(deny_unknown_fields)]
pub struct MetricSetFilters {
    /// The attributes to add for every node, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_nodes: Option<FilterAttributes>,
    /// The attributes to add for every virtual node, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_virtual_nodes: Option<FilterAttributes>,
    #[serde(default)]
    pub all_parameters: bool,
    #[serde(default)]
    pub all_edges: bool,
    /// The attributes to give for every core node of every node. See [`CoreNodesReference`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_core_nodes: Vec<CoreNodeAttribute>,
}

#[cfg(feature = "core")]
impl MetricSetFilters {
    fn create_metrics(&self, args: &LoadArgs) -> Vec<Metric> {
        use crate::metric::NodeAttrReference;

        let mut metrics = vec![];

        if let Some(selection) = self.all_nodes {
            for node in args.schema.nodes.iter() {
                let attributes = match selection {
                    FilterAttributes::Default => vec![None],
                    FilterAttributes::All => node.attributes().into_iter().map(Some).collect(),
                };
                for attribute in attributes {
                    metrics.push(Metric::Node(NodeAttrReference::new(node.name().to_string(), attribute)));
                }
            }
        }

        if let Some(selection) = self.all_virtual_nodes
            && let Some(virtual_nodes) = args.schema.virtual_nodes.as_ref()
        {
            for node in virtual_nodes.iter() {
                let attributes = match selection {
                    FilterAttributes::Default => vec![None],
                    FilterAttributes::All => node.attributes().into_iter().map(Some).collect(),
                };
                for attribute in attributes {
                    metrics.push(Metric::VirtualNode(VirtualNodeAttrReference::new(
                        node.name().to_string(),
                        attribute,
                    )));
                }
            }
        }

        if self.all_parameters
            && let Some(parameters) = args.schema.parameters.as_ref()
        {
            for parameter in parameters.iter() {
                // Skip Python parameters that return multiple values as the type or keys of these values is not
                // known at this point.
                if let Parameter::Python(param) = parameter
                    && matches!(param.return_type, PythonReturnType::Dict)
                {
                    continue;
                }

                // Which phases a parameter calculates is only known once it is built (e.g.
                // from a Python class's methods), so ask for both.
                for return_value in [ParameterReturnValue::Before, ParameterReturnValue::After] {
                    let mut p_ref_builder = ParameterReferenceBuilder::new(parameter.name());
                    p_ref_builder.return_value(return_value);
                    metrics.push(Metric::Parameter(p_ref_builder.build()));
                }
            }
        }

        if self.all_edges {
            for edge in args.schema.edges.iter() {
                metrics.push(Metric::Edge(EdgeReference { edge: edge.clone() }));
            }
        }

        metrics
    }
}

/// A set of metrics that can be output from a model run.
///
/// A metric set can optionally have an aggregator, which will apply an aggregation function
/// over metrics set. If the aggregator has a defined frequency then the aggregation will result
/// in multiple values (i.e. per each period implied by the frequency).
///
/// Metrics added by the filters will be appended to any metrics specified for the metric attribute,
/// if they are not a duplication.
#[skip_serializing_none]
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema, PywrVisitPaths, PywrVisitReferences)]
#[serde(deny_unknown_fields)]
pub struct MetricSet {
    pub meta: NamedMeta,
    pub metrics: Option<Vec<Metric>>,
    /// The attributes of each core node of nodes, given after `metrics`.
    pub core_nodes: Option<Vec<CoreNodesReference>>,
    pub aggregator: Option<MetricAggregator>,
    #[serde(default)]
    pub filters: MetricSetFilters,
}

impl MetricSet {
    pub fn name(&self) -> &str {
        &self.meta.name
    }

    pub fn meta(&self) -> &NamedMeta {
        &self.meta
    }

    pub fn meta_mut(&mut self) -> &mut NamedMeta {
        &mut self.meta
    }

    /// Check the set's own fields and return every problem found. Whether it would be empty is
    /// not checked, since that depends on the network its filters read.
    pub fn validate(&self) -> Result<(), Vec<MetricSetProblem>> {
        let problems: Vec<MetricSetProblem> = self
            .metrics
            .iter()
            .flatten()
            .enumerate()
            .filter(|(_, metric)| matches!(metric, Metric::Literal { .. }))
            .map(|(index, _)| MetricSetProblem::Literal { index })
            .collect();

        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }

    #[cfg(feature = "core")]
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Create metrics from filters and load them as output metrics
        let metrics_from_filters = self
            .filters
            .create_metrics(args)
            .iter()
            .map(|m| m.load_as_output(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let named_metrics = self
            .metrics
            .iter()
            .flatten()
            .map(|m| m.load_as_output(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let metrics_from_filters: Vec<_> = metrics_from_filters
            .into_iter()
            .filter(|m| !named_metrics.contains(m))
            .collect();

        // Every node's core nodes from the filter, after any named explicitly.
        let mut core_nodes: Vec<(&str, Vec<pywr_core::node::CoreNodeAttribute>)> = self
            .core_nodes
            .iter()
            .flatten()
            .map(|r| (r.node.as_str(), r.attributes.iter().map(|&a| a.into()).collect()))
            .collect();

        if !self.filters.all_core_nodes.is_empty() {
            let attributes: Vec<_> = self.filters.all_core_nodes.iter().map(|&a| a.into()).collect();
            for node in args.schema.nodes.iter().filter(|node| !node.is_placeholder()) {
                if !core_nodes.iter().any(|(name, _)| *name == node.name()) {
                    core_nodes.push((node.name(), attributes.clone()));
                }
            }
        }

        for (name, _) in &core_nodes {
            if args.schema.get_node_by_name(name).is_none() {
                return Err(SchemaError::NodeNotFound { name: name.to_string() });
            }
        }

        if named_metrics.is_empty() && metrics_from_filters.is_empty() && core_nodes.is_empty() {
            return Err(SchemaError::EmptyMetricSet(self.name().to_string()));
        }

        let mut metric_set = pywr_core::recorders::MetricSetBuilder::new(self.name());

        if let Some(aggregator) = &self.aggregator {
            metric_set.aggregator(aggregator.load(args.data_path)?);
        }

        for m in named_metrics {
            metric_set.metric(m);
        }

        // A filter's metric may ask for a phase its parameter does not calculate.
        for m in metrics_from_filters {
            metric_set.metric_if_calculated(m);
        }

        for (name, attributes) in core_nodes {
            metric_set.node_family(name, &attributes);
        }

        network.metric_set(metric_set);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MetricSet;
    use crate::validation::MetricSetProblem::Literal;
    use serde_json::json;

    /// [`MetricSet::validate`] should refuse each literal in `metrics`, by its index.
    #[test]
    fn test_validate_refuses_each_literal() {
        let metric_set = |metrics: serde_json::Value| -> MetricSet {
            serde_json::from_value(json!({ "meta": { "name": "outputs" }, "metrics": metrics })).unwrap()
        };
        let literal = json!({ "type": "Literal", "value": 1.0 });
        let node = json!({ "type": "Node", "name": "supply" });

        assert_eq!(
            metric_set(json!([node, literal, node, literal])).validate(),
            Err(vec![Literal { index: 1 }, Literal { index: 3 }])
        );
        assert_eq!(metric_set(json!([node])).validate(), Ok(()));
    }
}
