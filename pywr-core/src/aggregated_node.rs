#![warn(clippy::pedantic)]
use crate::NodeIndex;
use crate::metric::{ConstantMetricF64Error, MetricF64, MetricF64Error};
use crate::network::Network;
use crate::node::{FlowConstraints, NodeMeta};
use crate::state::{ConstParameterValues, State};
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use thiserror::Error;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct AggregatedNodeIndex(usize);

impl Deref for AggregatedNodeIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for AggregatedNodeIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
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
    #[must_use]
    pub fn get(&self, index: &AggregatedNodeIndex) -> Option<&AggregatedNode> {
        self.nodes.get(index.0)
    }

    pub fn get_mut(&mut self, index: &AggregatedNodeIndex) -> Option<&mut AggregatedNode> {
        self.nodes.get_mut(index.0)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[Vec<NodeIndex>],
        relationship: Option<Relationship>,
    ) -> AggregatedNodeIndex {
        let node_index = AggregatedNodeIndex(self.nodes.len());
        let node = AggregatedNode::new(node_index, name, sub_name, nodes, relationship);
        self.nodes.push(node);
        node_index
    }
}

/// Factors relating node flows in an aggregated node.
#[derive(Debug, PartialEq)]
pub enum Factors {
    /// Proportional factors require that the sum of the factors is less than 1.0, and that
    /// all factors are non-negative. The first node in the aggregated node has an implicit
    /// factor of `1.0 - sum(factors)`. Therefore, there should be one less factor than nodes.
    Proportion { factors: Vec<MetricF64> },
    /// Ratio factors require that all factors are non-negative. There should be the same
    /// number of factors as nodes.
    Ratio { factors: Vec<MetricF64> },
    /// Linear combination of node flows. The factors can be positive or negative, and a
    /// right-hand side (rhs) value can be provided. There should be the same number of
    /// factors as nodes. Currently only two nodes are supported.
    Coefficients {
        factors: Vec<MetricF64>,
        rhs: Option<MetricF64>,
    },
}

