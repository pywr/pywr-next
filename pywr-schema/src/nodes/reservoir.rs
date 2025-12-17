use crate::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot, StorageNode, StorageNodeAttribute};
use crate::parameters::ConstantFloatVec;
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::agg_funcs::AggFuncF64;
#[cfg(feature = "core")]
use pywr_core::derived_metric::DerivedMetric;
#[cfg(feature = "core")]
use pywr_core::metric::ConstantMetricF64::Constant;
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
#[cfg(feature = "core")]
use pywr_core::metric::SimpleMetricF64;
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterName;
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
#[doc = svgbobdoc::transform!(
/// ```svgbob
///
///             <Reservoir>.Rainfall
///                      *
///     U                |            if OutputNode                if LinkNode
/// - - *--.             |                                         D[from_spill]
///        |             |           <Reservoir>.Spill   - or -     <Reservoir>.Spill
///        V             V             D[to_spill]                 D[to_spill]
///        .----------------+----------------->o               --------->*- - -
///        |                |
///        |  StorageNode   |
///        |                |-------------->o   <Reservoir>.Evaporation
///        +-------------+--+---------------------------.
///        |             |                              |
///        |             +--------->*- -                +------>o  <Reservoir>.Leakage
///        |               <Reservoir>.Compensation
///        |                   D[compensation]
///        +---->*- - -
///             D[<default>]
///
/// ```
)]
///
/// This is a [`StorageNode`] connected to an upstream node `U` and downstream network node `D`. When
/// an edge to this component is created without slots, the target nodes are directly connected to
/// `D[<default>]`.
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
/// ```json
#[doc = include_str!("../../tests/reservoir_with_spill1.json")]
/// ```
///
/// ## Reservoir with link spill
/// The compensation goes into the spill which routes water to the "River termination" node.
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
    const DEFAULT_OUTPUT_SLOT: ReservoirOutputNodeSlot = ReservoirOutputNodeSlot::Storage;

    /// Get the node's metadata.
    pub(crate) fn meta(&self) -> &NodeMeta {
        &self.storage.meta
    }

    /// The sub-name of the compensation link node.
    fn compensation_node_sub_name() -> Option<&'static str> {
        Some("compensation")
    }

    /// The sub-name of the spill output node.
    fn spill_node_sub_name() -> Option<&'static str> {
        Some("spill")
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![(self.meta().name.as_str(), None)])
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        // Use the default slot if none is specified
        let slot = match slot {
            Some(c) => c.clone().try_into()?,
            None => Self::DEFAULT_OUTPUT_SLOT,
        };

        let indices = match slot {
            ReservoirOutputNodeSlot::Storage => {
                vec![(self.meta().name.as_str(), None)]
            }
            ReservoirOutputNodeSlot::Compensation => {
                vec![(
                    self.meta().name.as_str(),
                    Self::compensation_node_sub_name().map(|n| n.to_string()),
                )]
            }
            ReservoirOutputNodeSlot::Spill => match self.spill {
                Some(SpillNodeType::LinkNode) => vec![(
                    self.meta().name.as_str(),
                    Self::spill_node_sub_name().map(|n| n.to_string()),
                )],
                _ => {
                    return Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.into() });
                }
            },
        };

        Ok(indices)
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
    /// The sub-name of the rainfall node.
    fn rainfall_node_sub_name() -> Option<&'static str> {
        Some("rainfall")
    }

    /// The sub-name of the evaporation node.
    fn evaporation_node_sub_name() -> Option<&'static str> {
        Some("evaporation")
    }

    /// The sub-name of the leakage node.
    fn leakage_node_sub_name() -> Option<&'static str> {
        Some("leakage")
    }

    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let idx = match component {
            ReservoirNodeComponent::Loss => network
                .get_node_index_by_name(self.meta().name.as_str(), Self::leakage_node_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta().name.clone(),
                    sub_name: Self::leakage_node_sub_name().map(String::from),
                })?,
            ReservoirNodeComponent::Compensation => network
                .get_node_index_by_name(self.meta().name.as_str(), Self::compensation_node_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta().name.clone(),
                    sub_name: Self::compensation_node_sub_name().map(String::from),
                })?,
            ReservoirNodeComponent::Rainfall => network
                .get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta().name.clone(),
                    sub_name: Self::rainfall_node_sub_name().map(String::from),
                })?,
            ReservoirNodeComponent::Evaporation => network
                .get_node_index_by_name(self.meta().name.as_str(), Self::evaporation_node_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta().name.clone(),
                    sub_name: Self::evaporation_node_sub_name().map(String::from),
                })?,
        };

        Ok(vec![idx])
    }

    pub fn node_indices_for_storage_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = vec![
            network
                .get_node_index_by_name(self.meta().name.as_str(), None)
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta().name.clone(),
                    sub_name: Self::compensation_node_sub_name().map(String::from),
                })?,
        ];
        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // add storage and spill
        self.storage.add_to_model(network)?;
        let storage = network
            .get_node_index_by_name(self.meta().name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta().name.clone(),
                sub_name: Self::evaporation_node_sub_name().map(String::from),
            })?;

        // add compensation node and edge
        let comp_node = match &self.compensation {
            Some(_) => {
                let comp = network.add_link_node(self.meta().name.as_str(), Self::compensation_node_sub_name())?;

                network.connect_nodes(storage, comp)?;
                Some(comp)
            }
            None => None,
        };

        // add spill and edge
        let spill_node = match &self.spill {
            None => None,
            Some(node_type) => {
                let spill = match node_type {
                    SpillNodeType::OutputNode => {
                        network.add_output_node(self.meta().name.as_str(), Self::spill_node_sub_name())?
                    }
                    SpillNodeType::LinkNode => {
                        network.add_link_node(self.meta().name.as_str(), Self::spill_node_sub_name())?
                    }
                };

                network.connect_nodes(storage, spill)?;
                Some(spill)
            }
        };

        // connect compensation and spill
        let connect_comp = self.connect_compensation_to_spill.unwrap_or(true);
        if connect_comp {
            if let (Some(spill), Some(comp)) = (spill_node, comp_node) {
                network.connect_nodes(comp, spill)?;
            }
        }
        // add rainfall node and edge
        if self.rainfall.is_some() {
            if self.surface_area.is_none() {
                return Err(SchemaError::MissingNodeAttribute {
                    attr: "surface_area".to_string(),
                    name: self.meta().name.clone(),
                });
            }

            let rainfall = network.add_input_node(self.meta().name.as_str(), Self::rainfall_node_sub_name())?;
            network.connect_nodes(rainfall, storage)?;
        }

        // add evaporation node and edge
        if self.evaporation.is_some() {
            if self.surface_area.is_none() {
                return Err(SchemaError::MissingNodeAttribute {
                    attr: "surface_area".to_string(),
                    name: self.meta().name.clone(),
                });
            }

            let evaporation = network.add_output_node(self.meta().name.as_str(), Self::evaporation_node_sub_name())?;
            network.connect_nodes(storage, evaporation)?;
        }

        // add leakage node and edge
        if self.leakage.is_some() {
            let leakage = network.add_output_node(self.meta().name.as_str(), Self::leakage_node_sub_name())?;
            network.connect_nodes(storage, leakage)?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        self.storage.set_constraints(network, args)?;

        // Set compensation
        if let Some(compensation) = &self.compensation {
            let value = compensation.load(network, args, Some(&self.meta().name))?;
            network.set_node_min_flow(
                self.meta().name.as_str(),
                Self::compensation_node_sub_name(),
                value.into(),
            )?;
        }

        // add leakage
        if let Some(leakage) = &self.leakage {
            let value = leakage.loss.load(network, args, Some(&self.meta().name))?;
            network.set_node_max_flow(self.meta().name.as_str(), Self::leakage_node_sub_name(), value.into())?;
            if let Some(cost) = &leakage.cost {
                let value = cost.load(network, args, Some(&self.meta().name))?;
                network.set_node_cost(self.meta().name.as_str(), Self::leakage_node_sub_name(), value.into())?;
            }
        }

        // add rainfall and evaporation
        if let Some(bathymetry) = &self.surface_area {
            // add the rainfall
            if let Some(rainfall) = &self.rainfall {
                let use_max_area = rainfall.use_max_area.unwrap_or(false);
                let rainfall_area_metric =
                    self.get_area_metric(network, args, "rainfall_area", bathymetry, use_max_area)?;
                let rainfall_metric = rainfall.data.load(network, args, Some(&self.meta().name))?;

                let rainfall_flow_parameter = pywr_core::parameters::AggregatedParameter::new(
                    ParameterName::new("rainfall", Some(self.meta().name.as_str())),
                    &[rainfall_metric, rainfall_area_metric],
                    AggFuncF64::Product,
                );
                let rainfall_idx = network.add_parameter(Box::new(rainfall_flow_parameter))?;
                let rainfall_flow_metric: MetricF64 = rainfall_idx.into();

                network.set_node_min_flow(
                    self.meta().name.as_str(),
                    Self::rainfall_node_sub_name(),
                    Some(rainfall_flow_metric.clone()),
                )?;
                network.set_node_max_flow(
                    self.meta().name.as_str(),
                    Self::rainfall_node_sub_name(),
                    Some(rainfall_flow_metric),
                )?;
            }

            // add the evaporation
            if let Some(evaporation) = &self.evaporation {
                let use_max_area = evaporation.use_max_area.unwrap_or(false);
                let evaporation_area_metric =
                    self.get_area_metric(network, args, "evaporation_area", bathymetry, use_max_area)?;

                // add volume to output node
                let evaporation_metric = evaporation.data.load(network, args, Some(&self.meta().name))?;
                let evaporation_flow_parameter = pywr_core::parameters::AggregatedParameter::new(
                    ParameterName::new("evaporation", Some(self.meta().name.as_str())),
                    &[evaporation_metric, evaporation_area_metric],
                    AggFuncF64::Product,
                );
                let evaporation_idx = network.add_parameter(Box::new(evaporation_flow_parameter))?;
                let evaporation_flow_metric: MetricF64 = evaporation_idx.into();

                network.set_node_max_flow(
                    self.meta().name.as_str(),
                    Self::evaporation_node_sub_name(),
                    Some(evaporation_flow_metric),
                )?;

                // set optional cost
                if let Some(cost) = &evaporation.cost {
                    let value = cost.load(network, args, Some(&self.meta().name))?;
                    network.set_node_cost(
                        self.meta().name.as_str(),
                        Self::evaporation_node_sub_name(),
                        Some(value),
                    )?;
                }
            }
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
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        name: &str,
        bathymetry: &Bathymetry,
        use_max_area: bool,
    ) -> Result<MetricF64, SchemaError> {
        // get the current storage
        let storage_node = network
            .get_node_index_by_name(self.meta().name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta().name.clone(),
                sub_name: None,
            })?;

        // the storage (absolute or relative) can be the current or max volume
        let current_storage = match (bathymetry.is_storage_proportional, use_max_area) {
            (false, false) => MetricF64::NodeVolume(storage_node),
            (true, false) => {
                let dm = DerivedMetric::NodeProportionalVolume(storage_node);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            (false, true) => MetricF64::NodeMaxVolume(storage_node),
            (true, true) => MetricF64::Simple(SimpleMetricF64::Constant(Constant(1.0))),
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

                let interpolated_area_parameter = pywr_core::parameters::InterpolatedParameter::new(
                    ParameterName::new(name, Some(self.meta().name.as_str())),
                    current_storage,
                    points,
                    true,
                );
                let area_idx = network.add_parameter(Box::new(interpolated_area_parameter))?;
                let interpolated_area_metric: MetricF64 = area_idx.into();
                interpolated_area_metric
            }
            BathymetryType::Polynomial(coeffs) => {
                let poly_area_parameter = pywr_core::parameters::Polynomial1DParameter::new(
                    ParameterName::new(name, Some(self.meta().name.as_str())),
                    current_storage,
                    coeffs.clone(),
                    1.0,
                    0.0,
                );
                let area_idx = network.add_parameter(Box::new(poly_area_parameter))?;
                let poly_area_metric: MetricF64 = area_idx.into();
                poly_area_metric
            }
        };

        Ok(area_metric)
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        match self.storage.create_metric(network, attribute) {
            Ok(m) => Ok(m),
            Err(SchemaError::NodeAttributeNotSupported { .. }) => {
                let attr = match attribute {
                    Some(attr) => attr.try_into()?,
                    None => self.default_attribute(),
                };

                let metric = match attr {
                    ReservoirNodeAttribute::Compensation => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::compensation_node_sub_name())
                    {
                        Some(idx) => MetricF64::NodeInFlow(idx),
                        None => 0.0.into(),
                    },
                    ReservoirNodeAttribute::Rainfall => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())
                    {
                        Some(idx) => MetricF64::NodeInFlow(idx),
                        None => 0.0.into(),
                    },
                    ReservoirNodeAttribute::Evaporation => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())
                    {
                        Some(idx) => MetricF64::NodeInFlow(idx),
                        None => 0.0.into(),
                    },
                    ReservoirNodeAttribute::Volume => {
                        let idx = network
                            .get_node_index_by_name(self.meta().name.as_str(), None)
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta().name.clone(),
                                sub_name: None,
                            })?;
                        MetricF64::NodeVolume(idx)
                    }
                    ReservoirNodeAttribute::ProportionalVolume => {
                        let idx = network
                            .get_node_index_by_name(self.meta().name.as_str(), None)
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta().name.clone(),
                                sub_name: None,
                            })?;
                        let dm = DerivedMetric::NodeProportionalVolume(idx);
                        let derived_metric_idx = network.add_derived_metric(dm);
                        MetricF64::DerivedMetric(derived_metric_idx)
                    }
                    ReservoirNodeAttribute::MaxVolume => {
                        let idx = network
                            .get_node_index_by_name(self.meta().name.as_str(), None)
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta().name.clone(),
                                sub_name: None,
                            })?;
                        MetricF64::NodeMaxVolume(idx)
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
        let mut model: pywr_core::models::Model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 5);
    }
}
