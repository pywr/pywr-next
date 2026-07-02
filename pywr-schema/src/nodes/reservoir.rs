use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot, StorageNode, StorageNodeAttribute};
use crate::parameters::ConstantFloatVec;
use crate::{SchemaError, mermaid};
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{agg_funcs::AggFuncF64, metric::UnresolvedMetricF64, node::UnresolvedNode, parameters::ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

/// The type of spill node.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub enum SpillNodeType {
    /// The spill node is created as output node.
    OutputNode,
    /// The spill node is created as link node.
    LinkNode,
}

/// The bathymetry data type.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub enum BathymetryType {
    /// The bathymetry is calculated by interpolating the storage and area data piecewise.
    Interpolated {
        storage: ConstantFloatVec,
        area: ConstantFloatVec,
    },
    /// The bathymetry is calculated using a polynomial expressions and the provided coefficients.
    Polynomial(Vec<f64>),
}

/// The bathymetric data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Bathymetry {
    /// The bathymetric data and type.
    pub data: BathymetryType,
    /// Whether the `storage` provided to the [`BathymetryType`] is proportional (0-1) or not.
    pub is_storage_proportional: bool,
}

/// The evaporation data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Evaporation {
    /// The [`Metric`] containing the evaporation height.
    pub data: Metric,
    /// The cost to assign to the [`crate::nodes::OutputNode`].
    pub cost: Option<Metric>,
    /// If `true` the maximum surface area will be used to calculate the evaporation volume. When
    /// `false`, the area is calculated from the bathymetric data. This defaults to `false`.
    pub use_max_area: Option<bool>,
}

/// The leakage data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Leakage {
    /// The [`Metric`] containing the lost flow.
    pub loss: Metric,
    /// The cost to assign to the [`crate::nodes::OutputNode`].
    pub cost: Option<Metric>,
}

/// The rainfall data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Rainfall {
    /// The [`Metric`] containing the rainfall level.
    pub data: Metric,
    /// If `true` the maximum surface area will be used to calculate the rainfall volume. When
    /// `false`, the area is calculated from the bathymetric data. This defaults to `false`.
    pub use_max_area: Option<bool>,
}

// This macro generates a subset enum for the `ReservoirNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum ReservoirNodeAttribute {
        /// The absolute reservoir volume.
        Volume,
        /// The proportional reservoir proportional volume (0-1).
        ProportionalVolume,
        MaxVolume,
        /// The minimum residual flow when the `compensation` field is provided.
        Compensation,
        /// The rainfall flow when the `rainfall` field is provided.
        Rainfall,
        /// The evaporation flow when the `evaporation` field is provided.
        Evaporation,
    }
}

impl From<StorageNodeAttribute> for ReservoirNodeAttribute {
    fn from(attr: StorageNodeAttribute) -> Self {
        match attr {
            StorageNodeAttribute::Volume => ReservoirNodeAttribute::Volume,
            StorageNodeAttribute::ProportionalVolume => ReservoirNodeAttribute::ProportionalVolume,
            StorageNodeAttribute::MaxVolume => ReservoirNodeAttribute::MaxVolume,
        }
    }
}

node_component_subset_enum! {
    pub enum ReservoirNodeComponent {
        Loss,
        Compensation,
        Rainfall,
        Evaporation,
    }
}

pub enum ReservoirOutputNodeSlot {
    Storage,
    Compensation,
    Spill,
}

impl From<ReservoirOutputNodeSlot> for NodeSlot {
    fn from(slot: ReservoirOutputNodeSlot) -> Self {
        match slot {
            ReservoirOutputNodeSlot::Storage => NodeSlot::Storage,
            ReservoirOutputNodeSlot::Compensation => NodeSlot::Compensation,
            ReservoirOutputNodeSlot::Spill => NodeSlot::Spill,
        }
    }
}