impl Factors {
    /// Returns true if all factors and any right-hands sides are constant
    #[must_use]
    pub fn is_constant(&self) -> bool {
        match self {
            Self::Proportion { factors } | Self::Ratio { factors } => factors.iter().all(MetricF64::is_constant),
            Self::Coefficients { factors, rhs } => {
                factors.iter().all(MetricF64::is_constant) && rhs.as_ref().is_none_or(MetricF64::is_constant)
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Exclusivity {
    // The minimum number of nodes that must be active
    min_active: u64,
    // The maximum number of nodes that can be active
    max_active: u64,
}

impl Exclusivity {
    #[must_use]
    pub fn min_active(&self) -> u64 {
        self.min_active
    }

    #[must_use]
    pub fn max_active(&self) -> u64 {
        self.max_active
    }
}

/// Additional relationship between nodes in an aggregated node.
#[derive(Debug, PartialEq)]
pub enum Relationship {
    /// Node flows are related to on another by a set of factors.
    Factored(Factors),
    /// Node flows are mutually exclusive.
    Exclusive(Exclusivity),
}

impl Relationship {
    #[must_use]
    pub fn new_ratio_factors(factors: &[MetricF64]) -> Self {
        Relationship::Factored(Factors::Ratio {
            factors: factors.to_vec(),
        })
    }

    #[must_use]
    pub fn new_proportion_factors(factors: &[MetricF64]) -> Self {
        Relationship::Factored(Factors::Proportion {
            factors: factors.to_vec(),
        })
    }

    #[must_use]
    pub fn new_coefficient_factors(factors: &[MetricF64], rhs: Option<MetricF64>) -> Self {
        Relationship::Factored(Factors::Coefficients {
            factors: factors.to_vec(),
            rhs,
        })
    }

    #[must_use]
    pub fn new_exclusive(min_active: u64, max_active: u64) -> Self {
        Relationship::Exclusive(Exclusivity { min_active, max_active })
    }
}

#[derive(Debug, Error)]
pub enum AggregatedNodeError {
    #[error("Flow constraints are undefined for this node type")]
    FlowConstraintsUndefined,
    #[error("Storage constraints are undefined for this node type")]
    StorageConstraintsUndefined,
    #[error("Negative factor is not allowed")]
    NegativeFactor,
}

#[derive(Debug, PartialEq)]
pub struct AggregatedNode {
    meta: NodeMeta<AggregatedNodeIndex>,
    flow_constraints: FlowConstraints,
    nodes: Vec<Vec<NodeIndex>>,
    relationship: Option<Relationship>,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeFactor<'a> {
    indices: &'a [NodeIndex],
    factor: f64,
}

impl<'a> NodeFactor<'a> {
    fn new(indices: &'a [NodeIndex], factor: f64) -> Self {
        Self { indices, factor }
    }
}

/// A pair of nodes and their factors
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeFactorPair<'a> {
    node0: NodeFactor<'a>,
    node1: NodeFactor<'a>,
    rhs: f64,
}

impl<'a> NodeFactorPair<'a> {
    fn new(node0: NodeFactor<'a>, node1: NodeFactor<'a>, rhs: f64) -> Self {
        Self { node0, node1, rhs }
    }

    #[must_use]
    pub fn node0_indices(&self) -> &[NodeIndex] {
        self.node0.indices
    }
    #[must_use]
    pub fn node0_factor(&self) -> f64 {
        self.node0.factor
    }

    #[must_use]
    pub fn node1_indices(&self) -> &[NodeIndex] {
        self.node1.indices
    }
    #[must_use]
    pub fn node1_factor(&self) -> f64 {
        self.node1.factor
    }

    #[must_use]
    pub fn rhs(&self) -> f64 {
        self.rhs
    }
}

/// A constant node factor. If the factor is non-constant, the factor value here is `None`.
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeConstFactor<'a> {
    indices: &'a [NodeIndex],
    factor: Option<f64>,
}

impl<'a> NodeConstFactor<'a> {
    fn new(indices: &'a [NodeIndex], factor: Option<f64>) -> Self {
        Self { indices, factor }
    }
}

/// A pair of nodes and their factors
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct NodeConstFactorPair<'a> {
    node0: NodeConstFactor<'a>,
    node1: NodeConstFactor<'a>,
    rhs: f64,
}

impl<'a> NodeConstFactorPair<'a> {
    fn new(node0: NodeConstFactor<'a>, node1: NodeConstFactor<'a>, rhs: f64) -> Self {
        Self { node0, node1, rhs }
    }

    #[must_use]
    pub fn node0_indices(&self) -> &[NodeIndex] {
        self.node0.indices
    }
    #[must_use]
    pub fn node0_factor(&self) -> Option<f64> {
        self.node0.factor
    }

    #[must_use]
    pub fn node1_indices(&self) -> &[NodeIndex] {
        self.node1.indices
    }
    #[must_use]
    pub fn node1_factor(&self) -> Option<f64> {
        self.node1.factor
    }

