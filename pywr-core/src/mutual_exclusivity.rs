use crate::node::NodeMeta;
use crate::{NodeIndex, PywrError};
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MutualExclusivityNodeIndex(usize);

impl Deref for MutualExclusivityNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct MutualExclusivityNodeVec {
    nodes: Vec<MutualExclusivityNode>,
}

impl Deref for MutualExclusivityNodeVec {
    type Target = Vec<MutualExclusivityNode>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for MutualExclusivityNodeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl MutualExclusivityNodeVec {
    pub fn get(&self, index: &MutualExclusivityNodeIndex) -> Result<&MutualExclusivityNode, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &MutualExclusivityNodeIndex) -> Result<&mut MutualExclusivityNode, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        min_active: usize,
        max_active: usize,
    ) -> MutualExclusivityNodeIndex {
        let node_index = MutualExclusivityNodeIndex(self.nodes.len());
        let node = MutualExclusivityNode::new(&node_index, name, sub_name, nodes, min_active, max_active);
        self.nodes.push(node);
        node_index
    }
}

/// A node that represents an exclusivity constraint between a set of nodes.
///
/// The constraint operates over a set of node indices, and will ensure that `min_active` to
/// `max_active` (inclusive) nodes are active. By itself this will not require that an
/// "active" node is utilised.
#[derive(Debug, PartialEq)]
pub struct MutualExclusivityNode {
    // Meta data
    meta: NodeMeta<MutualExclusivityNodeIndex>,
    // The set of node indices that are constrained
    nodes: HashSet<NodeIndex>,
    // The minimum number of nodes that must be active
    min_active: usize,
    // The maximum number of nodes that can be active
    max_active: usize,
}

impl MutualExclusivityNode {
    pub fn new(
        index: &MutualExclusivityNodeIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        min_active: usize,
        max_active: usize,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            nodes: nodes.iter().copied().collect(),
            min_active,
            max_active,
        }
    }
    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    pub fn index(&self) -> MutualExclusivityNodeIndex {
        *self.meta.index()
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = &NodeIndex> {
        self.nodes.iter()
    }

    pub fn min_active(&self) -> usize {
        self.min_active
    }

    pub fn max_active(&self) -> usize {
        self.max_active
    }
}

#[cfg(test)]
mod tests {
    use crate::metric::MetricF64;
    use crate::models::Model;
    use crate::network::Network;
    use crate::node::ConstraintValue;
    use crate::recorders::AssertionRecorder;
    use crate::test_utils::{default_time_domain, run_all_solvers};
    use ndarray::Array2;

    /// Test mutual exclusive flows
    ///
    /// The model has a single input that diverges to two links, only one of which can be active at a time.
    #[test]
    fn test_simple_mutual_exclusivity() {
        let mut network = Network::default();

        let input_node = network.add_input_node("input", None).unwrap();
        let link_node0 = network.add_link_node("link", Some("0")).unwrap();
        let output_node0 = network.add_output_node("output", Some("0")).unwrap();

        network.connect_nodes(input_node, link_node0).unwrap();
        network.connect_nodes(link_node0, output_node0).unwrap();

        let link_node1 = network.add_link_node("link", Some("1")).unwrap();
        let output_node1 = network.add_output_node("output", Some("1")).unwrap();

        network.connect_nodes(input_node, link_node1).unwrap();
        network.connect_nodes(link_node1, output_node1).unwrap();

        let _me_node = network.add_mutual_exclusivity_node("mutual-exclusivity", None, &[link_node0, link_node1], 0, 1);

        // Setup a demand on output-0 and output-1.
        // output-0 has a lower penalty cost than output-1, so the flow should be directed to output-0.
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node
            .set_max_flow_constraint(ConstraintValue::Scalar(100.0))
            .unwrap();

        output_node.set_cost(ConstraintValue::Scalar(-10.0));

        let output_node = network.get_mut_node_by_name("output", Some("1")).unwrap();
        output_node
            .set_max_flow_constraint(ConstraintValue::Scalar(100.0))
            .unwrap();

        output_node.set_cost(ConstraintValue::Scalar(-5.0));

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionRecorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionRecorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model);
    }
}
