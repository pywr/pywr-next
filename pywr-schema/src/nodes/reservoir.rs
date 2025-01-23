use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta, StorageNode};
#[cfg(feature = "core")]
use crate::SchemaError;
#[cfg(feature = "core")]
use pywr_core::derived_metric::DerivedMetric;
#[cfg(feature = "core")]
use pywr_core::metric::ConstantMetricF64::Constant;
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
#[cfg(feature = "core")]
use pywr_core::metric::SimpleMetricF64;
#[cfg(feature = "core")]
use pywr_core::parameters::{AggFunc, ParameterName};
use pywr_schema_macros::PywrVisitAll;
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
    Interpolated { storage: Metric, area: Metric },
    /// The bathymetry is calculated using a polynomial expressions and the provided coefficients.
    Polynomial(Vec<f64>),
}

/// The bathymetric data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Bathymetry {
    /// The bathymetric data and type.
    pub data: BathymetryType,
    /// Whether the `storage` provided by the [`BathymetryType`] is relative (0-1).
    pub is_storage_relative: bool,
}

/// The evaporation data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Evaporation {
    /// The [`Metric`] containing the evaporation height.
    data: Metric,
    /// The cost to assign to the [`crate::nodes::OutputNode`].
    cost: Option<Metric>,
    /// If `true` the maximum surface area will be used to calculate the evaporation volume. When
    /// `false`, the area is calculated from the bathymetric data. This defaults to `false`.
    use_max_area: Option<bool>,
}

/// The leakage data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Leakage {
    /// The [`Metric`] containing the lost volume.
    loss: Metric,
    /// The cost to assign to the [`crate::nodes::OutputNode`].
    cost: Option<Metric>,
}

/// The rainfall data
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct Rainfall {
    /// The [`Metric`] containing the rainfall level.
    data: Metric,
    /// If `true` the maximum surface area will be used to calculate the rainfall volume. When
    /// `false`, the area is calculated from the bathymetric data. This defaults to `false`.
    use_max_area: Option<bool>,
}

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
///        |             |           <Reservoir>.Spill   -or-    <Reservoir>.Spill
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
/// and edge to this component is created without slots, the target nodes are directly connected to
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
/// - Evaporation: this is am optional [`pywr_core::node::OutputNode`] with a `max_flow` equal to
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
/// If `rainfall.use_max_area` is set to `true`, then the rainfall volume is calculated using the
/// maximum surface area only.
///
/// # Available metrics
/// The following metrics are available:
/// - Volume: to get the reservoir volume.
/// - ProportionalVolume: to get the reservoir relative volume (0-1).
/// - Compensation: to get the minimum residual flow when the `compensation` field is provided.
/// - Rainfall: to get the rainfall volume when the `rainfall` field is provided.
/// - Evaporation: to get the evaporation volume when the `rainfall` field is provided.
///
/// # JSON Examples
/// ## Reservoir with output spill
/// ```json
#[doc = include_str!("../../tests/reservoir_with_spill.json")]
/// ```
///
/// ## Reservoir with link spill
/// The compensation goes into the spill which routes water to the "River termination" node.
/// ```json
#[doc = include_str!("../../tests/reservoir_with_river.json")]
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
    pub bathymetry: Option<Bathymetry>,
    /// The rainfall data. Use `None` not to add the rainfall node.
    pub rainfall: Option<Rainfall>,
    /// The evaporation data. Use `None` not to add the evaporation node.
    pub evaporation: Option<Evaporation>,
    /// The leakage to set on the node. Use `None` not to add any loss.
    pub leakage: Option<Leakage>,
}

impl ReservoirNode {
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