    #[must_use]
    pub fn rhs(&self) -> f64 {
        self.rhs
    }
}

impl AggregatedNode {
    #[must_use]
    pub fn new(
        index: AggregatedNodeIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[Vec<NodeIndex>],
        relationship: Option<Relationship>,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::default(),
            nodes: nodes.to_vec(),
            relationship,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub-name
    #[must_use]
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    #[must_use]
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    #[must_use]
    pub fn index(&self) -> AggregatedNodeIndex {
        *self.meta.index()
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = &[NodeIndex]> {
        self.nodes.iter().map(Vec::as_slice)
    }

    /// Does the aggregated node have a mutual exclusivity relationship?
    #[must_use]
    pub fn has_exclusivity(&self) -> bool {
        self.relationship
            .as_ref()
            .is_some_and(|r| matches!(r, Relationship::Exclusive(_)))
    }

    /// Does the aggregated node have factors?
    #[must_use]
    pub fn has_factors(&self) -> bool {
        self.relationship
            .as_ref()
            .is_some_and(|r| matches!(r, Relationship::Factored(_)))
    }

    /// Does the aggregated node have constant factors?
    #[must_use]
    pub fn has_const_factors(&self) -> bool {
        self.relationship.as_ref().is_some_and(|r| match r {
            Relationship::Factored(f) => f.is_constant(),
            Relationship::Exclusive(_) => false,
        })
    }
    pub fn set_relationship(&mut self, relationship: Option<Relationship>) {
        self.relationship = relationship;
    }

    #[must_use]
    pub fn get_exclusivity(&self) -> Option<&Exclusivity> {
        self.relationship.as_ref().and_then(|r| match r {
            Relationship::Factored(_) => None,
            Relationship::Exclusive(e) => Some(e),
        })
    }

    #[must_use]
    pub fn get_factors(&self) -> Option<&Factors> {
        self.relationship.as_ref().and_then(|r| match r {
            Relationship::Factored(f) => Some(f),
            Relationship::Exclusive(_) => None,
        })
    }

    /// Return normalised factor pairs
    #[must_use]
    pub fn get_factor_node_pairs(&self) -> Option<Vec<(&[NodeIndex], &[NodeIndex])>> {
        if self.has_factors() {
            let n0 = self.nodes[0].as_slice();

            Some(
                self.nodes
                    .iter()
                    .skip(1)
                    .map(|n1| (n0, n1.as_slice()))
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        }
    }

    /// Return constant normalised factor pairs
    ///
    /// # Error
    ///
    /// A [`ConstantFactorError`] with variant corresponding to the type of factors will be returned
    /// if there is any error calculating the factor values.
    #[must_use]
    pub fn get_const_norm_factor_pairs(
        &self,
        values: &ConstParameterValues,
    ) -> Option<Result<Vec<NodeConstFactorPair<'_>>, ConstantFactorError>> {
        if let Some(factors) = self.get_factors() {
            let pairs = match factors {
                Factors::Proportion { factors } => {
                    get_const_norm_proportional_factor_pairs(factors, &self.nodes, values)
                        .map_err(ConstantFactorError::Proportional)
                }
                Factors::Ratio { factors } => {
                    get_const_norm_ratio_factor_pairs(factors, &self.nodes, values).map_err(ConstantFactorError::Ratio)
                }
                Factors::Coefficients { factors, rhs } => {
                    get_const_coefficient_factor_pairs(factors, &self.nodes, rhs.as_ref(), values)
                        .map_err(ConstantFactorError::Coefficient)
                }
            };
            Some(pairs)
        } else {
            None
        }
    }

    /// Return normalised factor pairs
    ///
    /// # Error
    ///
    /// A [`FactorError`] with variant corresponding to the type of factors will be returned
    /// if there is any error calculating the factor values.
    ///
    #[must_use]
    pub fn get_norm_factor_pairs(
        &self,
        model: &Network,
        state: &State,
    ) -> Option<Result<Vec<NodeFactorPair<'_>>, FactorError>> {
        if let Some(factors) = self.get_factors() {
            let pairs = match factors {
                Factors::Proportion { factors } => {
                    get_norm_proportional_factor_pairs(factors, &self.nodes, model, state)
                        .map_err(FactorError::Proportional)
                }
                Factors::Ratio { factors } => {
                    get_norm_ratio_factor_pairs(factors, &self.nodes, model, state).map_err(FactorError::Ratio)
                }
                Factors::Coefficients { factors, rhs } => {
                    get_coefficient_factor_pairs(factors, &self.nodes, rhs.as_ref(), model, state)
                        .map_err(FactorError::Coefficient)
                }
            };
            Some(pairs)
        } else {
            None
        }
    }

    pub fn set_min_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.min_flow = value;
    }

    /// Get the min flow constraint value.
    ///
    /// # Errors
    ///
    /// If the constraint is a metric any error when attempting to retrieve
    /// that metric will be returned. See [`MetricF64::get_value`] for more information.
    pub fn get_min_flow(&self, model: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(model, state)
    }
    pub fn set_max_flow(&mut self, value: Option<MetricF64>) {
        self.flow_constraints.max_flow = value;
    }

    /// Get the max flow constraint value.
    ///
    /// # Errors
    ///
    /// If the constraint is a metric any error when attempting to retrieve
    /// that metric will be returned. See [`MetricF64::get_value`] for more information.
    pub fn get_max_flow(&self, model: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_max_flow(model, state)
    }

