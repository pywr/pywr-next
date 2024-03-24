use crate::metric::MetricF64;
use crate::network::Network;
use crate::node::{Constraint, ConstraintValue, FlowConstraints, NodeMeta};
use crate::state::State;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedNodeIndex(usize);

impl Deref for AggregatedNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct AggregatedNodeVec {
    nodes: Vec<AggregatedNode>,
}

impl Deref for AggregatedNodeVec {
    type Target = Vec<AggregatedNode>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for AggregatedNodeVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl AggregatedNodeVec {
    pub fn get(&self, index: &AggregatedNodeIndex) -> Result<&AggregatedNode, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &AggregatedNodeIndex) -> Result<&mut AggregatedNode, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<Factors>,
    ) -> AggregatedNodeIndex {
        let node_index = AggregatedNodeIndex(self.nodes.len());
        let node = AggregatedNode::new(&node_index, name, sub_name, nodes, factors);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, PartialEq)]
pub enum Factors {
    Proportion(Vec<MetricF64>),
    Ratio(Vec<MetricF64>),
}

#[derive(Debug, PartialEq)]
pub struct AggregatedNode {
    meta: NodeMeta<AggregatedNodeIndex>,
    flow_constraints: FlowConstraints,
    nodes: Vec<NodeIndex>,
    factors: Option<Factors>,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeFactor {
    pub index: NodeIndex,
    pub factor: f64,
}

impl NodeFactor {
    fn new(node: NodeIndex, factor: f64) -> Self {
        Self { index: node, factor }
    }
}

/// A pair of nodes and their factors
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeFactorPair {
    pub node0: NodeFactor,
    pub node1: NodeFactor,
}

impl NodeFactorPair {
    fn new(node0: NodeFactor, node1: NodeFactor) -> Self {
        Self { node0, node1 }
    }

    /// Return the ratio of the two factors (node0 / node1)
    pub fn ratio(&self) -> f64 {
        self.node0.factor / self.node1.factor
    }
}

impl AggregatedNode {
    pub fn new(
        index: &AggregatedNodeIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<Factors>,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes: nodes.to_vec(),
            factors,
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

    pub fn index(&self) -> AggregatedNodeIndex {
        *self.meta.index()
    }

    pub fn get_nodes(&self) -> Vec<NodeIndex> {
        self.nodes.to_vec()
    }

    pub fn set_factors(&mut self, factors: Option<Factors>) {
        self.factors = factors;
    }

    pub fn get_factors(&self) -> Option<&Factors> {
        self.factors.as_ref()
    }

    pub fn get_factor_node_pairs(&self) -> Option<Vec<(NodeIndex, NodeIndex)>> {
        if self.factors.is_some() {
            let n0 = self.nodes[0];

            Some(self.nodes.iter().skip(1).map(|&n1| (n0, n1)).collect::<Vec<_>>())
        } else {
            None
        }
    }

    /// Return normalised factor pairs
    ///
    pub fn get_norm_factor_pairs(&self, model: &Network, state: &State) -> Option<Vec<NodeFactorPair>> {
        if let Some(factors) = &self.factors {
            let pairs = match factors {
                Factors::Proportion(prop_factors) => {
                    get_norm_proportional_factor_pairs(prop_factors, &self.nodes, model, state)
                }
                Factors::Ratio(ratio_factors) => get_norm_ratio_factor_pairs(ratio_factors, &self.nodes, model, state),
            };
            Some(pairs)
        } else {
            None
        }
    }

    pub fn set_min_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    pub fn get_min_flow_constraint(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(model, state)
    }
    pub fn set_max_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    pub fn get_max_flow_constraint(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(model, state)
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value),
            Constraint::MaxFlow => self.set_max_flow_constraint(value),
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value.clone());
                self.set_max_flow_constraint(value);
            }
            Constraint::MinVolume => return Err(PywrError::StorageConstraintsUndefined),
            Constraint::MaxVolume => return Err(PywrError::StorageConstraintsUndefined),
        }
        Ok(())
    }

    pub fn get_current_min_flow(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(model, state)
    }

    pub fn get_current_max_flow(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(model, state)
    }

    pub fn get_current_flow_bounds(&self, model: &Network, state: &State) -> Result<(f64, f64), PywrError> {
        match (
            self.get_current_min_flow(model, state),
            self.get_current_max_flow(model, state),
        ) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn default_metric(&self) -> MetricF64 {
        MetricF64::AggregatedNodeInFlow(self.index())
    }
}

/// Proportional factors
fn get_norm_proportional_factor_pairs(
    factors: &[MetricF64],
    nodes: &[NodeIndex],
    model: &Network,
    state: &State,
) -> Vec<NodeFactorPair> {
    if factors.len() != nodes.len() - 1 {
        panic!("Found {} proportional factors and {} nodes in aggregated node. The number of proportional factors should equal one less than the number of nodes.", factors.len(), nodes.len());
    }

    // First get the current factor values
    let values: Vec<f64> = factors
        .iter()
        .map(|f| f.get_value(model, state))
        .collect::<Result<Vec<_>, PywrError>>()
        .expect("Failed to get current factor values.");

    // TODO do we need to assert that each individual factor is positive?
    let total: f64 = values.iter().sum();
    if total < 0.0 {
        panic!("Proportional factors are too small or negative.");
    }
    if total >= 1.0 {
        panic!("Proportional factors are too large.")
    }

    let f0 = 1.0 - total;
    let n0 = nodes[0];

    nodes
        .iter()
        .skip(1)
        .zip(values)
        .map(move |(&n1, f1)| NodeFactorPair::new(NodeFactor::new(n0, f0), NodeFactor::new(n1, f1)))
        .collect::<Vec<_>>()
}

/// Ratio factors
fn get_norm_ratio_factor_pairs(
    factors: &[MetricF64],
    nodes: &[NodeIndex],
    model: &Network,
    state: &State,
) -> Vec<NodeFactorPair> {
    if factors.len() != nodes.len() {
        panic!("Found {} ratio factors and {} nodes in aggregated node. The number of ratio factors should equal the number of nodes.", factors.len(), nodes.len());
    }

    let n0 = nodes[0];
    let f0 = factors[0].get_value(model, state).unwrap();

    nodes
        .iter()
        .zip(factors)
        .skip(1)
        .map(move |(&n1, f1)| {
            NodeFactorPair::new(
                NodeFactor::new(n0, f0),
                NodeFactor::new(n1, f1.get_value(model, state).unwrap()),
            )
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use crate::aggregated_node::Factors;
    use crate::metric::MetricF64;
    use crate::models::Model;
    use crate::network::Network;
    use crate::node::ConstraintValue;
    use crate::recorders::AssertionRecorder;
    use crate::test_utils::{default_time_domain, run_all_solvers};
    use ndarray::Array2;

    /// Test the factors forcing a simple ratio of flow
    ///
    /// The model has a single input that diverges to two links and respective output nodes.
    #[test]
    fn test_simple_factors() {
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

        let factors = Some(Factors::Ratio(vec![MetricF64::Constant(2.0), MetricF64::Constant(1.0)]));

        let _agg_node = network.add_aggregated_node("agg-node", None, &[link_node0, link_node1], factors);

        // Setup a demand on output-0
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node
            .set_max_flow_constraint(ConstraintValue::Scalar(100.0))
            .unwrap();

        output_node.set_cost(ConstraintValue::Scalar(-10.0));

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionRecorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionRecorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model);
    }
}
