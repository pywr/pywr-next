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

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::network::NetworkSchema;
    use crate::nodes::VirtualNode;
    use crate::visit::VisitMetrics;
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
    fn collect(network: &NetworkSchema) -> Vec<String> {
        let mut locations = Vec::new();
        network.visit_metrics(&mut |metric| locations.push(location(metric)));
        locations.sort();
        locations
    }

    /// As [`collect`], but using the mutable visitor.
    fn collect_mut(network: &mut NetworkSchema) -> Vec<String> {
        let mut locations = Vec::new();
        network.visit_metrics_mut(&mut |metric| locations.push(location(metric)));
        locations.sort();
        locations
    }

    /// Every location a metric can appear should be reachable from the visitor.
    #[test]
    fn test_visit_metrics_reaches_every_metric() {
        let network = NetworkSchema::from_str(NETWORK_WITH_METRICS).unwrap();

        assert_eq!(collect(&network), EXPECTED_LOCATIONS);
    }

    /// The mutable visitor should visit the same metrics.
    #[test]
    fn test_visit_metrics_mut_reaches_every_metric() {
        let mut network = NetworkSchema::from_str(NETWORK_WITH_METRICS).unwrap();

        assert_eq!(collect_mut(&mut network), EXPECTED_LOCATIONS);
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
}