    /// The name of the compensation slot.
    fn compensation_slot_name() -> &'static str {
        "compensation"
    }

    /// The name of the spill to_slot.
    fn spill_to_slot_name() -> &'static str {
        "to_spill"
    }

    /// The name of the spill from_slot.
    fn spill_from_slot_name() -> &'static str {
        "from_spill"
    }

    pub fn input_connectors(&self, slot: Option<&str>) -> Vec<(&str, Option<String>)> {
        match slot {
            None => vec![(self.meta().name.as_str(), None)],
            Some(name) => match name {
                name if name == Self::spill_to_slot_name() => vec![(
                    self.meta().name.as_str(),
                    Self::spill_node_sub_name().map(|n| n.to_string()),
                )],
                _ => panic!("The slot '{name}' does not exist in {}", self.meta().name),
            },
        }
    }

    pub fn output_connectors(&self, slot: Option<&str>) -> Vec<(&str, Option<String>)> {
        match slot {
            None => vec![(self.meta().name.as_str(), None)],
            Some(name) => match name {
                name if name == Self::compensation_slot_name() => vec![(
                    self.meta().name.as_str(),
                    Self::compensation_node_sub_name().map(|n| n.to_string()),
                )],
                name if name == Self::spill_from_slot_name() => {
                    if let Some(SpillNodeType::OutputNode) = self.spill {
                        panic!(
                            "The slot '{name}' in {} is only supported when the spill node is a link",
                            self.meta().name
                        )
                    }
                    vec![(
                        self.meta().name.as_str(),
                        Self::spill_node_sub_name().map(|n| n.to_string()),
                    )]
                }
                _ => panic!("The slot '{name}' does not exist in {}", self.meta().name),
            },
        }
    }

    pub fn default_metric(&self) -> NodeAttribute {
        self.storage.default_metric()
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

    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = vec![
            network.get_node_index_by_name(self.meta().name.as_str(), Self::compensation_node_sub_name())?,
            network.get_node_index_by_name(self.meta().name.as_str(), Self::leakage_node_sub_name())?,
            network.get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())?,
            network.get_node_index_by_name(self.meta().name.as_str(), Self::evaporation_node_sub_name())?,
        ];
        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // add storage and spill
        self.storage.add_to_model(network)?;
        let storage = network.get_node_index_by_name(self.meta().name.as_str(), None)?;

        // add compensation node and edge
        let comp_node = match &self.compensation {
            Some(_) => {
                network.add_link_node(self.meta().name.as_str(), Self::compensation_node_sub_name())?;
                let comp =
                    network.get_node_index_by_name(self.meta().name.as_str(), Self::compensation_node_sub_name())?;
                network.connect_nodes(storage, comp)?;
                Some(comp)
            }
            None => None,
        };

        // add spill and edge
        let spill_node = match &self.spill {
            None => None,
            Some(node_type) => {
                match node_type {
                    SpillNodeType::OutputNode => {
                        network.add_output_node(self.meta().name.as_str(), Self::spill_node_sub_name())?;
                    }
                    SpillNodeType::LinkNode => {
                        network.add_link_node(self.meta().name.as_str(), Self::spill_node_sub_name())?;
                    }
                }
                let spill = network.get_node_index_by_name(self.meta().name.as_str(), Self::spill_node_sub_name())?;
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
            network.add_input_node(self.meta().name.as_str(), Self::rainfall_node_sub_name())?;
            let rainfall = network.get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())?;
            network.connect_nodes(rainfall, storage)?;

            if self.bathymetry.is_none() {
                return Err(SchemaError::MissingNodeAttribute {
                    attr: "bathymetry".to_string(),
                    name: self.meta().name.clone(),
                });
            }
        }

        // add evaporation node and edge
        if self.evaporation.is_some() {
            network.add_output_node(self.meta().name.as_str(), Self::evaporation_node_sub_name())?;
            let evaporation =
                network.get_node_index_by_name(self.meta().name.as_str(), Self::evaporation_node_sub_name())?;
            network.connect_nodes(storage, evaporation)?;
        }

        // add leakage node and edge
        if self.leakage.is_some() {
            network.add_output_node(self.meta().name.as_str(), Self::leakage_node_sub_name())?;
            let leakage = network.get_node_index_by_name(self.meta().name.as_str(), Self::leakage_node_sub_name())?;
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
        if let Some(bathymetry) = &self.bathymetry {
            // add the rainfall
            if let Some(rainfall) = &self.rainfall {
                let use_max_area = rainfall.use_max_area.unwrap_or(false);
                let rainfall_area_metric =
                    self.get_area_metric(network, args, "rainfall_area", bathymetry, use_max_area)?;
                let rainfall_metric = rainfall.data.load(network, args, Some(&self.meta().name))?;

                let rainfall_volume_parameter = pywr_core::parameters::AggregatedParameter::new(
                    ParameterName::new("rainfall", Some(self.meta().name.as_str())),
                    &[rainfall_metric, rainfall_area_metric],
                    AggFunc::Product,
                );
                let rainfall_idx = network.add_parameter(Box::new(rainfall_volume_parameter))?;
                let rainfall_volume_metric: MetricF64 = rainfall_idx.into();

                network.set_node_min_flow(
                    self.meta().name.as_str(),
                    Self::rainfall_node_sub_name(),
                    Some(rainfall_volume_metric.clone()),
                )?;
                network.set_node_max_flow(
                    self.meta().name.as_str(),
                    Self::rainfall_node_sub_name(),
                    Some(rainfall_volume_metric),
                )?;
            }

            // add the evaporation
            if let Some(evaporation) = &self.evaporation {
                let use_max_area = evaporation.use_max_area.unwrap_or(false);
                let evaporation_area_metric =
                    self.get_area_metric(network, args, "evaporation_area", bathymetry, use_max_area)?;

                // add volume to output node
                let evaporation_metric = evaporation.data.load(network, args, Some(&self.meta().name))?;
                let evaporation_volume_parameter = pywr_core::parameters::AggregatedParameter::new(
                    ParameterName::new("evaporation", Some(self.meta().name.as_str())),
                    &[evaporation_metric, evaporation_area_metric],
                    AggFunc::Product,
                );
                let evaporation_idx = network.add_parameter(Box::new(evaporation_volume_parameter))?;
                let evaporation_volume_metric: MetricF64 = evaporation_idx.into();

                network.set_node_max_flow(
                    self.meta().name.as_str(),
                    Self::evaporation_node_sub_name(),
                    Some(evaporation_volume_metric),
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
        let storage_node = network.get_node_index_by_name(self.meta().name.as_str(), None)?;

        // the storage (absolute or relative) can be the current or max volume
        let current_storage = match (bathymetry.is_storage_relative, use_max_area) {
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
                let storage_metric = storage.load(network, args, Some(&self.meta().name))?;
                let area_metric = area.load(network, args, Some(&self.meta().name))?;

                let interpolated_area_parameter = pywr_core::parameters::InterpolatedParameter::new(
                    ParameterName::new(name, Some(self.meta().name.as_str())),
                    current_storage,
                    vec![(storage_metric, area_metric.clone())],
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
                let attr = attribute.unwrap();
                let metric = match attr {
                    NodeAttribute::Compensation => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::compensation_node_sub_name())
                    {
                        Ok(idx) => MetricF64::NodeInFlow(idx),
                        Err(_) => 0.0.into(),
                    },
                    NodeAttribute::Rainfall => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())
                    {
                        Ok(idx) => MetricF64::NodeInFlow(idx),
                        Err(_) => 0.0.into(),
                    },
                    NodeAttribute::Evaporation => match network
                        .get_node_index_by_name(self.meta().name.as_str(), Self::rainfall_node_sub_name())
                    {
                        Ok(idx) => MetricF64::NodeInFlow(idx),
                        Err(_) => 0.0.into(),
                    },
                    _ => {
                        return Err(SchemaError::NodeAttributeNotSupported {
                            ty: "ReservoirNode".to_string(),
                            name: self.meta().name.clone(),
                            attr,
                        })
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
    use crate::model::PywrModel;

    fn reservoir_with_spill_str() -> &'static str {
        include_str!("../../tests/reservoir_with_spill.json")
    }

    #[test]
    fn test_model_nodes_and_edges() {
        let data = reservoir_with_spill_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model: pywr_core::models::Model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 5);
    }
}
