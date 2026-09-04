use crate::metric::{IndexMetric, Metric};
use std::collections::HashMap;
use std::num::NonZeroUsize;
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

/// A trait for recursively visiting node references in a schema.
///
/// This trait is used to visit every place a node's name is *referred to*. This is useful,
/// for example, for finding every component that depends on a node.
///
/// This vistor assumes that names are unique in the schema, and that every reference is
/// unambiguous. This is guaranteed by [`NetworkSchema::validate`](crate::NetworkSchema::validate),
/// which should be called before visiting references.
///
/// This trait does **not** visit the name a node gives *itself* in its
/// [`crate::nodes::NodeMeta`], because that is a definition rather than a reference.
pub trait VisitNodeReferences {
    fn visit_node_references<F: FnMut(&str)>(&self, _visitor: &mut F) {}

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, _visitor: &mut F) {}
}

impl VisitNodeReferences for Metric {
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        match self {
            Metric::Node(node_ref) => node_ref.visit_node_references(visitor),
            Metric::VirtualNode(node_ref) => node_ref.visit_node_references(visitor),
            Metric::Edge(edge_ref) => edge_ref.visit_node_references(visitor),
            Metric::Literal { .. }
            | Metric::Table(_)
            | Metric::Timeseries(_)
            | Metric::Parameter(_)
            | Metric::LocalParameter(_)
            | Metric::InterNetworkTransfer { .. } => {}
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        match self {
            Metric::Node(node_ref) => node_ref.visit_node_references_mut(visitor),
            Metric::VirtualNode(node_ref) => node_ref.visit_node_references_mut(visitor),
            Metric::Edge(edge_ref) => edge_ref.visit_node_references_mut(visitor),
            Metric::Literal { .. }
            | Metric::Table(_)
            | Metric::Timeseries(_)
            | Metric::Parameter(_)
            | Metric::LocalParameter(_)
            | Metric::InterNetworkTransfer { .. } => {}
        }
    }
}

impl VisitNodeReferences for IndexMetric {
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        match self {
            IndexMetric::Node(node_ref) => node_ref.visit_node_references(visitor),
            IndexMetric::Constant { .. }
            | IndexMetric::Table(_)
            | IndexMetric::Timeseries(_)
            | IndexMetric::Parameter(_)
            | IndexMetric::LocalParameter(_)
            | IndexMetric::InterNetworkTransfer { .. } => {}
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        match self {
            IndexMetric::Node(node_ref) => node_ref.visit_node_references_mut(visitor),
            IndexMetric::Constant { .. }
            | IndexMetric::Table(_)
            | IndexMetric::Timeseries(_)
            | IndexMetric::Parameter(_)
            | IndexMetric::LocalParameter(_)
            | IndexMetric::InterNetworkTransfer { .. } => {}
        }
    }
}

impl<T> VisitNodeReferences for Option<T>
where
    T: VisitNodeReferences,
{
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_node_references(visitor);
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_node_references_mut(visitor);
        }
    }
}

impl<T> VisitNodeReferences for Vec<T>
where
    T: VisitNodeReferences,
{
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_node_references(visitor);
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_node_references_mut(visitor);
        }
    }
}

/// Visit all the node references in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitNodeReferences for HashMap<K, V>
where
    V: VisitNodeReferences,
{
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_node_references(visitor);
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_node_references_mut(visitor);
        }
    }
}

impl<A, B> VisitNodeReferences for (A, B)
where
    A: VisitNodeReferences,
    B: VisitNodeReferences,
{
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        self.0.visit_node_references(visitor);
        self.1.visit_node_references(visitor);
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        self.0.visit_node_references_mut(visitor);
        self.1.visit_node_references_mut(visitor);
    }
}

impl VisitNodeReferences for u8 {}
impl VisitNodeReferences for i8 {}
impl VisitNodeReferences for u16 {}
impl VisitNodeReferences for i16 {}
impl VisitNodeReferences for u32 {}
impl VisitNodeReferences for i32 {}

impl VisitNodeReferences for f32 {}
impl VisitNodeReferences for f64 {}
impl<const N: usize> VisitNodeReferences for [f64; N] {}
impl<const N: usize> VisitNodeReferences for [Metric; N] {
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_node_references(visitor);
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_node_references_mut(visitor);
        }
    }
}
impl VisitNodeReferences for bool {}
impl VisitNodeReferences for u64 {}
/// A plain string is not a node reference; only the reference types are.
impl VisitNodeReferences for String {}
impl VisitNodeReferences for PathBuf {}
impl VisitNodeReferences for NonZeroUsize {}

impl VisitNodeReferences for serde_json::Value {}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::network::NetworkSchema;
    use crate::nodes::VirtualNode;
    use crate::visit::{VisitMetrics, VisitNodeReferences};
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

    /// Collect every visited reference name, sorted, so that the assertions do not depend on the
    /// order in which the schema happens to be walked.
    fn collect_node_references(network: &NetworkSchema) -> Vec<String> {
        let mut refs = Vec::new();
        network.visit_node_references(&mut |name| refs.push(name.to_string()));
        refs.sort();
        refs
    }

    /// A network containing a node reference in every location a node name can appear.
    ///
    /// Each reference names the location it appears in, so that a location the visitor fails to
    /// reach can be identified from the assertion failure.
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
                "phase": "Before",
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
    /// so they refer to *nodes* even though they are reached through a virtual node.
    #[test]
    fn test_visit_node_references() {
        let network = NetworkSchema::from_str(NETWORK_WITH_NODE_REFERENCES).unwrap();

        assert_eq!(
            collect_node_references(&network),
            vec![
                "aggregated-nodes",
                "aggregated-storage-nodes",
                "edge-from",
                "edge-to",
                "index-metric",
                "metric-edge-from",
                "metric-edge-to",
                "metric-set-metric",
                "node-metric",
                "virtual-node-metric",
                "virtual-storage-nodes",
            ]
        );
    }

    /// The names a node gives itself are definitions rather than references, so they should not
    /// be visited.
    #[test]
    fn test_visit_node_references_skips_component_names() {
        let network = NetworkSchema::from_str(NETWORK_WITH_NODE_REFERENCES).unwrap();

        let names = collect_node_references(&network);

        for defined_name in ["target", "downstream", "licence", "agg", "agg-storage", "p1", "ms1"] {
            assert!(!names.contains(&defined_name.to_string()), "{defined_name} was visited");
        }
    }

    /// The mutable visitor should rewrite every reference to a renamed node.
    ///
    /// Node names are a single name-space, so the visitor rewrites by name alone.
    #[test]
    fn test_visit_node_references_mut() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_NODE_REFERENCES).unwrap();

        network.visit_node_references_mut(&mut |name| {
            if name == "edge-from" {
                *name = "renamed".to_string();
            }
        });

        assert_eq!(network.edges[0].from_node, "renamed");

        let names = collect_node_references(&network);
        assert!(names.contains(&"renamed".to_string()));
        assert!(!names.contains(&"edge-from".to_string()));
    }

    /// A reference to a virtual node is visited in the same way as a reference to a node, so a
    /// rename reaches both.
    #[test]
    fn test_visit_node_references_mut_rewrites_virtual_node_references() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_NODE_REFERENCES).unwrap();

        network.visit_node_references_mut(&mut |name| {
            if name == "virtual-node-metric" {
                *name = "renamed-vn".to_string();
            }
        });

        let names = collect_node_references(&network);
        assert!(names.contains(&"renamed-vn".to_string()));
        assert!(!names.contains(&"virtual-node-metric".to_string()));
    }
}