    /// Get the min and max flow bounds as a tuple.
    ///
    /// # Errors
    ///
    /// If either constraint is a metric any error when attempting to retrieve
    /// that metric will be returned. See [`MetricF64::get_value`] for more information.
    pub fn get_flow_bounds(&self, model: &Network, state: &State) -> Result<(f64, f64), AggregatedNodeError> {
        match (self.get_min_flow(model, state), self.get_max_flow(model, state)) {
            (Ok(min_flow), Ok(max_flow)) => Ok((min_flow, max_flow)),
            _ => Err(AggregatedNodeError::FlowConstraintsUndefined),
        }
    }

    #[must_use]
    pub fn default_metric(&self) -> MetricF64 {
        MetricF64::AggregatedNodeInFlow(self.index())
    }
}

#[derive(Debug, Error)]
pub enum FactorError {
    #[error("Error calculating proportional factors: {0}")]
    Proportional(#[from] ProportionalFactorError),
    #[error("Error calculating ratio factors: {0}")]
    Ratio(#[from] RatioFactorError),
    #[error("Error calculating coefficient factors: {0}")]
    Coefficient(#[from] CoefficientFactorError),
}

#[derive(Debug, Error)]
pub enum ConstantFactorError {
    #[error("Error calculating proportional factors: {0}")]
    Proportional(#[from] ConstantProportionalFactorError),
    #[error("Error calculating ratio factors: {0}")]
    Ratio(#[from] ConstantRatioFactorError),
    #[error("Error calculating coefficient factors: {0}")]
    Coefficient(#[from] ConstantCoefficientFactorError),
}

#[derive(Debug, Error)]
pub enum ProportionalFactorError {
    #[error(
        "Found {num_factors} proportional factors and {num_nodes} nodes in aggregated node. The number of proportional factors should equal one less than the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] MetricF64Error),
    #[error("Negative or zero factor values are not allowed. Found: {value}")]
    NegativeOrZeroFactor { value: f64 },
    #[error("Sum total of factors is greater than or equal to one. Total: {total} ")]
    TotalIsGreaterThanOrEqualToOne { total: f64 },
}

/// Calculate factor pairs for proportional factors.
///
/// There should be one less factor than node indices. The factors correspond to each of the node
/// indices after the first. Factor pairs relating the first index to each of the other indices are
/// calculated. This requires the sum of the factors to be greater than 0.0 and less than 1.0.
fn get_norm_proportional_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    model: &Network,
    state: &State,
) -> Result<Vec<NodeFactorPair<'a>>, ProportionalFactorError> {
    if factors.len() != nodes.len() - 1 {
        return Err(ProportionalFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    // First get the current factor values
    let values: Vec<f64> = factors
        .iter()
        .map(|f| {
            let v = f.get_value(model, state)?;
            if v < 0.0 {
                Err(ProportionalFactorError::NegativeOrZeroFactor { value: v })
            } else {
                Ok(v)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    let total: f64 = values.iter().sum();
    if total >= 1.0 {
        return Err(ProportionalFactorError::TotalIsGreaterThanOrEqualToOne { total });
    }

    let f0 = 1.0 - total;
    let n0 = nodes[0].as_slice();

    let pairs = nodes
        .iter()
        .skip(1)
        .zip(values)
        .map(move |(n1, f1)| {
            NodeFactorPair::new(NodeFactor::new(n0, 1.0), NodeFactor::new(n1.as_slice(), -f0 / f1), 0.0)
        })
        .collect::<Vec<_>>();

    Ok(pairs)
}

#[derive(Debug, Error)]
pub enum ConstantProportionalFactorError {
    #[error(
        "Found {num_factors} proportional factors and {num_nodes} nodes in aggregated node. The number of proportional factors should equal one less than the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] ConstantMetricF64Error),
    #[error("Negative or zero factor values are not allowed. Found: {value}")]
    NegativeOrZeroFactor { value: f64 },
    #[error("Sum total of factors is greater than or equal to one. Total: {total} ")]
    TotalIsGreaterThanOrEqualToOne { total: f64 },
}

/// Calculate constant factor pairs for proportional factors.
///
/// There should be one less factor than node indices. The factors correspond to each of the node
/// indices after the first. Factor pairs relating the first index to each of the other indices are
/// calculated. This requires the sum of the factors to be greater than 0.0 and less than 1.0. If
/// any of the factors are not constant, the factor pairs will contain `None` values.
fn get_const_norm_proportional_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    values: &ConstParameterValues,
) -> Result<Vec<NodeConstFactorPair<'a>>, ConstantProportionalFactorError> {
    if factors.len() != nodes.len() - 1 {
        return Err(ConstantProportionalFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    // First get the current factor values, ensuring they are all non-negative
    let factor_values: Vec<Option<f64>> = factors
        .iter()
        .map(|f| {
            let v = f.try_get_constant_value(values)?;
            if let Some(v) = v {
                if v < 0.0 {
                    Err(ConstantProportionalFactorError::NegativeOrZeroFactor { value: v })
                } else {
                    Ok(Some(v))
                }
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    let n0 = nodes[0].as_slice();

    // To calculate the factors we require that every factor is available.
    let pairs = if factor_values.iter().any(Option::is_none) {
        // At least one factor is not available; therefore we can not calculate "f0"
        nodes
            .iter()
            .skip(1)
            .zip(factor_values)
            .map(move |(n1, _f1)| {
                NodeConstFactorPair::new(
                    NodeConstFactor::new(n0, Some(1.0)),
                    NodeConstFactor::new(n1.as_slice(), None),
                    0.0,
                )
            })
            .collect::<Vec<_>>()
    } else {
        // All factors are available; therefore we can calculate "f0"
        let total: f64 = factor_values
            .iter()
            .map(|v| v.expect("Factor is `None`; this should be impossible."))
            .sum();
        if total >= 1.0 {
            return Err(ConstantProportionalFactorError::TotalIsGreaterThanOrEqualToOne { total });
        }

        let f0 = 1.0 - total;

        nodes
            .iter()
            .skip(1)
            .zip(factor_values)
            .map(move |(n1, f1)| {
                NodeConstFactorPair::new(
                    NodeConstFactor::new(n0, Some(1.0)),
                    NodeConstFactor::new(n1.as_slice(), f1.map(|v| -f0 / v)),
                    0.0,
                )
            })
            .collect::<Vec<_>>()
    };

    Ok(pairs)
}

#[derive(Debug, Error)]
pub enum RatioFactorError {
    #[error(
        "Found {num_factors} ratio factors and {num_nodes} nodes in aggregated node. The number of ratio factors should equal the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] MetricF64Error),
    #[error("Negative or zero factor values are not allowed. Found: {value}")]
    NegativeOrZeroFactor { value: f64 },
    #[error("Sum total of factors is greater than or equal to one. Total: {total} ")]
    TotalIsGreaterThanOrEqualToOne { total: f64 },
}

/// Calculate factor pairs for ratio factors.
///
/// The number of node indices and factors should be equal. The factors correspond to each of the
/// node indices. Factor pairs relating the first index to each of the other indices are calculated.
/// This requires that the factors are all non-zero.
fn get_norm_ratio_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    model: &Network,
    state: &State,
) -> Result<Vec<NodeFactorPair<'a>>, RatioFactorError> {
    // TODO handle error cases more gracefully
    if factors.len() != nodes.len() {
        return Err(RatioFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    let n0 = nodes[0].as_slice();
    let f0 = factors[0].get_value(model, state)?;
    if f0 < 0.0 {
        return Err(RatioFactorError::NegativeOrZeroFactor { value: f0 });
    }

    let pairs = nodes
        .iter()
        .zip(factors)
        .skip(1)
        .map(|(n1, f1)| {
            let v1 = f1.get_value(model, state)?;

            Ok(NodeFactorPair::new(
                NodeFactor::new(n0, 1.0),
                NodeFactor::new(n1.as_slice(), -f0 / v1),
                0.0,
            ))
        })
        .collect::<Result<Vec<_>, RatioFactorError>>()?;

    Ok(pairs)
}

#[derive(Debug, Error)]
pub enum ConstantRatioFactorError {
    #[error(
        "Found {num_factors} ratio factors and {num_nodes} nodes in aggregated node. The number of ratio factors should equal the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] ConstantMetricF64Error),
    #[error("Negative or zero factor values are not allowed. Found: {value}")]
    NegativeOrZeroFactor { value: f64 },
    #[error("Sum total of factors is greater than or equal to one. Total: {total} ")]
    TotalIsGreaterThanOrEqualToOne { total: f64 },
}

/// Constant ratio factors using constant values if they are available. If they are not available,
/// the factors are `None`.
fn get_const_norm_ratio_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    values: &ConstParameterValues,
) -> Result<Vec<NodeConstFactorPair<'a>>, ConstantRatioFactorError> {
    if factors.len() != nodes.len() {
        return Err(ConstantRatioFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    let n0 = nodes[0].as_slice();
    // Try to convert the factor into a constant

    let f0 = factors[0].try_get_constant_value(values)?;

    if let Some(v0) = f0 {
        if v0 < 0.0 {
            return Err(ConstantRatioFactorError::NegativeOrZeroFactor { value: v0 });
        }
    }

    let pairs = nodes
        .iter()
        .zip(factors)
        .skip(1)
        .map(|(n1, f1)| {
            let v1 = f1.try_get_constant_value(values)?;

            if let Some(v) = v1 {
                if v < 0.0 {
                    return Err(ConstantRatioFactorError::NegativeOrZeroFactor { value: v });
                }
            }

            let v1 = v1.and_then(|v| f0.map(|f0| -f0 / v));

            Ok(NodeConstFactorPair::new(
                NodeConstFactor::new(n0, Some(1.0)),
                NodeConstFactor::new(n1.as_slice(), v1),
                0.0,
            ))
        })
        .collect::<Result<Vec<_>, ConstantRatioFactorError>>()?;

    Ok(pairs)
}

#[derive(Debug, Error)]
pub enum CoefficientFactorError {
    #[error(
        "Found {num_factors} coefficient factors and {num_nodes} nodes in aggregated node. The number of coefficient factors should equal the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Coefficient factors are not yet implemented for more than two nodes.")]
    MoreThanTwoFactors,
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] MetricF64Error),
}

/// Calculate factor pairs for coefficient factors.
///
/// The number of node indices and factors should be equal. The factors correspond to each of the
/// node indices. The `rhs` value is the right-hand side of the ratio equation. The same right-hand side is used
/// for all factor pairs.
fn get_coefficient_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    rhs: Option<&MetricF64>,
    model: &Network,
    state: &State,
) -> Result<Vec<NodeFactorPair<'a>>, CoefficientFactorError> {
    // TODO handle error cases more gracefully
    if factors.len() != nodes.len() {
        return Err(CoefficientFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    if factors.len() != 2 {
        return Err(CoefficientFactorError::MoreThanTwoFactors);
    }

    let f0 = factors[0].get_value(model, state)?;
    let f1 = factors[1].get_value(model, state)?;
    let rhs = match rhs {
        Some(rhs) => rhs.get_value(model, state)?,
        None => 0.0,
    };

    Ok(vec![NodeFactorPair::new(
        NodeFactor::new(nodes[0].as_slice(), f0),
        NodeFactor::new(nodes[1].as_slice(), f1),
        rhs,
    )])
}

#[derive(Debug, Error)]
pub enum ConstantCoefficientFactorError {
    #[error(
        "Found {num_factors} coefficient factors and {num_nodes} nodes in aggregated node. The number of coefficient factors should equal the number of nodes."
    )]
    IncorrectNumberOfFactors { num_factors: usize, num_nodes: usize },
    #[error("Coefficient factors are not yet implemented for more than two nodes.")]
    MoreThanTwoFactors,
    #[error("Failed to get metric value for factor: {0}")]
    MetricF64(#[from] ConstantMetricF64Error),
}

/// Constant coefficient factors using constant values if they are available.
fn get_const_coefficient_factor_pairs<'a>(
    factors: &[MetricF64],
    nodes: &'a [Vec<NodeIndex>],
    rhs: Option<&MetricF64>,
    values: &ConstParameterValues,
) -> Result<Vec<NodeConstFactorPair<'a>>, ConstantCoefficientFactorError> {
    if factors.len() != nodes.len() {
        return Err(ConstantCoefficientFactorError::IncorrectNumberOfFactors {
            num_factors: factors.len(),
            num_nodes: nodes.len(),
        });
    }

    if factors.len() != 2 {
        return Err(ConstantCoefficientFactorError::MoreThanTwoFactors);
    }

    // Try to convert the factor into a constant

    let f0 = factors[0].try_get_constant_value(values)?;
    let f1 = factors[1].try_get_constant_value(values)?;

    let rhs = match rhs {
        Some(rhs) => rhs.try_get_constant_value(values)?.unwrap_or_default(),
        None => 0.0,
    };

    Ok(vec![NodeConstFactorPair::new(
        NodeConstFactor::new(nodes[0].as_slice(), f0),
        NodeConstFactor::new(nodes[1].as_slice(), f1),
        rhs,
    )])
}

#[cfg(test)]
mod tests {
    use crate::aggregated_node::Relationship;
    use crate::metric::MetricF64;
    use crate::models::Model;
    use crate::network::Network;
    use crate::parameters::MonthlyProfileParameter;
    use crate::recorders::AssertionF64Recorder;
    use crate::state::ParameterReturnValue;
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

        let relationship = Some(Relationship::new_ratio_factors(&[2.0.into(), 1.0.into()]));

        let _agg_node =
            network.add_aggregated_node("agg-node", None, &[vec![link_node0], vec![link_node1]], relationship);

        // Setup a demand on output-0
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-10.0).into()));

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model, &["ipm-simd", "ipm-ocl"], &[], &[]);
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

        let relationship = Some(Relationship::new_ratio_factors(&[
            factor_profile_idx.into_metric_f64(ParameterReturnValue::Before),
            1.0.into(),
        ]));

        let _agg_node =
            network.add_aggregated_node("agg-node", None, &[vec![link_node0], vec![link_node1]], relationship);

        // Setup a demand on output-0
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-10.0).into()));

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model, &["cbc", "ipm-simd", "ipm-ocl"], &[], &[]);
    }

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

        let _me_node = network.add_aggregated_node(
            "mutual-exclusivity",
            None,
            &[vec![link_node0], vec![link_node1]],
            Some(Relationship::new_exclusive(0, 1)),
        );

        // Setup a demand on output-0 and output-1.
        // output-0 has a lower penalty cost than output-1, so the flow should be directed to output-0.
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-10.0).into()));

        let output_node = network.get_mut_node_by_name("output", Some("1")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-5.0).into()));

        // Set-up assertion for "output-0" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "output-1" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64Recorder::new("link-1-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model, &["clp", "ipm-simd", "ipm-ocl"], &[], &[]);
    }

    /// Test double mutual exclusive flows
    ///
    /// The model has a single input that diverges to three links. Two sets of mutual exclusivity
    /// constraints are defined, one for the first two links and one for the last two links. This
    /// tests that a node can appear in two different mutual exclusivity constraints.
    #[test]
    fn test_double_mutual_exclusivity() {
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

        let link_node2 = network.add_link_node("link", Some("2")).unwrap();
        let output_node2 = network.add_output_node("output", Some("2")).unwrap();

        network.connect_nodes(input_node, link_node2).unwrap();
        network.connect_nodes(link_node2, output_node2).unwrap();

        let _me_node = network.add_aggregated_node(
            "mutual-exclusivity-01",
            None,
            &[vec![link_node0], vec![link_node1]],
            Some(Relationship::new_exclusive(0, 1)),
        );
        let _me_node = network.add_aggregated_node(
            "mutual-exclusivity-12",
            None,
            &[vec![link_node1], vec![link_node2]],
            Some(Relationship::new_exclusive(0, 1)),
        );

        // Setup a demand on the outputs
        // output-1 has a lower penalty cost than output-0 and output-2, so the flow should be directed to output-1.
        let output_node = network.get_mut_node_by_name("output", Some("0")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-5.0).into()));

        let output_node = network.get_mut_node_by_name("output", Some("1")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-15.0).into()));

        let output_node = network.get_mut_node_by_name("output", Some("2")).unwrap();
        output_node.set_max_flow_constraint(Some(100.0.into())).unwrap();

        output_node.set_cost(Some((-5.0).into()));

        // Set-up assertion for "output-0" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64Recorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "output-0" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64Recorder::new("link-1-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "output-2" node
        let idx = network.get_node_by_name("link", Some("2")).unwrap().index();
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64Recorder::new("link-2-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_time_domain().into(), network);

        run_all_solvers(&model, &["clp", "ipm-ocl", "ipm-simd"], &[], &[]);
    }
}
