use crate::metric::MetricF64;
use crate::network::Network;
use crate::node::{Constraint, FlowConstraints, NodeMeta};
use crate::state::{ConstParameterValues, State};
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

impl Factors {
    /// Returns true if all factors are constant
    pub fn is_constant(&self) -> bool {
        match self {
            Factors::Proportion(factors) => factors.iter().all(|f| f.is_constant()),
            Factors::Ratio(factors) => factors.iter().all(|f| f.is_constant()),
        }
    }
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

/// A constant node factor. If the factor is non-constant, the factor value here is `None`.
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeConstFactor {
    pub index: NodeIndex,
    pub factor: Option<f64>,
}

impl NodeConstFactor {
    fn new(node: NodeIndex, factor: Option<f64>) -> Self {
        Self { index: node, factor }
    }
}

/// A pair of nodes and their factors
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeConstFactorPair {
    pub node0: NodeConstFactor,
    pub node1: NodeConstFactor,
}

impl NodeConstFactorPair {
    fn new(node0: NodeConstFactor, node1: NodeConstFactor) -> Self {
        Self { node0, node1 }
    }

    /// Return the ratio of the two factors (node0 / node1). If either factor is `None`,
    /// the ratio is also `None`.
    pub fn ratio(&self) -> Option<f64> {
        match (self.node0.factor, self.node1.factor) {
            (Some(f0), Some(f1)) => Some(f0 / f1),
            _ => None,
        }
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
            flow_constraints: FlowConstraints::default(),
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

    /// Does the aggregated node have factors?
    pub fn has_factors(&self) -> bool {
        self.factors.is_some()
    }

    /// Does the aggregated node have constant factors?
    pub fn has_const_factors(&self) -> bool {
        self.factors.as_ref().map(|f| f.is_constant()).unwrap_or(false)
    }
    pub fn set_factors(&mut self, factors: Option<Factors>) {
        self.factors = factors;
    }

    pub fn get_factors(&self) -> Option<&Factors> {
        self.factors.as_ref()
    }

    /// Return normalised factor pairs
    pub fn get_factor_node_pairs(&self) -> Option<Vec<(NodeIndex, NodeIndex)>> {
        if self.factors.is_some() {
            let n0 = self.nodes[0];

            Some(self.nodes.iter().skip(1).map(|&n1| (n0, n1)).collect::<Vec<_>>())
        } else {
            None
        }
    }

    /// Return constant normalised factor pairs
    pub fn get_const_norm_factor_pairs(&self, values: &ConstParameterValues) -> Option<Vec<NodeConstFactorPair>> {
        if let Some(factors) = &self.factors {
            let pairs = match factors {
                Factors::Proportion(prop_factors) => {
                    get_const_norm_proportional_factor_pairs(prop_factors, &self.nodes, values)
                }
                Factors::Ratio(ratio_factors) => get_const_norm_ratio_factor_pairs(ratio_factors, &self.nodes, values),
            };
            Some(pairs)
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

    pub fn set_min_flow_constraint(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.min_flow = value;
    }
    pub fn get_min_flow_constraint(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(model, state)
    }
    pub fn set_max_flow_constraint(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.max_flow = value;
    }
    pub fn get_max_flow_constraint(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(model, state)
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: Option<MetricF64>, constraint: Constraint) -> Result<(), PywrError> {
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

/// Calculate factor pairs for proportional factors.
///
/// There should be one less factor than node indices. The factors correspond to each of the node
/// indices after the first. Factor pairs relating the first index to each of the other indices are
/// calculated. This requires the sum of the factors to be greater than 0.0 and less than 1.0.
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
        .map(|f| {
            let v = f.get_value(model, state)?;
            if v < 0.0 {
                Err(PywrError::NegativeFactor)
            } else {
                Ok(v)
            }
        })
        .collect::<Result<Vec<_>, PywrError>>()
        .expect("Failed to get current factor values. Ensure that all factors are not negative.");

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

/// Calculate constant factor pairs for proportional factors.
///
/// There should be one less factor than node indices. The factors correspond to each of the node
/// indices after the first. Factor pairs relating the first index to each of the other indices are
/// calculated. This requires the sum of the factors to be greater than 0.0 and less than 1.0. If
/// any of the factors are not constant, the factor pairs will contain `None` values.
fn get_const_norm_proportional_factor_pairs(
    factors: &[MetricF64],
    nodes: &[NodeIndex],
    values: &ConstParameterValues,
) -> Vec<NodeConstFactorPair> {
    if factors.len() != nodes.len() - 1 {
        panic!("Found {} proportional factors and {} nodes in aggregated node. The number of proportional factors should equal one less than the number of nodes.", factors.len(), nodes.len());
    }

    // First get the current factor values, ensuring they are all non-negative
    let values: Vec<Option<f64>> = factors
        .iter()
        .map(|f| {
            let v = f.try_get_constant_value(values)?;
            if let Some(v) = v {
                if v < 0.0 {
                    Err(PywrError::NegativeFactor)
                } else {
                    Ok(Some(v))
                }
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>, PywrError>>()
        .expect("Failed to get current factor values. Ensure that all factors are not negative.");

    let n0 = nodes[0];

    // To calculate the factors we require that every factor is available.
    if values.iter().any(|v| v.is_none()) {
        // At least one factor is not available; therefore we can not calculate "f0"
        nodes
            .iter()
            .skip(1)
            .zip(values)
            .map(move |(&n1, f1)| {
                NodeConstFactorPair::new(NodeConstFactor::new(n0, None), NodeConstFactor::new(n1, f1))
            })
            .collect::<Vec<_>>()
    } else {
        // All factors are available; therefore we can calculate "f0"
        let total: f64 = values
            .iter()
            .map(|v| v.expect("Factor is `None`; this should be impossible."))
            .sum();
        if total < 0.0 {
            panic!("Proportional factors are too small or negative.");
        }
        if total >= 1.0 {
            panic!("Proportional factors are too large.")
        }

        let f0 = Some(1.0 - total);

        nodes
            .iter()
            .skip(1)
            .zip(values)
            .map(move |(&n1, f1)| NodeConstFactorPair::new(NodeConstFactor::new(n0, f0), NodeConstFactor::new(n1, f1)))
            .collect::<Vec<_>>()
    }
}

/// Calculate factor pairs for ratio factors.
///
/// The number of node indices and factors should be equal. The factors correspond to each of the
/// node indices. Factor pairs relating the first index to each of the other indices are calculated.
/// This requires that the factors are all non-zero.
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
    if f0 < 0.0 {
        panic!("Negative factor is not allowed");
    }

    nodes
        .iter()
        .zip(factors)
        .skip(1)
        .map(move |(&n1, f1)| {
            let v1 = f1.get_value(model, state)?;
            if v1 < 0.0 {
                Err(PywrError::NegativeFactor)
            } else {
                Ok(NodeFactorPair::new(NodeFactor::new(n0, f0), NodeFactor::new(n1, v1)))
            }
        })
        .collect::<Result<Vec<_>, PywrError>>()
        .expect("Failed to get current factor values. Ensure that all factors are not negative.")
}

/// Constant ratio factors using constant values if they are available. If they are not available,
/// the factors are `None`.
fn get_const_norm_ratio_factor_pairs(
    factors: &[MetricF64],
    nodes: &[NodeIndex],
    values: &ConstParameterValues,
) -> Vec<NodeConstFactorPair> {
    if factors.len() != nodes.len() {
        panic!("Found {} ratio factors and {} nodes in aggregated node. The number of ratio factors should equal the number of nodes.", factors.len(), nodes.len());
    }

    let n0 = nodes[0];
    // Try to convert the factor into a constant

    let f0 = factors[0]
        .try_get_constant_value(values)
        .unwrap_or_else(|e| panic!("Failed to get constant value for factor: {}", e));

    if let Some(v0) = f0 {
        if v0 < 0.0 {
            panic!("Negative factor is not allowed");
        }
    }

    nodes
        .iter()
        .zip(factors)
        .skip(1)
        .map(move |(&n1, f1)| {
            let v1 = f1
                .try_get_constant_value(values)
                .unwrap_or_else(|e| panic!("Failed to get constant value for factor: {}", e));

            if let Some(v) = v1 {
                if v < 0.0 {
                    return Err(PywrError::NegativeFactor);
                }
            }

            Ok(NodeConstFactorPair::new(
                NodeConstFactor::new(n0, f0),
                NodeConstFactor::new(n1, v1),
            ))
        })
        .collect::<Result<Vec<_>, PywrError>>()
        .expect("Failed to get current factor values. Ensure that all factors are not negative.")
}

#[cfg(test)]
mod tests {
    use crate::aggregated_node::Factors;
    use crate::metric::MetricF64;
    use crate::models::Model;
    use crate::network::Network;
    use crate::parameters::MonthlyProfileParameter;
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

        let factors = Some(Factors::Ratio(vec![2.0.into(), 1.0.into()]));

        let _agg_node = network.add_aggregated_node("agg-node", None, &[link_node0, link_node1], factors);

        // Setup a demand on output-0
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-10.0).into()));

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

        run_all_solvers(&model, &[], &[]);
    }

    /// Test the factors forcing a simple ratio of flow that varies over time
    ///
    /// The model has a single input that diverges to two links and respective output nodes.
    #[test]
    fn test_simple_factor_profile() {
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

        let factor_profile = MonthlyProfileParameter::new("factor-profile".into(), [2.0; 12], None);
        let factor_profile_idx = network.add_simple_parameter(Box::new(factor_profile)).unwrap();

        let factors = Some(Factors::Ratio(vec![factor_profile_idx.into(), 1.0.into()]));

        let _agg_node = network.add_aggregated_node("agg-node", None, &[link_node0, link_node1], factors);

        // Setup a demand on output-0
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-10.0).into()));

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

        run_all_solvers(&model, &["cbc"], &[]);
    }
}