impl TryFrom<NodeSlot> for ReservoirOutputNodeSlot {
    type Error = SchemaError;
    fn try_from(slot: NodeSlot) -> Result<Self, Self::Error> {
        match slot {
            NodeSlot::Storage => Ok(ReservoirOutputNodeSlot::Storage),
            NodeSlot::Compensation => Ok(ReservoirOutputNodeSlot::Compensation),
            NodeSlot::Spill => Ok(ReservoirOutputNodeSlot::Spill),
            _ => Err(SchemaError::OutputNodeSlotNotSupported { slot }),
        }
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
/// A reservoir node with compensation, leakage, direct rainfall and evaporation.
///
/// # Implementation
///
/// This is a [`StorageNode`] connected to an upstream node `Upstream` and downstream network node `D`. When
/// an edge to this component is created without slots, the target nodes are directly connected to
/// `Downstream 1` via the "Storage" slot.
///
/// This component has the following internal nodes, which the modeller still needs to connect to
/// other network nodes using slots:
/// - Compensation: when the `compensation` field is provided, a `Link` node with a `min_flow`
///   constraint will be connected to the reservoir. To connect the compensation node to another
///   node, you can use the slot named `compensation` in an edge `from_slot` property.
/// - Spill: this can either be a [`pywr_core::node::OutputNode`] or a [`pywr_core::node::LinkNode`].
///   When an output node is created, a slot called `to_spill` is added to connect any node to this
///   internal node. For example, you can connect the compensation node. When a link is created two
///   slots are available: `from_spill`to connect the link to another network node; and `to_spill`
///   to route additional water via this node.
///   Use `None` if you don't want to create the spill and to manually route the water.
/// - Rainfall: this is a [`pywr_core::node::InputNode`] with a `min_flow` and `max_flow` equal to
///   the product of the surface area and the rainfall height.
/// - Evaporation: this is an optional [`pywr_core::node::OutputNode`] with a `max_flow` equal to
///   the product of the surface area and the evaporation level. A cost can be also added to control
///   the node's behaviour.
/// - Leakage: this is an optional [`pywr_core::node::OutputNode`] with a `max_flow` equal to the
///   provided [`Metric`]'s value.
///
/// The internal layout when configured with a `LinkNode` spill:
#[doc = mermaid!("doc_diagrams/reservoir-spill-link.mmd")]
///
/// The internal layout when configured with an `OutputNode` spill:
#[doc = mermaid!("doc_diagrams/reservoir-spill-output.mmd")]
///
/// ## Rainfall and evaporation calculation
/// The rainfall and evaporation volumes are calculated by multiplying the reservoir current
/// surface area by the provided heights. The area is calculated based on the [`BathymetryType`]
/// value:
///  - when [`BathymetryType::Interpolated`], the area is calculated using a piecewise linear
///    interpolation with the [`pywr_core::parameters::AggregatedParameter`] of the data in
///    [`Bathymetry`]'s `storage` and `area` fields.
/// - when [`BathymetryType::Polynomial`], the area is calculated from the polynomial expression
///   using the [`pywr_core::parameters::Polynomial1DParameter`].
///
///
/// If `rainfall.use_max_area` is set to `true`, then the rainfall volume is calculated using the
/// maximum surface area only.
/// It is up to the user to ensure that the units for these calculations are consistent. The
/// units for the rainfall and evaporation depth ($D$) must be consistent with those of the area ($A$)
/// to produce a flow ($Q$) consistent with the model's inflows
///
/// $$
/// D [\frac{L}{T}] * A [L^2] == Q [\frac{L^3}{T}]
/// $$
///
///
/// # Available attributes and components
///
/// The enums [`ReservoirNodeAttribute`] and [`ReservoirNodeComponent`] define the available
/// attributes and components for this node.
///
///
/// # JSON Examples
/// ## Reservoir with output spill
///
///
/// ```json
#[doc = include_str!("../../tests/reservoir_with_spill1.json")]
/// ```
///
/// ## Reservoir with link spill
/// The compensation goes into the spill which routes water to the "River termination" node.
///
/// ```json
#[doc = include_str!("../../tests/reservoir_with_river1.json")]
/// ```
pub struct ReservoirNode {
    #[serde(flatten)]
    pub storage: StorageNode,
    /// The compensation flow. Use `None` not to add any minimum residual flow to the reservoir.
    pub compensation: Option<Metric>,
    /// Whether to create the spill node. The node can be a link or an output node.
    pub spill: Option<SpillNodeType>,
    /// If the `compensation` and `spill` fields are set, this options when `true` will create an edge
    /// from the compensation to the spill node. When `false`, the user has to connect the
    /// compensation node to an existing node. Default to `true`.
    pub connect_compensation_to_spill: Option<bool>,
    /// The storage table with the relationship between storage and reservoir surface area. This must
    /// be provided for the calculations of the precipitation and evaporation volumes.
    pub surface_area: Option<Bathymetry>,
    /// The rainfall data. Use `None` not to add the rainfall node.
    pub rainfall: Option<Rainfall>,
    /// The evaporation data. Use `None` not to add the evaporation node.
    pub evaporation: Option<Evaporation>,
    /// The leakage to set on the node. Use `None` not to add any loss.
    pub leakage: Option<Leakage>,
}

impl ReservoirNode {
    pub const DEFAULT_COMPONENT: ReservoirNodeComponent = ReservoirNodeComponent::Compensation;

    /// Get the node's metadata.
    pub(crate) fn meta(&self) -> &NodeMeta {
        &self.storage.meta
    }

    pub fn iter_output_slots(&self) -> impl Iterator<Item = NodeSlot> + '_ {
        [
            ReservoirOutputNodeSlot::Storage.into(),
            ReservoirOutputNodeSlot::Compensation.into(),
            ReservoirOutputNodeSlot::Spill.into(),
        ]
        .into_iter()
    }

    pub fn default_attribute(&self) -> ReservoirNodeAttribute {
        self.storage.default_attribute().into()
    }

    pub fn default_component(&self) -> ReservoirNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl ReservoirNode {
    const DEFAULT_OUTPUT_SLOT: ReservoirOutputNodeSlot = ReservoirOutputNodeSlot::Storage;
    /// The sub-name of the compensation link node.
    fn compensation_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.storage.meta.name, Some("compensation"))
    }

