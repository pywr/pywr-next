#![warn(clippy::pedantic)]
use crate::NodeIndex;
use crate::metric::{ConstantMetricF64Error, MetricF64, MetricF64Error, MetricF64ResolutionError, UnresolvedMetricF64};
use crate::network::{AggregatedNodeIndex, Network, ResolutionMaps};
use crate::node::{FlowConstraints, NodeMeta, UnresolvedNode};
use crate::state::{ConstParameterValues, State};
use thiserror::Error;

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

#[derive(Default)]
pub struct ProportionalFactorsBuilder {
    factors: Vec<UnresolvedMetricF64>,
}

impl ProportionalFactorsBuilder {
    pub fn factor(&mut self, factor: UnresolvedMetricF64) -> &mut Self {
        self.factors.push(factor);
        self
    }
}

impl RelationshipBuilder for ProportionalFactorsBuilder {
    fn build(&self, resolution_maps: &ResolutionMaps) -> Result<Relationship, RelationshipBuildError> {
        let factors = self
            .factors
            .iter()
            .map(|f| f.resolve(resolution_maps))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RelationshipBuildError::ResolveMetricF64Error {
                attr: "factors".to_string(),
                source,
            })?;

        Ok(Relationship::Factored(Factors::Proportion { factors }))
    }
}

#[derive(Default)]
pub struct RatioFactorsBuilder {
    factors: Vec<UnresolvedMetricF64>,
}

impl RatioFactorsBuilder {
    pub fn factor(&mut self, factor: UnresolvedMetricF64) -> &mut Self {
        self.factors.push(factor);
        self
    }
}

impl RelationshipBuilder for RatioFactorsBuilder {
    fn build(&self, resolution_maps: &ResolutionMaps) -> Result<Relationship, RelationshipBuildError> {
        let factors = self
            .factors
            .iter()
            .map(|f| f.resolve(resolution_maps))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RelationshipBuildError::ResolveMetricF64Error {
                attr: "factors".to_string(),
                source,
            })?;

        Ok(Relationship::Factored(Factors::Ratio { factors }))
    }
}

#[derive(Default)]
pub struct CoefficientFactorsBuilder {
    factors: Vec<UnresolvedMetricF64>,
    rhs: Option<UnresolvedMetricF64>,
}

impl CoefficientFactorsBuilder {
    pub fn factor(&mut self, factor: UnresolvedMetricF64) -> &mut Self {
        self.factors.push(factor);
        self
    }

    pub fn rhs(&mut self, rhs: UnresolvedMetricF64) -> &mut Self {
        self.rhs = Some(rhs);
        self
    }
}

impl RelationshipBuilder for CoefficientFactorsBuilder {
    fn build(&self, resolution_maps: &ResolutionMaps) -> Result<Relationship, RelationshipBuildError> {
        let factors = self
            .factors
            .iter()
            .map(|f| f.resolve(resolution_maps))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| RelationshipBuildError::ResolveMetricF64Error {
                attr: "factors".to_string(),
                source,
            })?;

        let rhs = self
            .rhs
            .as_ref()
            .map(|r| r.resolve(resolution_maps))
            .transpose()
            .map_err(|source| RelationshipBuildError::ResolveMetricF64Error {
                attr: "rhs".to_string(),
                source,
            })?;

        Ok(Relationship::Factored(Factors::Coefficients { factors, rhs }))
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

#[derive(Default, Clone)]
pub struct ExclusivityBuilder {
    min_active: u64,
    max_active: u64,
}

impl ExclusivityBuilder {
    pub fn min_active(&mut self, min_active: u64) -> &mut Self {
        self.min_active = min_active;
        self
    }

