use super::NetworkSchema;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)] // We want to be explicit about the error types for clarity.
pub enum NetworkMergeError {
    #[error("Duplicate node name found when merging networks: {0}")]
    DuplicateNodeName(String),
    #[error("Duplicate parameter name found when merging networks: {0}")]
    DuplicateParameterName(String),
    #[error("Duplicate edge from `{from_node}` to `{to_node}`")]
    DuplicateEdge { from_node: String, to_node: String },
    #[error("Duplicate table name found when merging networks: {0}")]
    DuplicateTableName(String),
    #[error("Duplicate time series name found when merging networks: {0}")]
    DuplicateTimeSeriesName(String),
    #[error("Duplicate output name found when merging networks: {0}")]
    DuplicateOutputName(String),
    #[error("Duplicate metric found when merging metric sets with name `{0}`")]
    DuplicateMetric(String),
}

/// Options for merging two [`NetworkSchema`] networks together.
#[derive(Debug, Clone, Default)]
pub struct NetworkMergeOptions {
    /// If true, the coordinates of placeholder nodes will be kept when merging networks.
    /// If false, the coordinates of placeholder nodes will be replaced by the coordinates of the
    /// corresponding node in the other network.
    pub keep_placeholder_positions: bool,
    /// Position offset to apply to the schematic coordinates of nodes when merging networks.
    pub schematic_position_offset: Option<(f32, f32)>,
    /// Position offset to apply to the geographic coordinates of nodes when merging networks.
    pub geographic_position_offset: Option<(f32, f32)>,
}

