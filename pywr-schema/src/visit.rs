use crate::edge::Edge;
use crate::metric::{IndexMetric, Metric};
use std::collections::HashMap;
use std::num::{NonZeroI64, NonZeroUsize};
use std::path::{Path, PathBuf};

/// A trait for recursively visiting [`Metric`] in a schema.
///
/// This trait is used to visit all the metrics in a schema. This is useful for search for
/// specific metrics, parameters, or other values in a schema.
///
/// This trait is implemented for all the types that can be used in a schema. Additional
/// implementations can be added as needed.
pub trait VisitMetrics {
    fn visit_metrics<F: FnMut(&Metric)>(&self, _visitor: &mut F) {}

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, _visitor: &mut F) {}
}

impl VisitMetrics for Metric {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        visitor(self);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        visitor(self);
    }
}

impl VisitMetrics for IndexMetric {
    fn visit_metrics<F: FnMut(&Metric)>(&self, _visitor: &mut F) {}

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, _visitor: &mut F) {}
}

impl<T> VisitMetrics for Option<T>
where
    T: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_metrics(visitor);
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_metrics_mut(visitor);
        }
    }
}

impl<T> VisitMetrics for Vec<T>
where
    T: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_metrics(visitor);
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_metrics_mut(visitor);
        }
    }
}

impl<A, B> VisitMetrics for (A, B)
where
    A: VisitMetrics,
    B: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        self.0.visit_metrics(visitor);
        self.1.visit_metrics(visitor);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        self.0.visit_metrics_mut(visitor);
        self.1.visit_metrics_mut(visitor);
    }
}

/// Visit all the metrics in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitMetrics for HashMap<K, V>
where
    V: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_metrics(visitor);
        }
    }

    /// Mutably visit all the paths in the map.
    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_metrics_mut(visitor);
        }
    }
}

impl VisitMetrics for u8 {}
impl VisitMetrics for i8 {}
impl VisitMetrics for u16 {}
impl VisitMetrics for i16 {}
impl VisitMetrics for u32 {}
impl VisitMetrics for i32 {}

impl VisitMetrics for f32 {}
impl VisitMetrics for f64 {}
impl<const N: usize> VisitMetrics for [f64; N] {}
impl<const N: usize> VisitMetrics for [Metric; N] {}
impl VisitMetrics for bool {}
impl VisitMetrics for u64 {}
impl VisitMetrics for String {}
impl VisitMetrics for PathBuf {}
impl VisitMetrics for NonZeroUsize {}

impl VisitMetrics for serde_json::Value {}

/// A trait for recursively visiting paths in a schema.
///
/// This trait is used to visit all the paths in a schema. This is useful for finding
/// all the external files that need to be loaded.
///
/// This trait is implemented for all the types that can be used in a schema. Additional
/// implementations can be added as needed.
pub trait VisitPaths {
    fn visit_paths<F: FnMut(&Path)>(&self, _visitor: &mut F) {}

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, _visitor: &mut F) {}
}

impl VisitPaths for Metric {}
impl VisitPaths for IndexMetric {}

impl<T> VisitPaths for Option<T>
where
    T: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_paths(visitor);
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_paths_mut(visitor);
        }
    }
}

impl<T> VisitPaths for Vec<T>
where
    T: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_paths(visitor);
        }
    }

    /// Visit all the paths in the vector.
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_paths_mut(visitor);
        }
    }
}

/// Visit all the paths in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitPaths for HashMap<K, V>
where
    V: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_paths(visitor);
        }
    }

    /// Mutably visit all the paths in the map.
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_paths_mut(visitor);
        }
    }
}

impl<A, B> VisitPaths for (A, B)
where
    A: VisitPaths,
    B: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        self.0.visit_paths(visitor);
        self.1.visit_paths(visitor);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        self.0.visit_paths_mut(visitor);
        self.1.visit_paths_mut(visitor);
    }
}

impl VisitPaths for u8 {}
impl VisitPaths for i8 {}
impl VisitPaths for u16 {}
impl VisitPaths for i16 {}
impl VisitPaths for u32 {}
impl VisitPaths for i32 {}