    pub fn max_active(&mut self, max_active: u64) -> &mut Self {
        self.max_active = max_active;
        self
    }
}

impl RelationshipBuilder for ExclusivityBuilder {
    fn build(&self, _resolution_maps: &ResolutionMaps) -> Result<Relationship, RelationshipBuildError> {
        Ok(Relationship::Exclusive(Exclusivity {
            min_active: self.min_active,
            max_active: self.max_active,
        }))
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
pub enum RelationshipBuildError {
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
}
pub trait RelationshipBuilder {
    /// Try to construct a [`Relationship`].
    ///
    /// # Errors
    ///
    /// A [`RelationshipBuildError`] should be returned if the builder is unable to resolve any of
    /// the metrics it references.
    fn build(&self, resolution_maps: &ResolutionMaps) -> Result<Relationship, RelationshipBuildError>;
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

    /// Get the min flow constraint value.
    ///
    /// # Errors
    ///
    /// If the constraint is a metric any error when attempting to retrieve
    /// that metric will be returned. See [`MetricF64::get_value`] for more information.
    pub fn get_min_flow(&self, model: &Network, state: &State) -> Result<f64, MetricF64Error> {
        self.flow_constraints.get_min_flow(model, state)
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
pub enum AggregatedNodeBuilderError {
    #[error("Index not found in resolution map.")]
    IndexNotFound,
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("Reference to node not found.")]
    NodeIndexNotFound { node: UnresolvedNode },
    #[error("Error building relationship: {0}")]
    RelationshipBuildError(#[from] RelationshipBuildError),
}

pub struct AggregatedNodeBuilder {
    name: UnresolvedNode,
    min_flow: Option<UnresolvedMetricF64>,
    max_flow: Option<UnresolvedMetricF64>,
    nodes: Vec<Vec<UnresolvedNode>>,
    relationship: Option<Box<dyn RelationshipBuilder>>,
}

impl AggregatedNodeBuilder {
    #[must_use]
    pub fn new(name: &str) -> Self {
        let name = UnresolvedNode::new(name, None);

        Self {
            name,
            min_flow: None,
            max_flow: None,
            nodes: Vec::new(),
            relationship: None,
        }
    }

    #[must_use]
    pub fn name(&self) -> &UnresolvedNode {
        &self.name
    }

    pub fn sub_name(&mut self, sub_name: &str) -> &mut Self {
        self.name.set_sub_name(Some(sub_name));
        self
    }

    pub fn min_flow(&mut self, min_flow: UnresolvedMetricF64) -> &mut Self {
        self.min_flow = Some(min_flow);
        self
    }
    pub fn max_flow(&mut self, max_flow: UnresolvedMetricF64) -> &mut Self {
        self.max_flow = Some(max_flow);
        self
    }
    pub fn nodes(&mut self, nodes: Vec<UnresolvedNode>) -> &mut Self {
        self.nodes.push(nodes);
        self
    }

    pub fn relationship(&mut self, relationship: Box<dyn RelationshipBuilder>) -> &mut Self {
        self.relationship = Some(relationship);
        self
    }

    /// Build a [`FlowConstraints`] from the builder.
    fn build_flow_constraints(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<FlowConstraints, AggregatedNodeBuilderError> {
        let min_flow = self
            .min_flow
            .as_ref()
            .map(|min_flow| {
                min_flow
                    .resolve(resolution_maps)
                    .map_err(|source| AggregatedNodeBuilderError::ResolveMetricF64Error {
                        attr: "min_flow".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let max_flow = self
            .max_flow
            .as_ref()
            .map(|max_flow| {
                max_flow
                    .resolve(resolution_maps)
                    .map_err(|source| AggregatedNodeBuilderError::ResolveMetricF64Error {
                        attr: "max_flow".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let flow_constraints = FlowConstraints::new(min_flow, max_flow);

        Ok(flow_constraints)
    }

    /// Try to construct an [`AggregatedNode`] from this builder.
    ///
    /// # Errors
    ///
    /// An [`AggregatedNodeBuilderError`] will be returned if the builder is unable to resolve
    /// any of the metrics or node names it references.
    pub fn build(&self, resolution_maps: &ResolutionMaps) -> Result<AggregatedNode, AggregatedNodeBuilderError> {
        let index = resolution_maps
            .aggregated_nodes
            .get(&self.name)
            .ok_or(AggregatedNodeBuilderError::IndexNotFound)?;
        let meta = NodeMeta::from_unresolved_name(self.name.clone(), *index);

        let flow_constraints = self.build_flow_constraints(resolution_maps)?;

        let nodes = self
            .nodes
            .iter()
            .map(|indices| {
                indices
                    .iter()
                    .map(|unresolved| {
                        resolution_maps.nodes.get(unresolved).copied().ok_or_else(|| {
                            AggregatedNodeBuilderError::NodeIndexNotFound {
                                node: unresolved.clone(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;

        let relationship = self
            .relationship
            .as_ref()
            .map(|r| r.build(resolution_maps))
            .transpose()?;

        Ok(AggregatedNode {
            meta,
            flow_constraints,
            nodes,
            relationship,
        })
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
    use crate::aggregated_node::{AggregatedNodeBuilder, ExclusivityBuilder, RatioFactorsBuilder};
    use crate::metric::UnresolvedMetricF64;
    use crate::models::ModelBuilder;
    use crate::network::NetworkBuilder;
    use crate::node::NodeBuilder;
    use crate::parameters::{MonthlyProfileParameterBuilder, ParameterName};
    use crate::recorders::AssertionF64RecorderBuilder;
    use crate::state::ParameterReturnValue;
    use crate::test_utils::{default_domain_builder, run_all_solvers};
    use ndarray::Array2;

    /// Test the factors forcing a simple ratio of flow
    ///
    /// The model has a single input that diverges to two links and respective output nodes.
    #[test]
    fn test_simple_factors() {
        let mut builder = NetworkBuilder::default();

        let input_node = NodeBuilder::input("input");
        builder.node(input_node);

        let mut link_node0 = NodeBuilder::link("link");
        link_node0.sub_name("0");
        let link_node0_ref = link_node0.name().clone();
        builder.node(link_node0);

        // Setup a demand on output-0
        let mut output_node0 = NodeBuilder::output("output");
        output_node0.sub_name("0").max_flow(100.0.into()).cost((-10.0).into());
        builder.node(output_node0);

        builder.connect("input", None, "link", Some("0"));
        builder.connect("link", Some("0"), "output", Some("0"));

        let mut link_node1 = NodeBuilder::link("link");
        link_node1.sub_name("1");
        let link_node1_ref = link_node1.name().clone();
        builder.node(link_node1);

        let mut output_node1 = NodeBuilder::output("output");
        output_node1.sub_name("1");
        builder.node(output_node1);

        builder.connect("input", None, "link", Some("1"));
        builder.connect("link", Some("1"), "output", Some("1"));

        let mut relationship = RatioFactorsBuilder::default();
        relationship.factor(2.0.into()).factor(1.0.into());

        let mut agg_node = AggregatedNodeBuilder::new("agg-node");
        agg_node
            .nodes(vec![link_node0_ref.clone()])
            .nodes(vec![link_node1_ref.clone()])
            .relationship(Box::new(relationship));
        builder.agg_node(agg_node);

        // Set-up assertion for "link-0" node
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node0_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        // Set-up assertion for "link-1" node
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node1_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        let domain = default_domain_builder();
        let model = ModelBuilder::new(domain, builder).build().unwrap();

        run_all_solvers(&model, &["ipm-simd", "ipm-ocl"], &[], &[]);
    }

    /// Test the factors forcing a simple ratio of flow that varies over time
    ///
    /// The model has a single input that diverges to two links and respective output nodes.
    #[test]
    fn test_simple_factor_profile() {
        let mut builder = NetworkBuilder::default();

        let input_node = NodeBuilder::input("input");
        builder.node(input_node);

        let mut link_node0 = NodeBuilder::link("link");
        link_node0.sub_name("0");
        let link_node0_ref = link_node0.name().clone();
        builder.node(link_node0);

        // Setup a demand on output-0
        let mut output_node0 = NodeBuilder::output("output");
        output_node0.sub_name("0").max_flow(100.0.into()).cost((-10.0).into());
        builder.node(output_node0);

        builder.connect("input", None, "link", Some("0"));
        builder.connect("link", Some("0"), "output", Some("0"));

        let mut link_node1 = NodeBuilder::link("link");
        link_node1.sub_name("1");
        let link_node1_ref = link_node1.name().clone();
        builder.node(link_node1);

        let mut output_node1 = NodeBuilder::output("output");
        output_node1.sub_name("1");
        builder.node(output_node1);

        builder.connect("input", None, "link", Some("1"));
        builder.connect("link", Some("1"), "output", Some("1"));

        let factor_profile_name = ParameterName::new("factor-profile", None);
        let factor_profile = MonthlyProfileParameterBuilder::new(factor_profile_name.clone(), [2.0; 12]);
        builder.parameters().f64(Box::new(factor_profile));

        let mut relationship = RatioFactorsBuilder::default();
        relationship
            .factor(UnresolvedMetricF64::ParameterValue {
                name: factor_profile_name,
                return_value: ParameterReturnValue::Before,
            })
            .factor(1.0.into());

        let mut agg_node = AggregatedNodeBuilder::new("agg-node");
        agg_node
            .nodes(vec![link_node0_ref.clone()])
            .nodes(vec![link_node1_ref.clone()])
            .relationship(Box::new(relationship));
        builder.agg_node(agg_node);

        // Set-up assertion for "input" node
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node0_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        // Set-up assertion for "input" node
        let expected = Array2::from_elem((366, 10), 50.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-1-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node1_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        let domain = default_domain_builder();
        let model = ModelBuilder::new(domain, builder).build().unwrap();

        run_all_solvers(&model, &["cbc", "ipm-simd", "ipm-ocl"], &[], &[]);
    }

    /// Test mutual exclusive flows
    ///
    /// The model has a single input that diverges to two links, only one of which can be active at a time.
    #[test]
    fn test_simple_mutual_exclusivity() {
        let mut builder = NetworkBuilder::default();
        // Setup a demand on output-0 and output-1.
        // output-0 has a lower penalty cost than output-1, so the flow should be directed to output-0.
        let input_node = NodeBuilder::input("input");
        builder.node(input_node);

        let mut link_node0 = NodeBuilder::link("link");
        link_node0.sub_name("0");
        let link_node0_ref = link_node0.name().clone();
        builder.node(link_node0);

        // Setup a demand on output-0
        let mut output_node0 = NodeBuilder::output("output");
        output_node0.sub_name("0").max_flow(100.0.into()).cost((-10.0).into());
        builder.node(output_node0);

        builder.connect("input", None, "link", Some("0"));
        builder.connect("link", Some("0"), "output", Some("0"));

        let mut link_node1 = NodeBuilder::link("link");
        link_node1.sub_name("1");
        let link_node1_ref = link_node1.name().clone();
        builder.node(link_node1);

        let mut output_node1 = NodeBuilder::output("output");
        output_node1.sub_name("1").max_flow(100.0.into()).cost((-5.0).into());
        builder.node(output_node1);

        builder.connect("input", None, "link", Some("1"));
        builder.connect("link", Some("1"), "output", Some("1"));

        let mut relationship = ExclusivityBuilder::default();
        relationship.min_active(0).max_active(1);

        let mut agg_node = AggregatedNodeBuilder::new("mutual-exclusivity");
        agg_node
            .nodes(vec![link_node0_ref.clone()])
            .nodes(vec![link_node1_ref.clone()])
            .relationship(Box::new(relationship));
        builder.agg_node(agg_node);

        // Set-up assertion for "output-0" node

        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node0_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        // Set-up assertion for "output-1" node

        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-1-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node1_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        let domain = default_domain_builder();
        let model = ModelBuilder::new(domain, builder).build().unwrap();

        run_all_solvers(&model, &["clp", "ipm-simd", "ipm-ocl"], &[], &[]);
    }

    /// Test double mutual exclusive flows
    ///
    /// The model has a single input that diverges to three links. Two sets of mutual exclusivity
    /// constraints are defined, one for the first two links and one for the last two links. This
    /// tests that a node can appear in two different mutual exclusivity constraints.
    #[test]
    fn test_double_mutual_exclusivity() {
        let mut builder = NetworkBuilder::default();
        // Setup a demand on the outputs
        // output-1 has a lower penalty cost than output-0 and output-2, so the flow should be directed to output-1.
        let input_node = NodeBuilder::input("input");
        builder.node(input_node);

        let mut link_node0 = NodeBuilder::link("link");
        link_node0.sub_name("0");
        let link_node0_ref = link_node0.name().clone();
        builder.node(link_node0);

        // Setup a demand on output-0
        let mut output_node0 = NodeBuilder::output("output");
        output_node0.sub_name("0").max_flow(100.0.into()).cost((-5.0).into());
        builder.node(output_node0);

        builder.connect("input", None, "link", Some("0"));
        builder.connect("link", Some("0"), "output", Some("0"));

        let mut link_node1 = NodeBuilder::link("link");
        link_node1.sub_name("1");
        let link_node1_ref = link_node1.name().clone();
        builder.node(link_node1);

        let mut output_node1 = NodeBuilder::output("output");
        output_node1.sub_name("1").max_flow(100.0.into()).cost((-15.0).into());
        builder.node(output_node1);

        builder.connect("input", None, "link", Some("1"));
        builder.connect("link", Some("1"), "output", Some("1"));

        let mut link_node2 = NodeBuilder::link("link");
        link_node2.sub_name("2");
        let link_node2_ref = link_node2.name().clone();
        builder.node(link_node2);

        let mut output_node2 = NodeBuilder::output("output");
        output_node2.sub_name("2").max_flow(100.0.into()).cost((-5.0).into());
        builder.node(output_node2);

        builder.connect("input", None, "link", Some("2"));
        builder.connect("link", Some("2"), "output", Some("2"));

        let mut relationship = ExclusivityBuilder::default();
        relationship.min_active(0).max_active(1);

        let mut agg_node = AggregatedNodeBuilder::new("mutual-exclusivity-01");
        agg_node
            .nodes(vec![link_node0_ref.clone()])
            .nodes(vec![link_node1_ref.clone()])
            .relationship(Box::new(relationship.clone()));
        builder.agg_node(agg_node);

        let mut agg_node = AggregatedNodeBuilder::new("mutual-exclusivity-12");
        agg_node
            .nodes(vec![link_node1_ref.clone()])
            .nodes(vec![link_node2_ref.clone()])
            .relationship(Box::new(relationship));
        builder.agg_node(agg_node);

        // Set-up assertion for "link-0" node
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node0_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        // Set-up assertion for "link-1" node
        let expected = Array2::from_elem((366, 10), 100.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-1-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node1_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        // Set-up assertion for "link-2" node
        let expected = Array2::from_elem((366, 10), 0.0);
        let recorder = AssertionF64RecorderBuilder::new(
            "link-2-flow",
            UnresolvedMetricF64::NodeOutFlow(link_node2_ref.clone()),
            expected,
        );
        builder.recorder(Box::new(recorder));

        let domain = default_domain_builder();
        let model = ModelBuilder::new(domain, builder).build().unwrap();

        run_all_solvers(&model, &["clp", "ipm-ocl", "ipm-simd"], &[], &[]);
    }
}
