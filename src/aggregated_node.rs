use crate::metric::Metric;
use crate::node::{Constraint, ConstraintValue, FlowConstraints, NodeMeta};
use crate::parameters::FloatValue;
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
        factors: Option<&[f64]>,
    ) -> AggregatedNodeIndex {
        let node_index = AggregatedNodeIndex(self.nodes.len());
        let node = AggregatedNode::new(&node_index, name, sub_name, nodes, factors);
        self.nodes.push(node);
        node_index
    }
}

#[derive(Debug, PartialEq)]
pub struct AggregatedNode {
    meta: NodeMeta<AggregatedNodeIndex>,
    flow_constraints: FlowConstraints,
    nodes: Vec<NodeIndex>,
    factors: Option<Vec<FloatValue>>,
}

impl AggregatedNode {
    pub fn new(
        index: &AggregatedNodeIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<&[f64]>,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes: nodes.to_vec(),
            factors: factors.map(|f| f.iter().map(|f| FloatValue::Constant(*f)).collect()),
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

    pub fn set_factors(&mut self, values: Option<&[FloatValue]>) {
        self.factors = values.map(|f| f.to_vec());
    }

    pub fn get_factors(&self) -> Option<&Vec<FloatValue>> {
        self.factors.as_ref()
    }

    pub fn get_norm_factor_pairs(
        &self,
    ) -> Option<impl Iterator<Item = ((NodeIndex, FloatValue), (NodeIndex, FloatValue))> + '_> {
        if let Some(factors) = &self.factors {
            let n0 = &self.nodes[0];
            let f0 = &factors[0];

            Some(
                self.nodes
                    .iter()
                    .zip(factors)
                    .skip(1)
                    .map(|(n1, f1)| ((*n0, *f0), (*n1, *f1)))
                    .into_iter(),
            )
        } else {
            None
        }
    }

    pub fn set_min_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.min_flow = value;
    }
    pub fn get_min_flow_constraint(&self, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(state)
    }
    pub fn set_max_flow_constraint(&mut self, value: ConstraintValue) {
        self.flow_constraints.max_flow = value;
    }
    pub fn get_max_flow_constraint(&self, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(state)
    }

    /// Set a constraint on a node.
    pub fn set_constraint(&mut self, value: ConstraintValue, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => self.set_min_flow_constraint(value),
            Constraint::MaxFlow => self.set_max_flow_constraint(value),
            Constraint::MinAndMaxFlow => {
                self.set_min_flow_constraint(value);
                self.set_max_flow_constraint(value);
            }
            Constraint::MinVolume => return Err(PywrError::StorageConstraintsUndefined),
            Constraint::MaxVolume => return Err(PywrError::StorageConstraintsUndefined),
        }
        Ok(())
    }

    pub fn get_current_min_flow(&self, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_min_flow(state)
    }

    pub fn get_current_max_flow(&self, state: &State) -> Result<f64, PywrError> {
        self.flow_constraints.get_max_flow(state)
    }

    pub fn get_current_flow_bounds(&self, state: &State) -> Result<(f64, f64), PywrError> {
        match (self.get_current_min_flow(state), self.get_current_max_flow(state)) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    pub fn default_metric(&self) -> Vec<Metric> {
        self.nodes.iter().map(|n| Metric::NodeOutFlow(*n)).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::model::Model;
    use crate::node::ConstraintValue;
    use crate::recorders::AssertionRecorder;
    use crate::solvers::clp::{ClpSolver, HighsSolver};
    use crate::test_utils::{default_scenarios, default_timestepper};
    use ndarray::Array2;

    /// Test the factors forcing a simple ratio of flow
    ///
    /// The model has a single input that diverges to two links and respective output nodes.
    #[test]
    fn test_simple_factors() {
        let mut model = Model::default();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();

        let input_node = model.add_input_node("input", None).unwrap();
        let link_node0 = model.add_link_node("link", Some("0")).unwrap();
        let output_node0 = model.add_output_node("output", Some("0")).unwrap();

        model.connect_nodes(input_node, link_node0).unwrap();
        model.connect_nodes(link_node0, output_node0).unwrap();

        let link_node1 = model.add_link_node("link", Some("1")).unwrap();
        let output_node1 = model.add_output_node("output", Some("1")).unwrap();

        model.connect_nodes(input_node, link_node1).unwrap();
        model.connect_nodes(link_node1, output_node1).unwrap();

        let agg_node = model.add_aggregated_node("agg-node", None, &[link_node0, link_node1], Some(&[2.0, 1.0]));

        // Setup a demand on output-0
        let output_node = model.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node
            .set_max_flow_constraint(ConstraintValue::Scalar(100.0))
            .unwrap();

        output_node.set_cost(ConstraintValue::Scalar(-10.0));

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionRecorder::new("link-0-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionRecorder::new("link-0-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run::<ClpSolver>(&timestepper, &scenarios).unwrap();
    }
}
