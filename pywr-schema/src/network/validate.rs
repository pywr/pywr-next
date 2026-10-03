use super::NetworkSchema;
use crate::data_tables::DataTable;
use crate::edge::Edge;
use crate::nodes::VirtualNode;
use crate::parameters::{Parameter, validate_each_parameter};
use crate::time_series::TimeSeries;
use crate::util::duplicates;
use crate::validation::{
    DuplicateNodeName, EdgeProblem, EdgeValidationError, NetworkProblem, NetworkValidationError, NodeProblem,
    VirtualNodeProblem,
};
use crate::visit::{Owner, Reference};
use std::collections::HashMap;

impl NetworkSchema {
    /// Validate an edge against the network
    ///
    /// The following conditions are checked, with the first problem found being
    /// returned:
    ///
    /// - Both ends name an entry of `nodes`; a virtual node is not an edge end.
    /// - The two ends are different nodes.
    /// - The `from_node` provides flow and the `to_node` receives it, each through a slot it has;
    ///   see [`Node::validate_edge_from`](crate::nodes::Node::validate_edge_from) and
    ///   [`Node::validate_edge_to`](crate::nodes::Node::validate_edge_to) for the rules.
    ///
    /// All but the second are checks `pywr-core` makes only while building. The second is a
    /// schema-level rule: a composite node such as a `Reservoir` is one node here, so
    /// `Reservoir[Spill] -> Reservoir` is a loop, whereas `pywr-core` sees the flattened network,
    /// where the storage and spill are separate nodes.
    ///
    /// An end whose name is used by more than one node resolves to the first of them.
    pub fn validate_edge(&self, edge: &Edge) -> Result<(), EdgeProblem> {
        let from_node = self.get_node_by_name(&edge.from_node).ok_or_else(|| {
            match self.get_virtual_node_by_name(&edge.from_node) {
                Some(virtual_node) => EdgeProblem::VirtualFromNode {
                    name: edge.from_node.clone(),
                    node_type: virtual_node.node_type(),
                },
                None => EdgeProblem::UnknownFromNode(edge.from_node.clone()),
            }
        })?;

        let to_node =
            self.get_node_by_name(&edge.to_node)
                .ok_or_else(|| match self.get_virtual_node_by_name(&edge.to_node) {
                    Some(virtual_node) => EdgeProblem::VirtualToNode {
                        name: edge.to_node.clone(),
                        node_type: virtual_node.node_type(),
                    },
                    None => EdgeProblem::UnknownToNode(edge.to_node.clone()),
                })?;

        if edge.from_node == edge.to_node {
            return Err(EdgeProblem::SelfEdge(edge.from_node.clone()));
        }

        from_node.validate_edge_from(edge.from_slot.as_ref())?;
        to_node.validate_edge_to(edge.to_slot.as_ref())
    }

    /// The problems [`VirtualNode::validate_member`] finds with `virtual_node`'s members, in the
    /// order listed. A member naming a node that is not in `nodes` is skipped.
    fn member_problems(&self, virtual_node: &VirtualNode) -> Vec<NetworkProblem> {
        virtual_node
            .members()
            .iter()
            .filter_map(|member| {
                let node = self.get_node_by_name(&member.name)?;
                let problem = virtual_node.validate_member(member, node).err()?;

                Some(NetworkProblem::InvalidMember {
                    virtual_node: virtual_node.name().to_string(),
                    node: member.name.clone(),
                    node_type: node.node_type(),
                    problem,
                })
            })
            .collect()
    }

    /// The problems [`Node::validate_reference`](crate::nodes::Node::validate_reference) and
    /// [`VirtualNode::validate_reference`] find with the metrics reading a node or virtual node. A
    /// reference naming no node is skipped.
    fn node_reference_problems(&self) -> Vec<NetworkProblem> {
        let mut problems = Vec::new();

        self.visit_owned_references(&mut |owner, reference| {
            let problem = match reference {
                Reference::Node {
                    name,
                    attribute,
                    metric: Some(metric),
                } => {
                    let Some(node) = self.get_node_by_name(name) else {
                        return;
                    };
                    let Err(problem) = node.validate_reference(attribute, metric) else {
                        return;
                    };

                    NetworkProblem::InvalidNodeReference {
                        owner: owner.into(),
                        node: name.to_string(),
                        node_type: node.node_type(),
                        problem,
                    }
                }
                Reference::VirtualNode { name, attribute } => {
                    let Some(virtual_node) = self.get_virtual_node_by_name(name) else {
                        return;
                    };
                    let Err(problem) = virtual_node.validate_reference(attribute) else {
                        return;
                    };

                    NetworkProblem::InvalidVirtualNodeReference {
                        owner: owner.into(),
                        virtual_node: name.to_string(),
                        node_type: virtual_node.node_type(),
                        problem,
                    }
                }
                _ => return,
            };

            problems.push(problem);
        });

        problems
    }

    /// The problems [`Parameter::validate_reference`] finds with the parameter references. A
    /// reference naming no parameter is skipped.
    fn parameter_reference_problems(&self) -> Vec<NetworkProblem> {
        let mut problems = Vec::new();

        self.visit_owned_references(&mut |owner, reference| {
            let (name, node, key, metric, return_value) = match reference {
                Reference::Parameter {
                    name,
                    key,
                    metric,
                    return_value,
                } => (name, None, key, metric, return_value),
                Reference::LocalParameter {
                    node,
                    name,
                    key,
                    metric,
                    return_value,
                } => {
                    // Without a `node` it resolves in the node or virtual node holding it.
                    let node = node.or(match owner {
                        Owner::Node(node) | Owner::VirtualNode(node) => Some(node),
                        _ => None,
                    });
                    let Some(node) = node else { return };
                    (name, Some(node), key, metric, return_value)
                }
                _ => return,
            };

            let resolved = match node {
                Some(node) => match self.get_node_by_name(node) {
                    Some(n) => n.get_local_parameter(name),
                    None => self
                        .get_virtual_node_by_name(node)
                        .and_then(|n| n.get_local_parameter(name)),
                },
                None => self.get_parameter_by_name(name),
            };
            let Some(parameter) = resolved else {
                return;
            };
            let Err(problem) = parameter.validate_reference(key, metric, return_value) else {
                return;
            };

            problems.push(NetworkProblem::InvalidParameterReference {
                owner: owner.into(),
                parameter: name.to_string(),
                node: node.map(str::to_string),
                problem,
            });
        });

        problems
    }