impl VisitPaths for f32 {}
impl VisitPaths for f64 {}
impl<const N: usize> VisitPaths for [f64; N] {}
impl<const N: usize> VisitPaths for [Metric; N] {}
impl VisitPaths for bool {}
impl VisitPaths for u64 {}
impl VisitPaths for String {}
impl VisitPaths for PathBuf {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(self.as_path());
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(self);
    }
}
impl VisitPaths for NonZeroUsize {}

impl VisitPaths for serde_json::Value {}

/// A reference to a schema component by name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reference<'a> {
    /// Resolved in the network's `nodes`.
    Node(&'a str),
    /// Resolved in the network's `virtual_nodes`.
    VirtualNode(&'a str),
    /// Resolved in the network's `edges`, on all four fields: two edges can share endpoints and
    /// differ only in slot. The endpoints are also visited as [`Reference::Node`].
    Edge(&'a Edge),
    /// Resolved in the network's `parameters`.
    Parameter(&'a str),
    /// Resolved in the owning node's or virtual node's own `parameters`, so the name is
    /// meaningless without the [`Owner`].
    LocalParameter(&'a str),
    /// Resolved in the network's `tables`.
    Table(&'a str),
    /// Resolved in the network's `timeseries`.
    Timeseries(&'a str),
    /// Resolved in the network's `metric_sets`. Only an output names one.
    MetricSet(&'a str),
}

/// The mutable form of [`Reference`], with variants corresponding one-for-one.
#[derive(Debug, PartialEq)]
pub enum ReferenceMut<'a> {
    Node(&'a mut String),
    VirtualNode(&'a mut String),
    Edge(&'a mut Edge),
    Parameter(&'a mut String),
    LocalParameter(&'a mut String),
    Table(&'a mut String),
    Timeseries(&'a mut String),
    MetricSet(&'a mut String),
}

/// The top-level component holding a [`Reference`], however deeply nested the reference is. A
/// reference inside a node's local parameter is owned by the node, which is the scope a
/// [`Reference::LocalParameter`] resolves in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Owner<'a> {
    Node(&'a str),
    VirtualNode(&'a str),
    Edge(&'a Edge),
    Parameter(&'a str),
    MetricSet(&'a str),
    Output(&'a str),
}

/// A trait for recursively visiting every reference a schema component makes by name.
///
/// It reaches what [`VisitMetrics`] cannot, since an [`IndexMetric`] is not a [`Metric`] and its
/// `VisitMetrics` impl is empty.
///
/// It does not yield the name a component gives itself, which is a definition rather than a
/// reference, nor an inter-network transfer, which resolves against a multi-network model.
///
/// For the element holding each reference, use
/// [`NetworkSchema::visit_owned_references`](crate::NetworkSchema::visit_owned_references).
pub trait VisitReferences {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, _visitor: &mut F) {}

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, _visitor: &mut F) {}
}

impl VisitReferences for Metric {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            Metric::Node(node_ref) => node_ref.visit_references(visitor),
            Metric::VirtualNode(node_ref) => node_ref.visit_references(visitor),
            Metric::Edge(edge_ref) => edge_ref.visit_references(visitor),
            Metric::Table(table_ref) => table_ref.visit_references(visitor),
            Metric::Timeseries(ts_ref) => ts_ref.visit_references(visitor),
            Metric::Parameter(p_ref) => visitor(Reference::Parameter(&p_ref.name)),
            Metric::LocalParameter(p_ref) => visitor(Reference::LocalParameter(&p_ref.name)),
            // An inter-network transfer resolves against the multi-network model, not this one.
            Metric::Literal { .. } | Metric::InterNetworkTransfer { .. } => {}
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            Metric::Node(node_ref) => node_ref.visit_references_mut(visitor),
            Metric::VirtualNode(node_ref) => node_ref.visit_references_mut(visitor),
            Metric::Edge(edge_ref) => edge_ref.visit_references_mut(visitor),
            Metric::Table(table_ref) => table_ref.visit_references_mut(visitor),
            Metric::Timeseries(ts_ref) => ts_ref.visit_references_mut(visitor),
            Metric::Parameter(p_ref) => visitor(ReferenceMut::Parameter(&mut p_ref.name)),
            Metric::LocalParameter(p_ref) => visitor(ReferenceMut::LocalParameter(&mut p_ref.name)),
            Metric::Literal { .. } | Metric::InterNetworkTransfer { .. } => {}
        }
    }
}

impl VisitReferences for IndexMetric {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            IndexMetric::Node(node_ref) => node_ref.visit_references(visitor),
            IndexMetric::Table(table_ref) => table_ref.visit_references(visitor),
            IndexMetric::Timeseries(ts_ref) => ts_ref.visit_references(visitor),
            IndexMetric::Parameter(p_ref) => visitor(Reference::Parameter(&p_ref.name)),
            IndexMetric::LocalParameter(p_ref) => visitor(Reference::LocalParameter(&p_ref.name)),
            IndexMetric::Constant { .. } | IndexMetric::InterNetworkTransfer { .. } => {}
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            IndexMetric::Node(node_ref) => node_ref.visit_references_mut(visitor),
            IndexMetric::Table(table_ref) => table_ref.visit_references_mut(visitor),
            IndexMetric::Timeseries(ts_ref) => ts_ref.visit_references_mut(visitor),
            IndexMetric::Parameter(p_ref) => visitor(ReferenceMut::Parameter(&mut p_ref.name)),
            IndexMetric::LocalParameter(p_ref) => visitor(ReferenceMut::LocalParameter(&mut p_ref.name)),
            IndexMetric::Constant { .. } | IndexMetric::InterNetworkTransfer { .. } => {}
        }
    }
}

impl<T> VisitReferences for Option<T>
where
    T: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_references(visitor);
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_references_mut(visitor);
        }
    }
}

impl<T> VisitReferences for Vec<T>
where
    T: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_references(visitor);
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_references_mut(visitor);
        }
    }
}

/// Visit all the references in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitReferences for HashMap<K, V>
where
    V: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_references(visitor);
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_references_mut(visitor);
        }
    }
}

impl<A, B> VisitReferences for (A, B)
where
    A: VisitReferences,
    B: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        self.0.visit_references(visitor);
        self.1.visit_references(visitor);
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        self.0.visit_references_mut(visitor);
        self.1.visit_references_mut(visitor);
    }
}

impl<T> VisitReferences for Box<T>
where
    T: VisitReferences,
{
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        self.as_ref().visit_references(visitor);
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        self.as_mut().visit_references_mut(visitor);
    }
}

impl VisitReferences for u8 {}
impl VisitReferences for i8 {}
impl VisitReferences for u16 {}
impl VisitReferences for i16 {}
impl VisitReferences for u32 {}
impl VisitReferences for i32 {}

impl VisitReferences for f32 {}
impl VisitReferences for f64 {}
impl<const N: usize> VisitReferences for [f64; N] {}
impl<const N: usize> VisitReferences for [Metric; N] {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_references(visitor);
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_references_mut(visitor);
        }
    }
}
impl VisitReferences for bool {}
impl VisitReferences for u64 {}
/// A plain string is not a reference; only the reference types are.
impl VisitReferences for String {}
impl VisitReferences for PathBuf {}
impl VisitReferences for NonZeroUsize {}
impl VisitReferences for NonZeroI64 {}