impl NetworkSchema {
    /// Merge another [`NetworkSchema`] into this one.
    ///
    /// This will combine the nodes, virtual nodes, edges, and parameters of both networks.
    /// If there are any duplicate node or parameter names, an error will be returned. However,
    /// placeholder types (e.g. [`crate::nodes::PlaceholderNode`]) are replaced.
    ///
    /// Metric sets are merged by name, with the metrics of any metric sets with the same name being
    /// combined. Other information in the metric set (e.g. filters) is **not** merged.
    ///
    /// If an error occurs during the merge, the network will be left in a partially merged state.
    /// It is recommended to clone the network before merging if you want to keep the original network
    /// intact.
    pub fn merge(&mut self, other: NetworkSchema, options: &NetworkMergeOptions) -> Result<(), NetworkMergeError> {
        // Merge nodes replacing placeholders at their index if they exist, otherwise appending
        // to the end of the list, or returning an error if a duplicate name is found.
        for node in other.nodes {
            match self.get_node_by_name_mut(node.name()) {
                Some(existing_node) => {
                    if existing_node.is_placeholder() {
                        let orig_position = options
                            .keep_placeholder_positions
                            .then(|| existing_node.meta().position)
                            .flatten();

                        *existing_node = node;

                        if let Some(position) = orig_position {
                            // Restore the original position if we are keeping placeholder positions
                            existing_node.meta_mut().position = Some(position);
                        } else {
                            // Otherwise, apply any position offsets if they are specified in the options
                            if let Some(offset) = options.schematic_position_offset {
                                existing_node.meta_mut().apply_schematic_offset(offset);
                            }
                            if let Some(offset) = options.geographic_position_offset {
                                existing_node.meta_mut().apply_geographic_offset(offset);
                            }
                        }
                    } else if node.is_placeholder() {
                        // If the incoming node is a placeholder, we can ignore it and keep the existing node
                        continue;
                    } else {
                        return Err(NetworkMergeError::DuplicateNodeName(node.name().to_string()));
                    }
                }
                None => {
                    // Check if the node name exists in the virtual nodes list
                    if self.get_virtual_node_index_by_name(node.name()).is_some() {
                        return Err(NetworkMergeError::DuplicateNodeName(node.name().to_string()));
                    }

                    let mut new_node = node;

                    if let Some(offset) = options.schematic_position_offset {
                        new_node.meta_mut().apply_schematic_offset(offset);
                    }
                    if let Some(offset) = options.geographic_position_offset {
                        new_node.meta_mut().apply_geographic_offset(offset);
                    }

                    self.nodes.push(new_node);
                }
            }
        }

        // Merge virtual nodes. As per nodes, replacing placeholders at their index if they exist,
        // otherwise appending to the end of the list, or returning an error if a duplicate name is found.
        if let Some(other_virtual_nodes) = other.virtual_nodes {
            for v_node in other_virtual_nodes {
                match self.get_virtual_node_by_name_mut(v_node.name()) {
                    Some(existing_node) => {
                        if existing_node.is_placeholder() {
                            let orig_position = options
                                .keep_placeholder_positions
                                .then(|| existing_node.meta().position)
                                .flatten();

                            *existing_node = v_node;

                            if let Some(position) = orig_position {
                                existing_node.meta_mut().position = Some(position);
                            } else {
                                // Otherwise, apply any position offsets if they are specified in the options
                                if let Some(offset) = options.schematic_position_offset {
                                    existing_node.meta_mut().apply_schematic_offset(offset);
                                }
                                if let Some(offset) = options.geographic_position_offset {
                                    existing_node.meta_mut().apply_geographic_offset(offset);
                                }
                            }
                        } else if v_node.is_placeholder() {
                            // If the incoming node is a placeholder, we can ignore it and keep the existing node
                            continue;
                        } else {
                            return Err(NetworkMergeError::DuplicateNodeName(v_node.name().to_string()));
                        }
                    }
                    None => {
                        // Check if the node name exists as a regular node
                        if self.get_node_index_by_name(v_node.name()).is_some() {
                            return Err(NetworkMergeError::DuplicateNodeName(v_node.name().to_string()));
                        }

                        let mut new_v_node = v_node;

                        if let Some(offset) = options.schematic_position_offset {
                            new_v_node.meta_mut().apply_schematic_offset(offset);
                        }
                        if let Some(offset) = options.geographic_position_offset {
                            new_v_node.meta_mut().apply_geographic_offset(offset);
                        }

                        self.virtual_nodes.get_or_insert_default().push(new_v_node);
                    }
                }
            }
        }

        // Merge edges checking for duplicates
        for edge in other.edges {
            if self.edges.iter().any(|e| e == &edge) {
                return Err(NetworkMergeError::DuplicateEdge {
                    from_node: edge.from_node,
                    to_node: edge.to_node,
                });
            }
            self.edges.push(edge);
        }

        // Merge parameters
        if let Some(other_parameters) = other.parameters {
            for param in other_parameters {
                match self.get_parameter_by_name_mut(param.name()) {
                    Some(existing_param) => {
                        if existing_param.is_placeholder() {
                            *existing_param = param;
                        } else if param.is_placeholder() {
                            // If the incoming parameter is a placeholder, we can ignore it and keep the existing parameter
                            continue;
                        } else {
                            return Err(NetworkMergeError::DuplicateParameterName(param.name().to_string()));
                        }
                    }
                    None => {
                        self.parameters.get_or_insert_default().push(param);
                    }
                }
            }
        }

        // Merge tables
        if let Some(other_tables) = other.tables {
            for table in other_tables {
                match self.get_table_by_name_mut(table.name()) {
                    Some(existing_table) => {
                        if existing_table.is_placeholder() {
                            *existing_table = table;
                        } else if table.is_placeholder() {
                            // If the incoming table is a placeholder, we can ignore it and keep the existing table
                            continue;
                        } else {
                            return Err(NetworkMergeError::DuplicateTableName(table.name().to_string()));
                        }
                    }
                    None => {
                        self.tables.get_or_insert_default().push(table);
                    }
                }
            }
        }

        // Merge time series
        if let Some(other_time_series) = other.time_series {
            for ts in other_time_series {
                match self.get_time_series_by_name_mut(ts.name()) {
                    Some(existing_ts) => {
                        if existing_ts.is_placeholder() {
                            *existing_ts = ts;
                        } else if ts.is_placeholder() {
                            // If the incoming time series is a placeholder, we can ignore it and keep the existing time series
                            continue;
                        } else {
                            return Err(NetworkMergeError::DuplicateTimeSeriesName(ts.name().to_string()));
                        }
                    }
                    None => {
                        self.time_series.get_or_insert_default().push(ts);
                    }
                }
            }
        }

        // Merge metric sets. There are no placeholder metric sets. Instead, we merge the metrics
        // of any metric sets with the same name.
        if let Some(other_metric_sets) = other.metric_sets {
            for ms in other_metric_sets {
                let name = ms.name().to_string();
                match self.get_metric_set_by_name_mut(ms.name()) {
                    Some(existing_ms) => {
                        if let Some(incoming) = ms.meta.provenance.clone() {
                            match &mut existing_ms.meta.provenance {
                                Some(existing) => existing.merge(incoming),
                                None => existing_ms.meta.provenance = Some(incoming),
                            }
                        }
                        // Merge the metrics of the existing metric set with the new one.
                        if let Some(existing_metrics) = &mut existing_ms.metrics {
                            if let Some(new_metrics) = ms.metrics {
                                // Check for duplicate metrics
                                for new_metric in &new_metrics {
                                    if existing_metrics.iter().any(|m| m == new_metric) {
                                        return Err(NetworkMergeError::DuplicateMetric(name));
                                    }
                                }

                                existing_metrics.extend(new_metrics);
                            }
                        } else {
                            existing_ms.metrics = ms.metrics;
                        }
                    }
                    None => {
                        // No existing metric set with this name, so we can just add it.
                        self.metric_sets.get_or_insert_default().push(ms);
                    }
                }
            }
        }

        // Merge outputs. Replacing placeholders at their index if they exist, otherwise appending
        // to the end of the list, or returning an error if a duplicate name is found.
        if let Some(other_outputs) = other.outputs {
            for output in other_outputs {
                match self.get_output_by_name_mut(output.name()) {
                    Some(existing_output) => {
                        if existing_output.is_placeholder() {
                            *existing_output = output;
                        } else if output.is_placeholder() {
                            // If the incoming output is a placeholder, we can ignore it and keep the existing output
                            continue;
                        } else {
                            return Err(NetworkMergeError::DuplicateOutputName(output.name().to_string()));
                        }
                    }
                    None => {
                        self.outputs.get_or_insert_default().push(output);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{NetworkMergeError, NetworkMergeOptions};
    use crate::network::tests::parse_network;

    #[test]
    fn test_merge_appends_unique_nodes_and_edges() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "a" }, "type": "Input" },
                    { "meta": { "name": "b" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "a", "to_node": "b" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "c" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "b", "to_node": "c" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options).expect("Merge should succeed");

        assert_eq!(base.nodes.len(), 3);
        assert_eq!(base.edges.len(), 2);
        assert!(base.get_node_by_name("c").is_some());
    }

    #[test]
    fn test_merge_replaces_placeholder_node() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Placeholder" }
                ],
                "edges": []
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Input" }
                ],
                "edges": []
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder node");

        let merged = base.get_node_by_name("shared").expect("Node should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_non_placeholder_node_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Input" }
                ],
                "edges": []
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Output" }
                ],
                "edges": []
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate node names");
        assert!(matches!(err, NetworkMergeError::DuplicateNodeName(name) if name == "shared"));
    }

    #[test]
    fn test_merge_rejects_duplicate_edge() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "a" }, "type": "Input" },
                    { "meta": { "name": "b" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "a", "to_node": "b" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [
                    { "from_node": "a", "to_node": "b", "meta": {} }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate edges");
        assert!(matches!(
            err,
            NetworkMergeError::DuplicateEdge { from_node, to_node } if from_node == "a" && to_node == "b"
        ));
    }

    #[test]
    fn test_merge_combines_metric_set_content_for_matching_names() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "metric_sets": [
                    { "meta": { "name": "main" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "metric_sets": [
                    { "meta": { "name": "main" }, "metrics": [] }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options).expect("Merge should succeed");

        let metric_sets = base.metric_sets.as_ref().expect("Metric sets should exist");
        assert_eq!(metric_sets.len(), 1);
        assert!(
            base.get_metric_set_by_name("main")
                .and_then(|ms| ms.metrics.as_ref())
                .is_some_and(|metrics| metrics.is_empty())
        );
    }

    #[test]
    fn test_merge_replaces_placeholder_virtual_node() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "virtual_nodes": [
                    { "meta": { "name": "v-shared" }, "type": "Placeholder" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "virtual_nodes": [
                    { "meta": { "name": "v-shared" }, "type": "Aggregated", "nodes": [] }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder virtual node");

        let merged = base
            .get_virtual_node_by_name("v-shared")
            .expect("Virtual node should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_replaces_placeholder_parameter() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Placeholder", "meta": { "name": "p-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 1.0 } }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder parameter");

        let merged = base
            .get_parameter_by_name("p-shared")
            .expect("Parameter should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_parameter_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 1.0 } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 2.0 } }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate parameter names");
        assert!(matches!(err, NetworkMergeError::DuplicateParameterName(name) if name == "p-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_table() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "Placeholder", "meta": { "name": "tbl-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "data.csv" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder table");

        let merged = base
            .get_table_by_name("tbl-shared")
            .expect("Table should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_table_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "data.csv" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "other.csv" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate table names");
        assert!(matches!(err, NetworkMergeError::DuplicateTableName(name) if name == "tbl-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_time_series() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Placeholder", "meta": { "name": "ts-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "time-series.csv" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder time series");

        let merged = base
            .get_time_series_by_name("ts-shared")
            .expect("TimeSeries should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_time_series_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "time-series.csv" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "other.csv" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate time series names");
        assert!(matches!(err, NetworkMergeError::DuplicateTimeSeriesName(name) if name == "ts-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_output() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Placeholder", "meta": { "name": "out-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "meta": { "name": "out-shared" }, "metric_set": "ms" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        base.merge(other, &options)
            .expect("Merge should replace placeholder output");

        let merged = base
            .get_output_by_name("out-shared")
            .expect("Output should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_output_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "meta": { "name": "out-shared" }, "metric_set": "ms" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "meta": { "name": "out-shared" }, "metric_set": "ms2" }
                ]
            }
            "#,
        );

        let options = NetworkMergeOptions::default();
        let err = base
            .merge(other, &options)
            .expect_err("Merge should reject duplicate output names");
        assert!(matches!(err, NetworkMergeError::DuplicateOutputName(name) if name == "out-shared"));
    }
}