    /// The problems with the tables pywr cannot load, then with the table references, each
    /// checked by [`DataTable::validate_reference`]. A reference naming no table is skipped.
    fn table_problems(&self) -> Vec<NetworkProblem> {
        let mut problems: Vec<NetworkProblem> = self
            .tables
            .iter()
            .flatten()
            .filter_map(|table| match table {
                DataTable::CSV(csv) if !csv.is_lookup_supported() => Some(NetworkProblem::UnsupportedTableLookup {
                    table: csv.meta.name.clone(),
                    value_type: csv.ty,
                    lookup: csv.lookup.clone(),
                }),
                _ => None,
            })
            .collect();

        self.visit_owned_references(&mut |owner, reference| {
            let Reference::Table { table_ref, expected } = reference else {
                return;
            };

            let problem = self
                .get_table_by_name(&table_ref.table)
                .and_then(|table| table.validate_reference(table_ref, expected).err());

            if let Some(problem) = problem {
                problems.push(NetworkProblem::InvalidTableReference {
                    owner: owner.into(),
                    table: table_ref.table.clone(),
                    problem,
                });
            }
        });

        problems
    }

    /// The problems [`Node::validate`](crate::nodes::Node::validate) finds, with a local
    /// parameter's reported as [`NetworkProblem::InvalidParameter`].
    fn node_problems(&self) -> Vec<NetworkProblem> {
        self.nodes
            .iter()
            .flat_map(|node| {
                node.validate()
                    .err()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|problem| match problem {
                        NodeProblem::InvalidLocalParameter { parameter, problem } => NetworkProblem::InvalidParameter {
                            parameter,
                            node: Some(node.name().to_string()),
                            problem,
                        },
                        problem => NetworkProblem::InvalidNode {
                            node: node.name().to_string(),
                            problem,
                        },
                    })
            })
            .collect()
    }

    /// The problems [`VirtualNode::validate`] finds, with a local parameter's reported as
    /// [`NetworkProblem::InvalidParameter`].
    fn virtual_node_problems(&self) -> Vec<NetworkProblem> {
        self.virtual_nodes
            .iter()
            .flatten()
            .flat_map(|virtual_node| {
                virtual_node
                    .validate()
                    .err()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|problem| match problem {
                        VirtualNodeProblem::InvalidLocalParameter { parameter, problem } => {
                            NetworkProblem::InvalidParameter {
                                parameter,
                                node: Some(virtual_node.name().to_string()),
                                problem,
                            }
                        }
                        problem => NetworkProblem::InvalidVirtualNode {
                            virtual_node: virtual_node.name().to_string(),
                            problem,
                        },
                    })
            })
            .collect()
    }

    /// The problems [`Parameter::validate`] finds in the network's own parameters.
    fn parameter_problems(&self) -> Vec<NetworkProblem> {
        validate_each_parameter(self.parameters.as_deref().unwrap_or_default())
            .map(|(parameter, problem)| NetworkProblem::InvalidParameter {
                parameter: parameter.to_string(),
                node: None,
                problem,
            })
            .collect()
    }

    /// The problems [`MetricSet::validate`](crate::metric_sets::MetricSet::validate) finds.
    fn metric_set_problems(&self) -> Vec<NetworkProblem> {
        self.metric_sets
            .iter()
            .flatten()
            .flat_map(|metric_set| {
                metric_set
                    .validate()
                    .err()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|problem| NetworkProblem::InvalidMetricSet {
                        metric_set: metric_set.name().to_string(),
                        problem,
                    })
            })
            .collect()
    }

    /// Validate the network schema and report every problem.
    ///
    /// The following are checked:
    ///
    /// - The schema is unambiguous.
    /// - Each edge could be made; see [`NetworkSchema::validate_edge`] for the rules.
    /// - Each virtual node can take its members; see [`VirtualNode::validate_member`] for the rules.
    /// - Each metric can read the node or virtual node it names; see
    ///   [`Node::validate_reference`](crate::nodes::Node::validate_reference) and
    ///   [`VirtualNode::validate_reference`] for the rules.
    /// - Each parameter reference can read the parameter it names; see
    ///   [`Parameter::validate_reference`] for the rules.
    /// - Each table has a lookup pywr can load, and each table reference fits its table.
    /// - Each node's and virtual node's own fields and local parameters, and each parameter's and
    ///   metric set's own fields; see [`Node::validate`](crate::nodes::Node::validate),
    ///   [`VirtualNode::validate`], [`Parameter::validate`] and
    ///   [`MetricSet::validate`](crate::metric_sets::MetricSet::validate) for the rules.
    ///
    /// Whether the whole model can be built is not; use [`NetworkSchema::add_to_network`] for
    /// that. See [`NetworkProblem`] for the problems that are detected.
    pub fn validate(&self) -> Result<(), NetworkValidationError> {
        // Count the occurrences of each name in each of the two lists.
        let mut counts: HashMap<&str, (usize, usize)> = HashMap::with_capacity(self.nodes.len());

        for node in &self.nodes {
            counts.entry(node.name()).or_default().0 += 1;
        }

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            counts.entry(virtual_node.name()).or_default().1 += 1;
        }

        let mut duplicate_nodes: Vec<DuplicateNodeName> = counts
            .into_iter()
            .filter(|(_, (nodes, virtual_nodes))| nodes + virtual_nodes > 1)
            .map(|(name, (nodes, virtual_nodes))| DuplicateNodeName {
                name: name.to_string(),
                nodes,
                virtual_nodes,
            })
            .collect();

        let invalid_edges: Vec<EdgeValidationError> = self
            .edges
            .iter()
            .filter_map(|edge| {
                self.validate_edge(edge).err().map(|problem| EdgeValidationError {
                    edge: edge.clone(),
                    problem,
                })
            })
            .collect();

        // The duplicates come out of the hash map in a random order.
        duplicate_nodes.sort_by(|a, b| a.name.cmp(&b.name));

        let problems: Vec<NetworkProblem> = duplicate_nodes
            .into_iter()
            .map(NetworkProblem::DuplicateNodeName)
            .chain(
                duplicates(self.parameters.iter().flatten(), Parameter::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateParameterName {
                        name: name.to_string(),
                        count,
                    }),
            )
            .chain(
                duplicates(self.tables.iter().flatten(), DataTable::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateTableName {
                        name: name.to_string(),
                        count,
                    }),
            )
            .chain(
                duplicates(self.time_series.iter().flatten(), TimeSeries::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateTimeSeriesName {
                        name: name.to_string(),
                        count,
                    }),
            )
            .chain(
                duplicates(self.metric_sets.iter().flatten(), |metric_set| metric_set.name())
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateMetricSetName {
                        name: name.to_string(),
                        count,
                    }),
            )
            .chain(invalid_edges.into_iter().map(NetworkProblem::InvalidEdge))
            .chain(
                self.virtual_nodes
                    .iter()
                    .flatten()
                    .flat_map(|virtual_node| self.member_problems(virtual_node)),
            )
            .chain(self.node_reference_problems())
            .chain(self.parameter_reference_problems())
            .chain(self.table_problems())
            .chain(self.node_problems())
            .chain(self.virtual_node_problems())
            .chain(self.parameter_problems())
            .chain(self.metric_set_problems())
            .collect();

        if problems.is_empty() {
            Ok(())
        } else {
            Err(NetworkValidationError { name: None, problems })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::network::NetworkSchema;
    use crate::network::tests::parse_network;
    use crate::nodes::{NodeSlot, NodeType, VirtualNodeType};
    use crate::validation::{DuplicateNodeName, EdgeProblem, EdgeValidationError, NetworkProblem};

    /// Return the problems reported by [`NetworkSchema::validate`], or panic if it succeeded.
    fn expect_problems(network: &NetworkSchema) -> Vec<NetworkProblem> {
        match network.validate() {
            Err(error) => {
                assert_eq!(error.name, None, "A network validated on its own has no name");
                assert!(!error.problems.is_empty(), "An error must hold at least one problem");
                error.problems
            }
            Ok(()) => panic!("Expected validation to fail, but it succeeded"),
        }
    }

    /// Return the duplicates reported by [`NetworkSchema::validate`], or panic if it reported
    /// anything else.
    fn expect_duplicates(network: &NetworkSchema) -> Vec<DuplicateNodeName> {
        expect_problems(network)
            .into_iter()
            .map(|problem| match problem {
                NetworkProblem::DuplicateNodeName(duplicate) => duplicate,
                other => panic!("Expected only duplicate node names, but got: {other:?}"),
            })
            .collect()
    }

    /// Return the invalid edges reported by [`NetworkSchema::validate`] as `(edge, problem)`
    /// pairs, or panic if it reported anything else.
    fn expect_invalid_edges(network: &NetworkSchema) -> Vec<(String, EdgeProblem)> {
        expect_problems(network)
            .into_iter()
            .map(|problem| match problem {
                NetworkProblem::InvalidEdge(e) => (e.edge.to_string(), e.problem),
                other => panic!("Expected only invalid edges, but got: {other:?}"),
            })
            .collect()
    }

    const NETWORK_WITH_SEVERAL_DUPLICATES: &str = r#"
    {
        "nodes": [
            { "meta": { "name": "zzz" }, "type": "Input" },
            { "meta": { "name": "zzz" }, "type": "Input" },
            { "meta": { "name": "aaa" }, "type": "Output" },
            { "meta": { "name": "unique" }, "type": "Output" }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "aaa" },
                "type": "Aggregated",
                "nodes": [{ "name": "unique" }]
            }
        ],
        "edges": []
    }
    "#;

    /// Every duplicate is reported, not just the first one found. Nodes and virtual nodes are a
    /// single name-space, so a name shared between the two lists is a duplicate too.
    #[test]
    fn test_validate_reports_all_duplicates() {
        let network = parse_network(NETWORK_WITH_SEVERAL_DUPLICATES);

        assert_eq!(
            expect_duplicates(&network),
            vec![
                DuplicateNodeName {
                    name: "aaa".to_string(),
                    nodes: 1,
                    virtual_nodes: 1,
                },
                DuplicateNodeName {
                    name: "zzz".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                },
            ]
        );
    }

    /// A network with an edge for every [`EdgeProblem`] that does not need a virtual node, and
    /// into both node types that cannot receive flow.
    const NETWORK_WITH_INVALID_EDGES: &str = r#"
    {
        "nodes": [
            { "meta": { "name": "supply" }, "type": "Input" },
            { "meta": { "name": "catchment" }, "type": "Catchment", "flow": { "type": "Literal", "value": 0.0 } },
            { "meta": { "name": "link" }, "type": "Link" },
            { "meta": { "name": "demand" }, "type": "Output" }
        ],
        "edges": [
            { "from_node": "link", "to_node": "supply" },
            { "from_node": "link", "to_node": "missing" },
            { "from_node": "absent", "to_node": "link" },
            { "from_node": "demand", "to_node": "link" },
            { "from_node": "link", "from_slot": { "type": "Spill" }, "to_node": "demand" },
            { "from_node": "link", "to_node": "link" },
            { "from_node": "link", "to_node": "demand", "to_slot": { "type": "Storage" } },
            { "from_node": "link", "to_node": "catchment" }
        ]
    }
    "#;

    /// Every invalid edge is reported, in the order the edges are listed.
    #[test]
    fn test_validate_reports_all_invalid_edges() {
        let network = parse_network(NETWORK_WITH_INVALID_EDGES);

        assert_eq!(
            expect_invalid_edges(&network),
            vec![
                (
                    "link->supply".to_string(),
                    EdgeProblem::NoInflow {
                        name: "supply".to_string(),
                        node_type: NodeType::Input,
                    }
                ),
                (
                    "link->missing".to_string(),
                    EdgeProblem::UnknownToNode("missing".to_string())
                ),
                (
                    "absent->link".to_string(),
                    EdgeProblem::UnknownFromNode("absent".to_string())
                ),
                (
                    "demand->link".to_string(),
                    EdgeProblem::NoOutflow {
                        name: "demand".to_string(),
                        node_type: NodeType::Output,
                    }
                ),
                (
                    "link[Spill]->demand".to_string(),
                    EdgeProblem::UnknownFromSlot {
                        name: "link".to_string(),
                        node_type: NodeType::Link,
                        slot: NodeSlot::Spill,
                        valid: None,
                    }
                ),
                ("link->link".to_string(), EdgeProblem::SelfEdge("link".to_string())),
                (
                    "link->demand[Storage]".to_string(),
                    EdgeProblem::UnknownToSlot {
                        name: "demand".to_string(),
                        node_type: NodeType::Output,
                        slot: NodeSlot::Storage,
                        valid: None,
                    }
                ),
                (
                    "link->catchment".to_string(),
                    EdgeProblem::NoInflow {
                        name: "catchment".to_string(),
                        node_type: NodeType::Catchment,
                    }
                ),
            ]
        );
    }

    /// Edges connect only entries of `nodes`. A virtual node at either end is reported as the
    /// virtual node it is, rather than as a name the network does not define.
    #[test]
    fn test_validate_rejects_virtual_node_as_edge_end() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "supply" }, "type": "Input" },
                    { "meta": { "name": "demand" }, "type": "Output" }
                ],
                "virtual_nodes": [
                    {
                        "meta": { "name": "licence" },
                        "type": "Aggregated",
                        "nodes": [{ "name": "demand" }]
                    }
                ],
                "edges": [
                    { "from_node": "supply", "to_node": "demand" },
                    { "from_node": "licence", "to_node": "demand" },
                    { "from_node": "supply", "to_node": "licence" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![
                (
                    "licence->demand".to_string(),
                    EdgeProblem::VirtualFromNode {
                        name: "licence".to_string(),
                        node_type: VirtualNodeType::Aggregated,
                    }
                ),
                (
                    "supply->licence".to_string(),
                    EdgeProblem::VirtualToNode {
                        name: "licence".to_string(),
                        node_type: VirtualNodeType::Aggregated,
                    }
                ),
            ]
        );
    }

    /// A node cannot connect to itself even through a slot, although the flattened network that
    /// `pywr-core` builds would accept the edge.
    #[test]
    fn test_validate_rejects_self_edge_through_a_slot() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "reservoir" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                        "spill": "LinkNode"
                    }
                ],
                "edges": [
                    { "from_node": "reservoir", "from_slot": { "type": "Spill" }, "to_node": "reservoir" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![(
                "reservoir[Spill]->reservoir".to_string(),
                EdgeProblem::SelfEdge("reservoir".to_string())
            )]
        );
    }

    /// A slot is checked against the node's own configuration, not just its type: a `Reservoir`
    /// only has a `Spill` output slot when its spill is a link node.
    #[test]
    fn test_validate_checks_slot_against_node_configuration() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "with-spill" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                        "spill": "LinkNode"
                    },
                    {
                        "meta": { "name": "without-spill" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 }
                    },
                    { "meta": { "name": "river" }, "type": "River" }
                ],
                "edges": [
                    { "from_node": "with-spill", "from_slot": { "type": "Spill" }, "to_node": "river" },
                    { "from_node": "without-spill", "from_slot": { "type": "Spill" }, "to_node": "river" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![(
                "without-spill[Spill]->river".to_string(),
                EdgeProblem::UnknownFromSlot {
                    name: "without-spill".to_string(),
                    node_type: NodeType::Reservoir,
                    slot: NodeSlot::Spill,
                    valid: Some(vec![NodeSlot::Storage]),
                }
            )]
        );
    }

    /// A slot problem names the slots the node does have, so that a mistyped slot can be
    /// corrected without reading the node's definition; a node with no slots of that kind says so.
    #[test]
    fn test_invalid_slot_problem_lists_the_slots_the_node_has() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "split" },
                        "type": "RiverSplitWithGauge",
                        "splits": [
                            { "factor": { "type": "Literal", "value": 0.5 } },
                            { "factor": { "type": "Literal", "value": 0.5 }, "slot_name": "to-supply" }
                        ]
                    },
                    { "meta": { "name": "river" }, "type": "Link" },
                    { "meta": { "name": "demand" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "split", "from_slot": { "type": "Split", "position": 5 }, "to_node": "river" },
                    { "from_node": "river", "from_slot": { "type": "Spill" }, "to_node": "demand" }
                ]
            }
            "#,
        );

        let messages: Vec<String> = expect_invalid_edges(&network)
            .iter()
            .map(|(_, problem)| problem.to_string())
            .collect();

        assert_eq!(
            messages,
            vec![
                "The `RiverSplitWithGauge` node `split` has no output slot `Split[5]`. Its output slots are: `River`, `Split[0]`, `User[to-supply]`.",
                "The `Link` node `river` has no output slot `Spill`. Nodes of this type have no output slots.",
            ]
        );
    }

    /// A network with a member for every member problem, and members that pass: a part the node
    /// builds, a placeholder, a node the network does not have, and an aggregated storage member
    /// naming a component, which it ignores.
    const NETWORK_WITH_INVALID_MEMBERS: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "reservoir" },
                "type": "Reservoir",
                "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                "compensation": { "type": "Literal", "value": 1.0 },
                "rainfall": { "data": { "type": "Literal", "value": 1.0 } }
            },
            {
                "meta": { "name": "bare-reservoir" },
                "type": "Reservoir",
                "initial_volume": { "type": "Proportional", "proportion": 1.0 }
            },
            {
                "meta": { "name": "store" },
                "type": "Storage",
                "initial_volume": { "type": "Proportional", "proportion": 1.0 }
            },
            { "meta": { "name": "works" }, "type": "WaterTreatmentWorks" },
            { "meta": { "name": "river" }, "type": "River" },
            { "meta": { "name": "loss-link" }, "type": "LossLink" },
            { "meta": { "name": "link" }, "type": "Link" },
            { "meta": { "name": "placeholder" }, "type": "Placeholder" }
        ],
        "edges": [],
        "virtual_nodes": [
            {
                "meta": { "name": "agg" },
                "type": "Aggregated",
                "nodes": [
                    { "name": "reservoir", "component": "Compensation" },
                    { "name": "reservoir", "component": "Rainfall" },
                    { "name": "bare-reservoir" },
                    { "name": "link", "component": "Loss" },
                    { "name": "store" },
                    { "name": "placeholder" },
                    { "name": "missing" }
                ]
            },
            {
                "meta": { "name": "licence" },
                "type": "VirtualStorage",
                "nodes": [
                    { "name": "works", "component": "Loss" },
                    { "name": "river", "component": "Loss" },
                    { "name": "loss-link", "component": "Loss" }
                ],
                "initial_volume": { "type": "Proportional", "proportion": 1.0 }
            },
            {
                "meta": { "name": "total" },
                "type": "AggregatedStorage",
                "storage_nodes": [
                    { "name": "bare-reservoir", "component": "Rainfall" },
                    { "name": "link" }
                ]
            }
        ]
    }
    "#;

    /// Every invalid member is reported, in the order listed. A reservoir's rainfall needs a
    /// surface area as well.
    #[test]
    fn test_validate_reports_all_invalid_members() {
        let network = parse_network(NETWORK_WITH_INVALID_MEMBERS);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The virtual node `agg` takes the component `Rainfall` of the `Reservoir` node `reservoir`, but that node does not build it. It builds: `Compensation`.",
                "The virtual node `agg` takes the default component `Compensation` of the `Reservoir` node `bare-reservoir`, but that node does not build it. As configured, it builds no components.",
                "The virtual node `agg` takes the component `Loss` of the `Link` node `link`, but that node does not build it. It builds: `Inflow`, `Outflow`.",
                "The virtual node `agg` names the `Storage` node `store`, but nodes of its type have no components for it to take.",
                "The virtual node `licence` takes the component `Loss` of the `WaterTreatmentWorks` node `works`, but that node does not build it. It builds: `Inflow`, `Outflow`.",
                "The virtual node `licence` takes the component `Loss` of the `River` node `river`, but that node does not build it. It builds: `Inflow`, `Outflow`.",
                "The virtual node `licence` takes the component `Loss` of the `LossLink` node `loss-link`, but that node does not build it. It builds: `Inflow`, `Outflow`.",
                "The virtual node `total` names the `Link` node `link`, but an `AggregatedStorage` node takes only storage nodes.",
                // The rainfall that `reservoir` does not build is also a problem with the node.
                "The node `reservoir` is invalid. `rainfall` is set, but it needs a `surface_area`.",
            ]
        );
    }

    /// A network with a metric for every node reference problem, among ones that pass or are
    /// skipped: a default attribute, a placeholder and a node the network does not have.
    const NETWORK_WITH_INVALID_NODE_REFERENCES: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "max_flow": { "type": "Node", "name": "store", "attribute": "Inflow" }
            },
            {
                "meta": { "name": "store" },
                "type": "Storage",
                "initial_volume": { "type": "Proportional", "proportion": 1.0 }
            },
            { "meta": { "name": "link" }, "type": "Link" },
            { "meta": { "name": "placeholder" }, "type": "Placeholder" }
        ],
        "edges": [],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "VirtualStorage",
                "nodes": [],
                "initial_volume": { "type": "Proportional", "proportion": 1.0 }
            },
            { "meta": { "name": "virtual-placeholder" }, "type": "Placeholder" }
        ],
        "parameters": [
            {
                "meta": { "name": "total" },
                "type": "Aggregated",
                "phase": "Before",
                "agg_func": { "type": "Sum" },
                "metrics": [
                    { "type": "Node", "name": "link", "attribute": "Outflow" },
                    { "type": "Node", "name": "link", "attribute": "Volume" },
                    { "type": "Node", "name": "store" },
                    { "type": "VirtualNode", "name": "licence", "attribute": "Volume" },
                    { "type": "VirtualNode", "name": "licence", "attribute": "Inflow" },
                    { "type": "VirtualNode", "name": "virtual-placeholder", "attribute": "Inflow" },
                    { "type": "Node", "name": "placeholder", "attribute": "Volume" },
                    { "type": "Node", "name": "missing", "attribute": "Volume" }
                ]
            },
            {
                "meta": { "name": "indexed" },
                "type": "IndexedArray",
                "phase": "Before",
                "metrics": [{ "type": "Literal", "value": 1.0 }],
                "index_metric": { "type": "Node", "name": "link" }
            }
        ]
    }
    "#;

    /// Every metric reading an attribute its node does not have, and every index metric naming a
    /// node, is reported, in the order listed.
    #[test]
    fn test_validate_reports_all_invalid_node_references() {
        let network = parse_network(NETWORK_WITH_INVALID_NODE_REFERENCES);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The node `supply` reads the attribute `Inflow` of the `Storage` node `store`, but nodes of its type do not have it. Their attributes are: `Volume`, `ProportionalVolume`, `MaxVolume`.",
                "The parameter `total` reads the attribute `Volume` of the `Link` node `link`, but nodes of its type do not have it. Their attributes are: `Inflow`, `Outflow`.",
                "The parameter `total` reads the attribute `Inflow` of the `VirtualStorage` virtual node `licence`, but nodes of its type do not have it. Their attributes are: `Volume`, `ProportionalVolume`.",
                "The parameter `indexed` uses the `Link` node `link` as an index, but nodes give only float values.",
            ]
        );
    }

    /// A network with a parameter reference for every problem, among references from both kinds
    /// of metric that pass or are skipped.
    const NETWORK_WITH_INVALID_PARAMETER_REFERENCES: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "parameters": [
                    { "meta": { "name": "local-flow" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
                    {
                        "meta": { "name": "local-indexed" },
                        "type": "IndexedArray",
                        "phase": "Before",
                        "metrics": [{ "type": "Literal", "value": 1.0 }],
                        "index_metric": { "type": "LocalParameter", "name": "local-flow" }
                    }
                ],
                "max_flow": { "type": "LocalParameter", "name": "local-flow" }
            }
        ],
        "edges": [],
        "parameters": [
            { "meta": { "name": "flow" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
            {
                "meta": { "name": "switch" },
                "type": "AsymmetricSwitchIndex",
                "on_index_metric": { "type": "Constant", "value": 1 },
                "off_index_metric": { "type": "Constant", "value": 0 }
            },
            {
                "meta": { "name": "dict" },
                "type": "Python",
                "source": { "type": "Path", "path": "dict.py" },
                "object": { "type": "Class", "class": "Dict" },
                "return_type": "Dict"
            },
            { "meta": { "name": "placeholder" }, "type": "Placeholder" },
            {
                "meta": { "name": "indexed" },
                "type": "IndexedArray",
                "phase": "Before",
                "metrics": [
                    { "type": "Parameter", "name": "flow" },
                    { "type": "Parameter", "name": "switch" },
                    { "type": "Parameter", "name": "dict" },
                    { "type": "Parameter", "name": "dict", "key": "a" },
                    { "type": "Parameter", "name": "flow", "key": "a" },
                    { "type": "Parameter", "name": "placeholder" },
                    { "type": "Parameter", "name": "missing" }
                ],
                "index_metric": { "type": "Parameter", "name": "flow" }
            },
            {
                "meta": { "name": "agg-index" },
                "type": "AggregatedIndex",
                "phase": "Before",
                "agg_func": { "type": "Sum" },
                "metrics": [
                    { "type": "Parameter", "name": "switch" },
                    { "type": "Parameter", "name": "dict", "key": "a" },
                    { "type": "LocalParameter", "node": "supply", "name": "local-flow" },
                    { "type": "LocalParameter", "name": "local-flow" }
                ]
            }
        ]
    }
    "#;

    /// Every parameter reference of the wrong kind is reported, in the order listed. A local
    /// reference without a `node` resolves in the node holding it.
    #[test]
    fn test_validate_reports_all_invalid_parameter_references() {
        let network = parse_network(NETWORK_WITH_INVALID_PARAMETER_REFERENCES);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The node `supply` uses the local parameter `local-flow` of `supply` as an index, but it gives a float value.",
                "The parameter `indexed` refers to the parameter `dict` without a key, but it gives several values, one per key.",
                "The parameter `indexed` names the key `a` of the parameter `flow`, but it gives a single value and takes no key.",
                "The parameter `indexed` uses the parameter `flow` as an index, but it gives a float value.",
                "The parameter `agg-index` uses the local parameter `local-flow` of `supply` as an index, but it gives a float value.",
            ]
        );
    }

    /// A network whose references ask parameters for values they do not calculate, among ones
    /// that pass or are not checked.
    const NETWORK_WITH_PARAMETER_VALUES_NOT_CALCULATED: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "parameters": [
                    {
                        "meta": { "name": "local-after" },
                        "type": "Aggregated",
                        "phase": "After",
                        "agg_func": { "type": "Sum" },
                        "metrics": [{ "type": "Literal", "value": 1.0 }]
                    }
                ],
                "max_flow": { "type": "LocalParameter", "name": "local-after" },
                "cost": { "type": "Parameter", "name": "constant", "return_value": "After" }
            }
        ],
        "edges": [],
        "parameters": [
            { "meta": { "name": "constant" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
            {
                "meta": { "name": "after" },
                "type": "Aggregated",
                "phase": "After",
                "agg_func": { "type": "Sum" },
                "metrics": [{ "type": "Literal", "value": 1.0 }]
            },
            {
                "meta": { "name": "class" },
                "type": "Python",
                "source": { "type": "Path", "path": "custom.py" },
                "object": { "type": "Class", "class": "Custom" }
            },
            {
                "meta": { "name": "function" },
                "type": "Python",
                "source": { "type": "Path", "path": "custom.py" },
                "object": { "type": "Function", "function": "custom" }
            },
            {
                "meta": { "name": "total" },
                "type": "Aggregated",
                "phase": "After",
                "agg_func": { "type": "Sum" },
                "metrics": [
                    { "type": "Parameter", "name": "after" },
                    { "type": "Parameter", "name": "constant", "return_value": "AfterOrElseInitial" },
                    { "type": "Parameter", "name": "function", "return_value": "After" },
                    { "type": "Parameter", "name": "class", "return_value": "After" },
                    { "type": "Parameter", "name": "constant", "return_value": "Both" },
                    { "type": "Parameter", "name": "after", "return_value": "After" }
                ]
            }
        ]
    }
    "#;

    /// Every reference asking a parameter for a value it does not calculate is reported, in the
    /// order listed, whether the value is set or the default. A Python class's phase is decided
    /// when it is built and `Both` is not checked, so neither is reported.
    #[test]
    fn test_validate_reports_all_parameter_values_not_calculated() {
        let network = parse_network(NETWORK_WITH_PARAMETER_VALUES_NOT_CALCULATED);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The node `supply` asks the local parameter `local-after` of `supply` for its `Before` value, but it is calculated only in the `After` phase.",
                "The node `supply` asks the parameter `constant` for its `After` value, but it is calculated only in the `Before` phase.",
                "The parameter `total` asks the parameter `after` for its `Before` value, but it is calculated only in the `After` phase.",
                "The parameter `total` asks the parameter `constant` for its `AfterOrElseInitial` value, but it is calculated only in the `Before` phase.",
                "The parameter `total` asks the parameter `function` for its `After` value, but it is calculated only in the `Before` phase.",
            ]
        );
    }

    /// A network with every table problem, including a wrong-type reference from each type that
    /// can hold one, among references that pass or are skipped.
    const NETWORK_WITH_TABLE_PROBLEMS: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "max_flow": { "type": "Table", "table": "arrays", "row": "a" },
                "cost": { "type": "Table", "table": "grid", "row": "a" }
            }
        ],
        "edges": [],
        "tables": [
            { "meta": { "name": "scalars" }, "type": "Scalar", "format": "CSV", "lookup": { "type": "Row", "cols": 1 }, "url": "scalars.csv" },
            { "meta": { "name": "arrays" }, "type": "Array", "format": "CSV", "lookup": { "type": "Col", "rows": 1 }, "url": "arrays.csv" },
            { "meta": { "name": "grid" }, "type": "Scalar", "format": "CSV", "lookup": { "type": "Both", "rows": 1, "cols": 1 }, "url": "grid.csv" },
            { "meta": { "name": "array-grid" }, "type": "Array", "format": "CSV", "lookup": { "type": "Both", "rows": 1, "cols": 1 }, "url": "array-grid.csv" },
            { "meta": { "name": "deep" }, "type": "Scalar", "format": "CSV", "lookup": { "type": "Row", "cols": 5 }, "url": "deep.csv" },
            { "meta": { "name": "placeholder" }, "format": "Placeholder" }
        ],
        "parameters": [
            { "meta": { "name": "constant" }, "type": "Constant", "value": { "type": "Table", "table": "arrays", "row": "a" } },
            { "meta": { "name": "profile" }, "type": "MonthlyProfile", "values": { "type": "Table", "table": "scalars", "row": "a" } },
            {
                "meta": { "name": "indexed" },
                "type": "IndexedArray",
                "phase": "Before",
                "metrics": [
                    { "type": "Table", "table": "grid", "row": ["a", "x"] },
                    { "type": "Table", "table": "grid", "row": "a", "column": "" },
                    { "type": "Table", "table": "deep", "row": "a" },
                    { "type": "Table", "table": "placeholder" },
                    { "type": "Table", "table": "missing" }
                ],
                "index_metric": { "type": "Table", "table": "array-grid" }
            }
        ]
    }
    "#;

    /// Every table pywr cannot load is reported, then every table reference that does not fit.
    #[test]
    fn test_validate_reports_all_table_problems() {
        let network = parse_network(NETWORK_WITH_TABLE_PROBLEMS);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The table `array-grid` cannot be loaded. A table of `Array` values must have a `Row` or `Col` lookup, not `Both`.",
                "The table `deep` cannot be loaded. A `Row` lookup's `cols`, the number of index columns, must be 1 to 4, not 5.",
                "The node `supply` has an invalid reference to the table `arrays`. The table holds `Array` values, but `Scalar` values are read from it.",
                "The node `supply` has an invalid reference to the table `grid`. The table's key takes 2 label(s), but the reference gives 1.",
                "The parameter `constant` has an invalid reference to the table `arrays`. The table holds `Array` values, but `Scalar` values are read from it.",
                "The parameter `profile` has an invalid reference to the table `scalars`. The table holds `Scalar` values, but `Array` values are read from it.",
                "The parameter `indexed` has an invalid reference to the table `grid`. The reference contains an empty label at index 1 of its key.",
                "The parameter `indexed` has an invalid reference to the table `array-grid`. The table holds `Array` values, but `Scalar` values are read from it.",
            ]
        );
    }

    /// A network with a node, a virtual node and a parameter breaking each kind of rule.
    const NETWORK_WITH_INVALID_COMPONENTS: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "parameters": [
                    {
                        "meta": { "name": "drawdown" },
                        "type": "UniformDrawdownProfile",
                        "reset_day": { "type": "Literal", "value": 30 },
                        "reset_month": { "type": "Literal", "value": 2 }
                    }
                ]
            },
            {
                "meta": { "name": "loss" },
                "type": "LossLink",
                "loss_factor": { "type": "Gross", "factor": { "type": "Literal", "value": 1.0 } }
            }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "VirtualStorage",
                "nodes": [],
                "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                "reset": { "type": "Annual", "day": 30, "month": 2 },
                "parameters": [
                    {
                        "meta": { "name": "refill" },
                        "type": "UniformDrawdownProfile",
                        "reset_day": { "type": "Literal", "value": 31 },
                        "reset_month": { "type": "Literal", "value": 4 }
                    }
                ]
            }
        ],
        "edges": [],
        "parameters": [
            {
                "meta": { "name": "curve" },
                "type": "ControlCurve",
                "phase": "Before",
                "control_curves": [{ "type": "Literal", "value": 0.5 }],
                "storage_metric": { "type": "Literal", "value": 0.5 },
                "values": [{ "type": "Literal", "value": 1.0 }]
            },
            {
                "meta": { "name": "interpolated" },
                "type": "Interpolated",
                "phase": "Before",
                "x": { "type": "Literal", "value": 0.5 },
                "xp": [{ "type": "Literal", "value": 0.0 }, { "type": "Literal", "value": 1.0 }],
                "fp": [{ "type": "Literal", "value": 0.0 }]
            },
            {
                "meta": { "name": "profile" },
                "type": "DailyProfile",
                "values": { "type": "Literal", "values": [1.0, 2.0, 3.0] }
            }
        ],
        "metric_sets": [
            {
                "meta": { "name": "outputs" },
                "metrics": [{ "type": "Node", "name": "loss" }, { "type": "Literal", "value": 1.0 }]
            }
        ]
    }
    "#;

    /// Every component with invalid fields is reported: each node then its local parameters, each
    /// virtual node then its own, then the network's parameters, then the metric sets.
    #[test]
    fn test_validate_reports_all_invalid_components() {
        let network = parse_network(NETWORK_WITH_INVALID_COMPONENTS);

        let messages: Vec<String> = expect_problems(&network).iter().map(ToString::to_string).collect();

        assert_eq!(
            messages,
            vec![
                "The local parameter `drawdown` of `supply` is invalid. `reset_day` 30 and `reset_month` 2 do not make a date.",
                "The node `loss` is invalid. A `Gross` `loss_factor` must be from 0 up to 1, 1 excluded.",
                "The virtual node `licence` is invalid. `day` 30 and `month` 2 do not make a date.",
                "The local parameter `refill` of `licence` is invalid. `reset_day` 31 and `reset_month` 4 do not make a date.",
                "The parameter `curve` is invalid. `values` has 1 entry(s), but the control curves require 2.",
                "The parameter `interpolated` is invalid. The points in `xp` and `fp` cannot be interpolated between. There are 2 x value(s) but 1 y value(s), and each point needs one of each.",
                "The parameter `profile` is invalid. `values` has 3 entry(s), but the profile takes 365 or 366.",
                "The metric set `outputs` is invalid. The metric at index 1 of `metrics` is a literal, which has no name to be recorded under. Use a `Constant` parameter instead.",
            ]
        );
    }

    /// A duplicated name does not stop the edges being checked: both problems are reported, the
    /// duplicate first.
    #[test]
    fn test_validate_reports_duplicate_names_and_edges_together() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "link" }, "type": "Link" }
                ],
                "edges": [
                    { "from_node": "link", "to_node": "missing" }
                ]
            }
            "#,
        );

        let problems = expect_problems(&network);

        assert_eq!(
            problems,
            vec![
                NetworkProblem::DuplicateNodeName(DuplicateNodeName {
                    name: "link".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                }),
                NetworkProblem::InvalidEdge(EdgeValidationError {
                    edge: network.edges[0].clone(),
                    problem: EdgeProblem::UnknownToNode("missing".to_string()),
                }),
            ]
        );

        assert_eq!(
            network.validate().unwrap_err().report().to_string(),
            "The network has 2 problem(s):\n\
             - The name `link` is used by 2 node(s) and 0 virtual node(s), but each name must be unique.\n\
             - The edge `link->missing` is invalid. There is no node named `missing` to connect to."
        );
    }

    /// Every list is checked, and every duplicate is reported in the documented order. A
    /// placeholder entry counts like any other, and a name shared across lists is not a duplicate.
    #[test]
    fn test_validate_reports_duplicate_names_in_every_list() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "shared" }, "type": "Link" }
                ],
                "edges": [
                    { "from_node": "link", "to_node": "missing" }
                ],
                "parameters": [
                    { "meta": { "name": "p2" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
                    { "meta": { "name": "p1" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
                    { "meta": { "name": "p2" }, "type": "Placeholder" },
                    { "meta": { "name": "p1" }, "type": "Constant", "value": { "type": "Literal", "value": 2.0 } },
                    { "meta": { "name": "p2" }, "type": "Constant", "value": { "type": "Literal", "value": 3.0 } },
                    { "meta": { "name": "shared" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } }
                ],
                "tables": [
                    { "meta": { "name": "tbl" }, "format": "Placeholder" },
                    { "meta": { "name": "tbl" }, "type": "Scalar", "format": "CSV", "lookup": { "type": "Row", "cols": 1 }, "url": "tbl.csv" },
                    { "meta": { "name": "shared" }, "format": "Placeholder" }
                ],
                "time_series": [
                    { "meta": { "name": "ts" }, "type": "Polars", "time_col": "date", "path": "ts.csv" },
                    { "meta": { "name": "ts" }, "type": "Placeholder" },
                    { "meta": { "name": "shared" }, "type": "Placeholder" }
                ],
                "metric_sets": [
                    { "meta": { "name": "ms" }, "filters": { "all_nodes": true } },
                    { "meta": { "name": "ms" }, "filters": { "all_virtual_nodes": true } },
                    { "meta": { "name": "shared" }, "filters": { "all_nodes": true } }
                ]
            }
            "#,
        );

        let problems = expect_problems(&network);

        assert_eq!(
            problems,
            vec![
                NetworkProblem::DuplicateNodeName(DuplicateNodeName {
                    name: "link".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                }),
                NetworkProblem::DuplicateParameterName {
                    name: "p1".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateParameterName {
                    name: "p2".to_string(),
                    count: 3,
                },
                NetworkProblem::DuplicateTableName {
                    name: "tbl".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateTimeSeriesName {
                    name: "ts".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateMetricSetName {
                    name: "ms".to_string(),
                    count: 2,
                },
                NetworkProblem::InvalidEdge(EdgeValidationError {
                    edge: network.edges[0].clone(),
                    problem: EdgeProblem::UnknownToNode("missing".to_string()),
                }),
            ]
        );

        assert_eq!(
            network.validate().unwrap_err().report().to_string(),
            "The network has 7 problem(s):\n\
             - The name `link` is used by 2 node(s) and 0 virtual node(s), but each name must be unique.\n\
             - The name `p1` is used by 2 parameters, but each name must be unique.\n\
             - The name `p2` is used by 3 parameters, but each name must be unique.\n\
             - The name `tbl` is used by 2 tables, but each name must be unique.\n\
             - The name `ts` is used by 2 time series, but each name must be unique.\n\
             - The name `ms` is used by 2 metric sets, but each name must be unique.\n\
             - The edge `link->missing` is invalid. There is no node named `missing` to connect to."
        );
    }

    /// However many problems there are, `Display` stays a single line, while the report lists
    /// every one of them.
    #[test]
    fn test_validate_display_summarises_and_report_lists_every_problem() {
        let count = 13;
        let edges = (0..count)
            .map(|i| format!(r#"{{ "from_node": "link", "to_node": "missing-{i:02}" }}"#))
            .collect::<Vec<_>>()
            .join(", ");

        let network = parse_network(&format!(
            r#"{{ "nodes": [{{ "meta": {{ "name": "link" }}, "type": "Link" }}], "edges": [{edges}] }}"#
        ));

        let error = network.validate().unwrap_err();

        assert_eq!(error.to_string(), "The network has 13 problem(s).");

        // The summary, then one line per problem, down to the last edge listed.
        let report = error.report().to_string();
        let lines: Vec<&str> = report.lines().collect();

        assert_eq!(lines.len(), 1 + count);
        assert_eq!(lines[0], "The network has 13 problem(s):");
        assert!(lines[count].contains("`missing-12`"));
    }
}