impl VisitReferences for serde_json::Value {}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::network::NetworkSchema;
    use crate::nodes::VirtualNode;
    use crate::visit::{Owner, Reference, ReferenceMut, VisitMetrics, VisitReferences};
    use std::str::FromStr;

    /// A network containing a metric in every location a metric can appear.
    const NETWORK_WITH_METRICS: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply1" },
                "type": "Input",
                "parameters": [
                    {
                        "meta": { "name": "supply1-local" },
                        "type": "Negative",
                        "phase": "Before",
                        "parameter": { "type": "Parameter", "name": "node-local-parameter" }
                    }
                ],
                "max_flow": { "type": "Parameter", "name": "node-max-flow" }
            },
            {
                "meta": { "name": "reservoir1" },
                "type": "Storage",
                "max_volume": { "type": "Parameter", "name": "storage-node-max-volume" },
                "initial_volume": { "type": "Proportional", "proportion": 0.0 }
            },
            {
                "meta": { "name": "demand1" },
                "type": "Output"
            }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "VirtualStorage",
                "nodes": [{ "name": "supply1" }],
                "parameters": [
                    {
                        "meta": { "name": "licence-local" },
                        "type": "Negative",
                        "phase": "Before",
                        "parameter": { "type": "Parameter", "name": "virtual-storage-local-parameter" }
                    }
                ],
                "max_volume": { "type": "Parameter", "name": "virtual-storage-max-volume" },
                "min_volume": { "type": "Parameter", "name": "virtual-storage-min-volume" },
                "cost": { "type": "Parameter", "name": "virtual-storage-cost" },
                "initial_volume": { "type": "Proportional", "proportion": 0.0 }
            },
            {
                "meta": { "name": "agg" },
                "type": "Aggregated",
                "nodes": [{ "name": "supply1" }, { "name": "demand1" }],
                "max_flow": { "type": "Parameter", "name": "aggregated-max-flow" },
                "min_flow": { "type": "Parameter", "name": "aggregated-min-flow" },
                "relationship": {
                    "type": "Coefficients",
                    "factors": [
                        { "type": "Parameter", "name": "aggregated-relationship-factor-1" },
                        { "type": "Parameter", "name": "aggregated-relationship-factor-2" }
                    ],
                    "rhs": { "type": "Parameter", "name": "aggregated-relationship-rhs" }
                }
            },
            {
                "meta": { "name": "agg-storage" },
                "type": "AggregatedStorage",
                "storage_nodes": [{ "name": "reservoir1" }],
                "parameters": [
                    {
                        "meta": { "name": "agg-storage-local" },
                        "type": "Negative",
                        "phase": "Before",
                        "parameter": { "type": "Parameter", "name": "aggregated-storage-local-parameter" }
                    }
                ]
            }
        ],
        "edges": [
            { "from_node": "supply1", "to_node": "reservoir1" },
            { "from_node": "reservoir1", "to_node": "demand1" }
        ],
        "parameters": [
            {
                "meta": { "name": "demand" },
                "type": "Negative",
                "phase": "Before",
                "parameter": { "type": "Parameter", "name": "global-parameter" }
            }
        ],
        "metric_sets": [
            {
                "name": "ms1",
                "metrics": [{ "type": "Parameter", "name": "metric-set-metric" }]
            }
        ]
    }
    "#;

    /// Every location holding a metric in [`NETWORK_WITH_METRICS`], sorted.
    const EXPECTED_LOCATIONS: [&str; 15] = [
        "aggregated-max-flow",
        "aggregated-min-flow",
        "aggregated-relationship-factor-1",
        "aggregated-relationship-factor-2",
        "aggregated-relationship-rhs",
        "aggregated-storage-local-parameter",
        "global-parameter",
        "metric-set-metric",
        "node-local-parameter",
        "node-max-flow",
        "storage-node-max-volume",
        "virtual-storage-cost",
        "virtual-storage-local-parameter",
        "virtual-storage-max-volume",
        "virtual-storage-min-volume",
    ];

    /// The location a metric of [`NETWORK_WITH_METRICS`] appears in.
    fn location(metric: &Metric) -> String {
        match metric {
            Metric::Parameter(reference) => reference.name.clone(),
            _ => panic!("Unexpected metric in the fixture: {metric:?}"),
        }
    }

    /// Collect the location of every visited metric, sorted, so that the assertions do not depend
    /// on the order in which the schema happens to be walked.
    fn collect_metrics(network: &NetworkSchema) -> Vec<String> {
        let mut locations = Vec::new();
        network.visit_metrics(&mut |metric| locations.push(location(metric)));
        locations.sort();
        locations
    }

    /// As [`collect_metrics`], but using the mutable visitor.
    fn collect_metrics_mut(network: &mut NetworkSchema) -> Vec<String> {
        let mut locations = Vec::new();
        network.visit_metrics_mut(&mut |metric| locations.push(location(metric)));
        locations.sort();
        locations
    }

    /// Every location a metric can appear should be reachable from the visitor.
    #[test]
    fn test_visit_metrics_reaches_every_metric() {
        let network = NetworkSchema::from_str(NETWORK_WITH_METRICS).unwrap();

        assert_eq!(collect_metrics(&network), EXPECTED_LOCATIONS);
    }

    /// The mutable visitor should visit the same metrics.
    #[test]
    fn test_visit_metrics_mut_reaches_every_metric() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_METRICS).unwrap();

        assert_eq!(collect_metrics_mut(&mut network), EXPECTED_LOCATIONS);
    }

    /// The mutable visitor should hand out references into the schema, so that a metric it
    /// rewrites is replaced in the network itself.
    #[test]
    fn test_visit_metrics_mut_rewrites_every_metric() {
        const NEW_METRIC: Metric = Metric::Literal { value: 42.0 };

        let mut network = NetworkSchema::from_str(NETWORK_WITH_METRICS).unwrap();

        network.visit_metrics_mut(&mut |metric| *metric = NEW_METRIC);

        // Any location left un-rewritten is one the mutable visitor failed to reach.
        let mut count = 0;
        network.visit_metrics(&mut |metric| {
            assert_eq!(metric, &NEW_METRIC);
            count += 1;
        });
        assert_eq!(count, EXPECTED_LOCATIONS.len());

        // Check a rewritten metric directly, rather than through the visitor being tested.
        let virtual_node = network.get_virtual_node_by_name("licence").unwrap();
        match virtual_node {
            VirtualNode::VirtualStorage(n) => assert_eq!(n.max_volume, Some(NEW_METRIC)),
            _ => panic!("Expected a VirtualStorage node"),
        }
    }

    /// A node reference in every location a node name can appear, each named for its location.
    ///
    /// This covers the places [`NETWORK_WITH_REFERENCES`] does not: a virtual storage's and an
    /// aggregated storage's node lists, and a node metric on a storage node.
    const NETWORK_WITH_NODE_REFERENCES: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "target" },
                "type": "Storage",
                "max_volume": { "type": "Node", "name": "node-metric" },
                "initial_volume": { "type": "Proportional", "proportion": 0.0 }
            },
            {
                "meta": { "name": "downstream" },
                "type": "Output"
            }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "VirtualStorage",
                "nodes": [{ "name": "virtual-storage-nodes" }],
                "initial_volume": { "type": "Proportional", "proportion": 0.0 }
            },
            {
                "meta": { "name": "agg" },
                "type": "Aggregated",
                "nodes": [{ "name": "aggregated-nodes" }]
            },
            {
                "meta": { "name": "agg-storage" },
                "type": "AggregatedStorage",
                "storage_nodes": [{ "name": "aggregated-storage-nodes" }]
            }
        ],
        "edges": [
            { "from_node": "edge-from", "to_node": "edge-to" }
        ],
        "parameters": [
            {
                "meta": { "name": "p1" },
                "type": "IndexedArray",
                "metrics": [
                    { "type": "VirtualNode", "name": "virtual-node-metric" },
                    { "type": "Edge", "edge": { "from_node": "metric-edge-from", "to_node": "metric-edge-to" } }
                ],
                "index_parameter": { "type": "Node", "name": "index-metric" }
            }
        ],
        "metric_sets": [
            {
                "name": "ms1",
                "metrics": [{ "type": "Node", "name": "metric-set-metric" }]
            }
        ]
    }
    "#;

    /// Every location a node name can appear should be reachable from the visitor.
    ///
    /// Note the node lists of the virtual nodes hold [`crate::metric::NodeComponentReference`],
    /// so they yield [`Reference::Node`] even though they are reached through a virtual node.
    #[test]
    fn test_visit_references_reaches_every_node_location() {
        let network = NetworkSchema::from_str(NETWORK_WITH_NODE_REFERENCES).unwrap();

        assert_eq!(
            collect_references(&network),
            [
                "Edge:metric-edge-from->metric-edge-to",
                "Node:aggregated-nodes",
                "Node:aggregated-storage-nodes",
                "Node:edge-from",
                "Node:edge-to",
                "Node:index-metric",
                "Node:metric-edge-from",
                "Node:metric-edge-to",
                "Node:metric-set-metric",
                "Node:node-metric",
                "Node:virtual-storage-nodes",
                "VirtualNode:virtual-node-metric",
            ]
        );
    }

    /// A reference of every kind, each named for its location so a missed one is identifiable.
    /// Several sit behind an [`IndexMetric`], which no other visitor reaches.
    const NETWORK_WITH_REFERENCES: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "parameters": [
                    {
                        "meta": { "name": "supply-local" },
                        "type": "Negative",
                        "phase": "Before",
                        "parameter": { "type": "Timeseries", "name": "local-parameter-timeseries" }
                    }
                ],
                "max_flow": { "type": "Parameter", "name": "node-parameter" },
                "min_flow": { "type": "LocalParameter", "name": "node-local-parameter" },
                "cost": { "type": "Table", "table": "node-table" }
            },
            {
                "meta": { "name": "demand" },
                "type": "Output"
            }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "Aggregated",
                "parameters": [
                    {
                        "meta": { "name": "licence-local" },
                        "type": "Negative",
                        "phase": "Before",
                        "parameter": { "type": "Parameter", "name": "virtual-node-local-parameter" }
                    }
                ],
                "nodes": [{ "name": "aggregated-node-component" }],
                "max_flow": { "type": "VirtualNode", "name": "virtual-node-metric" }
            }
        ],
        "edges": [
            { "from_node": "edge-from", "to_node": "edge-to" }
        ],
        "parameters": [
            {
                "meta": { "name": "index-holder" },
                "type": "IndexedArray",
                "metrics": [
                    { "type": "Edge", "edge": { "from_node": "metric-edge-from", "to_node": "metric-edge-to" } }
                ],
                "index_parameter": { "type": "Parameter", "name": "index-metric-parameter" }
            },
            {
                "meta": { "name": "index-agg" },
                "type": "AggregatedIndex",
                "phase": "Before",
                "agg_func": { "type": "Sum" },
                "metrics": [
                    { "type": "LocalParameter", "name": "index-metric-local-parameter" },
                    { "type": "Table", "table": "index-metric-table" },
                    { "type": "Timeseries", "name": "index-metric-timeseries" },
                    { "type": "Node", "name": "index-metric-node" }
                ]
            },
            {
                "meta": { "name": "constant-from-table" },
                "type": "Constant",
                "value": { "type": "Table", "table": "constant-value-table" }
            }
        ],
        "metric_sets": [
            {
                "name": "ms1",
                "metrics": [{ "type": "Parameter", "name": "metric-set-parameter" }]
            }
        ],
        "outputs": [
            { "name": "csv-out", "type": "CSV", "format": "Long", "filename": "out.csv",
              "metric_set": ["csv-output-metric-set-1", "csv-output-metric-set-2"] },
            { "name": "hdf-out", "type": "HDF5", "filename": "out.h5", "metric_set": "hdf5-output-metric-set" },
            { "name": "memory-out", "type": "Memory", "metric_set": "memory-output-metric-set" }
        ]
    }
    "#;

    /// Every reference in [`NETWORK_WITH_REFERENCES`], sorted. Definitions are absent: the metric
    /// set `ms1` is defined but never named, and the `edges` entry contributes only its endpoints.
    const EXPECTED_REFERENCES: [&str; 23] = [
        "Edge:metric-edge-from->metric-edge-to",
        "LocalParameter:index-metric-local-parameter",
        "LocalParameter:node-local-parameter",
        "MetricSet:csv-output-metric-set-1",
        "MetricSet:csv-output-metric-set-2",
        "MetricSet:hdf5-output-metric-set",
        "MetricSet:memory-output-metric-set",
        "Node:aggregated-node-component",
        "Node:edge-from",
        "Node:edge-to",
        "Node:index-metric-node",
        "Node:metric-edge-from",
        "Node:metric-edge-to",
        "Parameter:index-metric-parameter",
        "Parameter:metric-set-parameter",
        "Parameter:node-parameter",
        "Parameter:virtual-node-local-parameter",
        "Table:constant-value-table",
        "Table:index-metric-table",
        "Table:node-table",
        "Timeseries:index-metric-timeseries",
        "Timeseries:local-parameter-timeseries",
        "VirtualNode:virtual-node-metric",
    ];

    /// Render a reference as "Kind:name", so a failure names the kind as well as the location.
    fn describe(reference: Reference<'_>) -> String {
        match reference {
            Reference::Node(name) => format!("Node:{name}"),
            Reference::VirtualNode(name) => format!("VirtualNode:{name}"),
            Reference::Edge(edge) => format!("Edge:{edge}"),
            Reference::Parameter(name) => format!("Parameter:{name}"),
            Reference::LocalParameter(name) => format!("LocalParameter:{name}"),
            Reference::Table(name) => format!("Table:{name}"),
            Reference::Timeseries(name) => format!("Timeseries:{name}"),
            Reference::MetricSet(name) => format!("MetricSet:{name}"),
        }
    }

    /// As [`describe`], for the mutable form.
    fn describe_mut(reference: &ReferenceMut<'_>) -> String {
        match reference {
            ReferenceMut::Node(name) => format!("Node:{name}"),
            ReferenceMut::VirtualNode(name) => format!("VirtualNode:{name}"),
            ReferenceMut::Edge(edge) => format!("Edge:{edge}"),
            ReferenceMut::Parameter(name) => format!("Parameter:{name}"),
            ReferenceMut::LocalParameter(name) => format!("LocalParameter:{name}"),
            ReferenceMut::Table(name) => format!("Table:{name}"),
            ReferenceMut::Timeseries(name) => format!("Timeseries:{name}"),
            ReferenceMut::MetricSet(name) => format!("MetricSet:{name}"),
        }
    }

    fn describe_owner(owner: Owner<'_>) -> String {
        match owner {
            Owner::Node(name) => format!("node \"{name}\""),
            Owner::VirtualNode(name) => format!("virtual node \"{name}\""),
            Owner::Edge(edge) => format!("edge \"{edge}\""),
            Owner::Parameter(name) => format!("parameter \"{name}\""),
            Owner::MetricSet(name) => format!("metric set \"{name}\""),
            Owner::Output(name) => format!("output \"{name}\""),
        }
    }

    /// Collect every visited reference, sorted, so assertions do not depend on the walk order.
    fn collect_references(network: &NetworkSchema) -> Vec<String> {
        let mut refs = Vec::new();
        network.visit_references(&mut |reference| refs.push(describe(reference)));
        refs.sort();
        refs
    }

    /// As [`collect_references`], but through the owner-aware walk, tagging each hit.
    fn collect_owned_references(network: &NetworkSchema) -> Vec<String> {
        let mut refs = Vec::new();
        network.visit_owned_references(&mut |owner, reference| {
            refs.push(format!("{}: {}", describe_owner(owner), describe(reference)))
        });
        refs.sort();
        refs
    }

    /// Every location a reference can appear should be reachable from the visitor.
    #[test]
    fn test_visit_references_reaches_every_reference() {
        let network = NetworkSchema::from_str(NETWORK_WITH_REFERENCES).unwrap();

        assert_eq!(collect_references(&network), EXPECTED_REFERENCES);
    }

    /// The mutable visitor should reach every reference with the same kind as the immutable one,
    /// and hand out borrows into the schema, so a name it rewrites is replaced in the network.
    #[test]
    fn test_visit_references_mut_reaches_and_rewrites_every_reference() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_REFERENCES).unwrap();

        let mut seen = Vec::new();
        network.visit_references_mut(&mut |reference| {
            seen.push(describe_mut(&reference));
            match reference {
                ReferenceMut::Node(name)
                | ReferenceMut::VirtualNode(name)
                | ReferenceMut::Parameter(name)
                | ReferenceMut::LocalParameter(name)
                | ReferenceMut::Table(name)
                | ReferenceMut::Timeseries(name)
                | ReferenceMut::MetricSet(name) => *name = "rewritten".to_string(),
                // An edge's endpoints are rewritten through their own `Node` arm.
                ReferenceMut::Edge(_) => {}
            }
        });
        seen.sort();
        assert_eq!(seen, EXPECTED_REFERENCES);

        for reference in collect_references(&network) {
            let (kind, name) = reference.split_once(':').unwrap();
            let expected = if kind == "Edge" {
                "rewritten->rewritten"
            } else {
                "rewritten"
            };
            assert_eq!(name, expected, "{kind} was not rewritten");
        }
    }

    /// Every reference should be reported with the element that holds it, and a reference
    /// inside a local parameter with the enclosing node, which is the scope that resolves it.
    #[test]
    fn test_visit_owned_references_names_the_owner() {
        let network = NetworkSchema::from_str(NETWORK_WITH_REFERENCES).unwrap();

        assert_eq!(
            collect_owned_references(&network),
            [
                "edge \"edge-from->edge-to\": Node:edge-from",
                "edge \"edge-from->edge-to\": Node:edge-to",
                "metric set \"ms1\": Parameter:metric-set-parameter",
                "node \"supply\": LocalParameter:node-local-parameter",
                "node \"supply\": Parameter:node-parameter",
                "node \"supply\": Table:node-table",
                "node \"supply\": Timeseries:local-parameter-timeseries",
                // A CSV output may name several metric sets, and yields one reference each.
                "output \"csv-out\": MetricSet:csv-output-metric-set-1",
                "output \"csv-out\": MetricSet:csv-output-metric-set-2",
                "output \"hdf-out\": MetricSet:hdf5-output-metric-set",
                "output \"memory-out\": MetricSet:memory-output-metric-set",
                "parameter \"constant-from-table\": Table:constant-value-table",
                "parameter \"index-agg\": LocalParameter:index-metric-local-parameter",
                "parameter \"index-agg\": Node:index-metric-node",
                "parameter \"index-agg\": Table:index-metric-table",
                "parameter \"index-agg\": Timeseries:index-metric-timeseries",
                "parameter \"index-holder\": Edge:metric-edge-from->metric-edge-to",
                "parameter \"index-holder\": Node:metric-edge-from",
                "parameter \"index-holder\": Node:metric-edge-to",
                "parameter \"index-holder\": Parameter:index-metric-parameter",
                "virtual node \"licence\": Node:aggregated-node-component",
                // Held by the virtual node's own local parameter.
                "virtual node \"licence\": Parameter:virtual-node-local-parameter",
                "virtual node \"licence\": VirtualNode:virtual-node-metric",
            ]
        );
    }

    /// Two metrics naming edges that share endpoints and differ only in slot.
    const NETWORK_WITH_SLOTTED_EDGES: &str = r#"
    {
        "nodes": [],
        "edges": [],
        "parameters": [
            {
                "meta": { "name": "p1" },
                "type": "Aggregated",
                "phase": "Before",
                "agg_func": { "type": "Sum" },
                "metrics": [
                    {
                        "type": "Edge",
                        "edge": {
                            "from_node": "reservoir",
                            "to_node": "reach",
                            "from_slot": { "type": "Spill" }
                        }
                    },
                    {
                        "type": "Edge",
                        "edge": { "from_node": "reservoir", "to_node": "reach" }
                    }
                ]
            }
        ]
    }
    "#;

    /// An edge reference carries all four fields, so two edges sharing endpoints and differing
    /// only in slot are told apart.
    #[test]
    fn test_edge_references_are_distinguished_by_slot() {
        let network = NetworkSchema::from_str(NETWORK_WITH_SLOTTED_EDGES).unwrap();

        let edges: Vec<String> = collect_references(&network)
            .into_iter()
            .filter(|r| r.starts_with("Edge:"))
            .collect();

        assert_eq!(edges, ["Edge:reservoir->reach", "Edge:reservoir[Spill]->reach"]);
    }

    /// A network where a global parameter, and a local parameter of two different nodes, all
    /// share one name.
    const NETWORK_WITH_SHARED_PARAMETER_NAME: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "n1" },
                "type": "Input",
                "max_flow": { "type": "Parameter", "name": "shared" },
                "min_flow": { "type": "LocalParameter", "name": "shared" }
            },
            {
                "meta": { "name": "n2" },
                "type": "Input",
                "max_flow": { "type": "LocalParameter", "name": "shared" }
            }
        ],
        "edges": []
    }
    "#;

    /// Renaming a global parameter must not touch a local one of the same name. The kind on the
    /// reference is what keeps them apart; a bare name could not.
    #[test]
    fn test_renaming_a_global_parameter_leaves_local_parameters_alone() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_SHARED_PARAMETER_NAME).unwrap();

        network.visit_references_mut(&mut |reference| {
            if let ReferenceMut::Parameter(name) = reference
                && name == "shared"
            {
                *name = "renamed".to_string();
            }
        });

        assert_eq!(
            collect_owned_references(&network),
            [
                "node \"n1\": LocalParameter:shared",
                "node \"n1\": Parameter:renamed",
                "node \"n2\": LocalParameter:shared",
            ]
        );
    }

    /// A local parameter resolves in its owner's list, so renaming one node's must not touch
    /// another node's of the same name. Only the owner makes that possible.
    #[test]
    fn test_renaming_a_local_parameter_is_scoped_to_its_owner() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_SHARED_PARAMETER_NAME).unwrap();

        network.visit_owned_references_mut(&mut |owner, reference| {
            if let (Owner::Node("n1"), ReferenceMut::LocalParameter(name)) = (owner, reference)
                && name == "shared"
            {
                *name = "renamed".to_string();
            }
        });

        assert_eq!(
            collect_owned_references(&network),
            [
                "node \"n1\": LocalParameter:renamed",
                "node \"n1\": Parameter:shared",
                "node \"n2\": LocalParameter:shared",
            ]
        );
    }
}