    /// The sub-name of the spill output node.
    fn spill_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.storage.meta.name, Some("spill"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![self.meta().name.as_str().into()])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Use the default slot if none is specified
        let slot = match slot {
            Some(c) => c.clone().try_into()?,
            None => Self::DEFAULT_OUTPUT_SLOT,
        };

        let indices = match slot {
            ReservoirOutputNodeSlot::Storage => {
                vec![self.meta().name.as_str().into()]
            }
            ReservoirOutputNodeSlot::Compensation => {
                vec![self.compensation_node_sub_name()]
            }
            ReservoirOutputNodeSlot::Spill => match self.spill {
                Some(SpillNodeType::LinkNode) => {
                    vec![self.spill_node_sub_name()]
                }
                _ => {
                    return Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.into() });
                }
            },
        };

        Ok(indices)
    }

    /// The sub-name of the rainfall node.
    fn rainfall_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.storage.meta.name, Some("rainfall"))
    }

    /// The sub-name of the evaporation node.
    fn evaporation_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.storage.meta.name, Some("evaporation"))
    }

    /// The sub-name of the leakage node.
    fn leakage_node_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.storage.meta.name, Some("leakage"))
    }

    pub fn nodes_for_flow_constraints(
        &self,
        component: Option<NodeComponent>,
    ) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let node = match component {
            ReservoirNodeComponent::Loss => self.leakage_node_sub_name(),
            ReservoirNodeComponent::Compensation => self.compensation_node_sub_name(),
            ReservoirNodeComponent::Rainfall => self.rainfall_node_sub_name(),
            ReservoirNodeComponent::Evaporation => self.evaporation_node_sub_name(),
        };

        Ok(vec![node])
    }

    pub fn node_indices_for_storage_constraints(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        let nodes = vec![UnresolvedNode::new(self.meta().name.as_str(), None)];
        Ok(nodes)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Add storage node
        self.storage.add_to_network(network, args)?;
        // Storage node name
        let storage_name = UnresolvedNode::new(self.meta().name.as_str(), None);

        // add compensation node and edge
        let comp_node = match &self.compensation {
            Some(compensation) => {
                let mut comp = pywr_core::NodeBuilder::link(self.compensation_node_sub_name());

                let value = compensation.load(network, args, Some(&self.meta().name))?;
                comp.min_flow(value);

                let comp_name = comp.name().clone();
                network.connect(storage_name.clone(), comp_name.clone());
                network.node(comp);

                Some(comp_name)
            }
            None => None,
        };

        // add spill and edge
        let spill_node = match &self.spill {
            None => None,
            Some(node_type) => {
                let spill = match node_type {
                    SpillNodeType::OutputNode => pywr_core::NodeBuilder::output(self.spill_node_sub_name()),
                    SpillNodeType::LinkNode => pywr_core::NodeBuilder::link(self.spill_node_sub_name()),
                };

                let spill_name = spill.name().clone();
                network.connect(storage_name.clone(), spill_name.clone());

                network.node(spill);

                Some(spill_name)
            }
        };

        // connect compensation and spill
        let connect_comp = self.connect_compensation_to_spill.unwrap_or(true);
        if connect_comp {
            if let (Some(spill), Some(comp)) = (spill_node, comp_node) {
                network.connect(comp, spill);
            }
        }

        // add rainfall node and edge
        if self.rainfall.is_some() && self.surface_area.is_none() {
            return Err(SchemaError::MissingNodeAttribute {
                attr: "surface_area".to_string(),
                name: self.meta().name.clone(),
            });
        }

        if self.evaporation.is_some() && self.surface_area.is_none() {
            return Err(SchemaError::MissingNodeAttribute {
                attr: "surface_area".to_string(),
                name: self.meta().name.clone(),
            });
        }

        // add rainfall and evaporation
        if let Some(bathymetry) = &self.surface_area {
            if let Some(rainfall) = &self.rainfall {
                let mut rainfall_input = pywr_core::NodeBuilder::input(self.rainfall_node_sub_name());

                let use_max_area = rainfall.use_max_area.unwrap_or(false);
                let rainfall_area_metric =
                    self.get_area_metric(network, args, "rainfall_area", bathymetry, use_max_area)?;
                let rainfall_metric = rainfall.data.load(network, args, Some(&self.meta().name))?;

                let rainfall_flow_parameter_name = ParameterName::new("rainfall", Some(self.meta().name.as_str()));
                let mut rainfall_flow_parameter = pywr_core::parameters::AggregatedParameterBuilder::new(
                    rainfall_flow_parameter_name.clone(),
                    AggFuncF64::Product,
                );

                rainfall_flow_parameter
                    .metric(rainfall_metric)
                    .metric(rainfall_area_metric.clone());

                network.parameters().f64(Box::new(rainfall_flow_parameter));

                let rainfall_flow_metric = UnresolvedMetricF64::new_parameter_before(rainfall_flow_parameter_name);

                rainfall_input
                    .min_flow(rainfall_flow_metric.clone())
                    .max_flow(rainfall_flow_metric);

                network.connect(rainfall_input.name().clone(), storage_name.clone());
                network.node(rainfall_input);
            }

            // add evaporation node and edge
            if let Some(evaporation) = &self.evaporation {
                let mut evaporation_output = pywr_core::NodeBuilder::input(self.evaporation_node_sub_name());

                let use_max_area = evaporation.use_max_area.unwrap_or(false);
                let evaporation_area_metric =
                    self.get_area_metric(network, args, "evaporation_area", bathymetry, use_max_area)?;

                // add volume to output node
                let evaporation_metric = evaporation.data.load(network, args, Some(&self.meta().name))?;
                let evaporation_flow_parameter_name =
                    ParameterName::new("evaporation", Some(self.meta().name.as_str()));
                let mut evaporation_flow_parameter = pywr_core::parameters::AggregatedParameterBuilder::new(
                    evaporation_flow_parameter_name.clone(),
                    AggFuncF64::Product,
                );
                evaporation_flow_parameter
                    .metric(evaporation_metric)
                    .metric(evaporation_area_metric.clone());

                network.parameters().f64(Box::new(evaporation_flow_parameter));

                let evaporation_flow_metric =
                    UnresolvedMetricF64::new_parameter_before(evaporation_flow_parameter_name);

                evaporation_output.max_flow(evaporation_flow_metric);
                // set optional cost
                if let Some(cost) = &evaporation.cost {
                    let value = cost.load(network, args, Some(&self.meta().name))?;
                    evaporation_output.cost(value);
                }

                network.connect(storage_name.clone(), evaporation_output.name().clone());
                network.node(evaporation_output);
            }
        }

        // add leakage node and edge
        if let Some(leakage) = &self.leakage {
            let mut leakage_output = pywr_core::NodeBuilder::output(self.leakage_node_sub_name());

            let value = leakage.loss.load(network, args, Some(&self.meta().name))?;
            leakage_output.max_flow(value);

            if let Some(cost) = &leakage.cost {
                let value = cost.load(network, args, Some(&self.meta().name))?;
                leakage_output.cost(value);
            }

            network.connect(storage_name.clone(), leakage_output.name().clone());
            network.node(leakage_output);
        }

        Ok(())
    }

    /// Get the `MetricF64` for the reservoir surface area. The area is calculated either using
    /// a [`pywr_core::parameters::InterpolatedParameter`] or a
    /// [`pywr_core::parameters::Polynomial1DParameter`] from the storage's volume. When the
    /// `use_max_area` parameter is `true`, the area is calculated using the storage's max volume
    /// from the state instead of the current reservoir's volume.
    ///
    /// # Arguments
    ///
    /// * `network`: The network.
    /// * `args`: The arguments.
    /// * `name`: The unique name to assign to the created parameters.
    /// * `bathymetry`: The bathymetric data.
    /// * `use_max_area`: Whether to get the max area from the reservoir's max volume.
    ///
    /// returns: `Result<MetricF64, SchemaError>`
    fn get_area_metric(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        name: &str,
        bathymetry: &Bathymetry,
        use_max_area: bool,
    ) -> Result<UnresolvedMetricF64, SchemaError> {
        // get the current storage
        let storage_node = UnresolvedNode::new(self.meta().name.as_str(), None);

        // the storage (absolute or relative) can be the current or max volume
        let current_storage = match (bathymetry.is_storage_proportional, use_max_area) {
            (false, false) => UnresolvedMetricF64::NodeVolume(storage_node),
            (true, false) => UnresolvedMetricF64::NodeProportionalVolume(storage_node),
            (false, true) => UnresolvedMetricF64::NodeMaxVolume(storage_node),
            (true, true) => UnresolvedMetricF64::Constant(1.0),
        };

        // get the variable area metric
        let area_metric = match &bathymetry.data {
            BathymetryType::Interpolated { storage, area } => {
                let storage_metric = storage.load(args.tables)?;
                let area_metric = area.load(args.tables)?;

                let points = storage_metric
                    .into_iter()
                    .zip(area_metric)
                    .map(|(s, a)| (s.into(), a.into()))
                    .collect::<Vec<_>>();

                let interpolated_area_parameter_name = ParameterName::new(name, Some(self.meta().name.as_str()));
                let interpolated_area_parameter = pywr_core::parameters::InterpolatedParameterBuilder::new(
                    interpolated_area_parameter_name.clone(),
                    current_storage,
                    points,
                );

                network.parameters().f64(Box::new(interpolated_area_parameter));

                UnresolvedMetricF64::new_parameter_before(interpolated_area_parameter_name)
            }
            BathymetryType::Polynomial(coeffs) => {
                let poly_area_parameter_name = ParameterName::new(name, Some(self.meta().name.as_str()));
                let poly_area_parameter = pywr_core::parameters::Polynomial1DParameterBuilder::new(
                    poly_area_parameter_name.clone(),
                    current_storage,
                    coeffs.clone(),
                );

                network.parameters().f64(Box::new(poly_area_parameter));

                UnresolvedMetricF64::new_parameter_before(poly_area_parameter_name)
            }
        };

        Ok(area_metric)
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        match self.storage.create_metric(attribute) {
            Ok(m) => Ok(m),
            Err(SchemaError::NodeAttributeNotSupported { .. }) => {
                let attr = match attribute {
                    Some(attr) => attr.try_into()?,
                    None => self.default_attribute(),
                };

                let metric = match attr {
                    ReservoirNodeAttribute::Compensation => {
                        if self.compensation.is_some() {
                            UnresolvedMetricF64::NodeInFlow(self.compensation_node_sub_name())
                        } else {
                            0.0.into()
                        }
                    }
                    ReservoirNodeAttribute::Rainfall => {
                        if self.rainfall.is_some() && self.surface_area.is_some() {
                            UnresolvedMetricF64::NodeInFlow(self.rainfall_node_sub_name())
                        } else {
                            0.0.into()
                        }
                    }
                    ReservoirNodeAttribute::Evaporation => {
                        if self.rainfall.is_some() && self.surface_area.is_some() {
                            UnresolvedMetricF64::NodeInFlow(self.evaporation_node_sub_name())
                        } else {
                            0.0.into()
                        }
                    }
                    ReservoirNodeAttribute::Volume => {
                        UnresolvedMetricF64::NodeVolume(UnresolvedNode::new(self.meta().name.as_str(), None))
                    }
                    ReservoirNodeAttribute::ProportionalVolume => UnresolvedMetricF64::NodeProportionalVolume(
                        UnresolvedNode::new(self.meta().name.as_str(), None),
                    ),
                    ReservoirNodeAttribute::MaxVolume => {
                        UnresolvedMetricF64::NodeMaxVolume(UnresolvedNode::new(self.meta().name.as_str(), None))
                    }
                };

                Ok(metric)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod tests {
    use crate::model::ModelSchema;

    fn reservoir_with_spill_str() -> &'static str {
        include_str!("../../tests/reservoir_with_spill1.json")
    }

    #[test]
    fn test_model_nodes_and_edges() {
        let data = reservoir_with_spill_str();
        let schema: ModelSchema = serde_json::from_str(data).unwrap();
        let builder = schema.create_model_builder(None, None).unwrap();

        let model = builder.build().unwrap();

        let network = model.network();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 5);
    }
}
