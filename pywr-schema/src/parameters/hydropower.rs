use crate::data_tables::LoadedTableCollection;
use crate::parameters::{
    DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
use crate::{ConversionError, SchemaError};
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::HydropowerTargetParameter as HydropowerTargetParameterV1;
use std::collections::HashMap;
use std::path::Path;

/// A parameter that returns flow from a hydropower generation target.
///
/// This parameter calculates the flow required to generate a given hydropower production target `P`. It
/// is intended to be used on a node representing a turbine where a particular production target
/// is required at each time-step. The parameter uses the following (hydropower) equation to calculate
/// the flow `q` required to produce `P`:
///
///    q = P / ( C<sub>E</sub> * ρ * g * H * δ * C<sub>F</sub>)
///
/// where:
///  - `q` is the flow needed to achieve `P`.
///  - `P` is the desired hydropower production target.
///  - C<sub>E</sub> is a coefficient to convert the energy unit.
///  - `ρ` is the water density.
///  - `g` is the gravitational acceleration (9.81 m s<sup>-2</sup>).
///  - `H` is the turbine head. If `water_elevation` is given, then the head is the difference between `water_elevation`
///     and `turbine_elevation`. If `water_elevation` is not provided, then the head is simply `turbine_elevation`.
///  - `δ` is the turbine efficiency.
///  - C<sub>E</sub> is a coefficient to convert the flow unit. Use the `flow_unit_conversion` parameter to convert `q`
///    from units of m<sup>3</sup> day<sup>-1</sup> to those used by the model.
///
/// # JSON Examples
/// The example below shows the definition of a [`HydropowerTargetParameter`] in JSON.
///
/// ```json
#[doc = include_str!("doc_examples/hydropower.json")]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct HydropowerTargetParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    /// Hydropower production target. This can be a constant, a value from a table, a
    /// parameter name or an inline parameter (see [`DynamicFloatValue`]). Units should be in
    /// units of energy per day.
    pub target: DynamicFloatValue,
    /// The elevation of water entering the turbine. The difference of this
    /// value with the `turbine_elevation` gives the working head of the turbine. This is optional
    /// and can be a constant, a value from a table, a parameter name or an inline parameter
    /// (see [`DynamicFloatValue`]).
    pub water_elevation: Option<DynamicFloatValue>,
    /// The elevation of the turbine. The difference between the `water_elevation` and this value
    /// gives the working head of the turbine. Default to `0.0`.
    pub turbine_elevation: Option<f64>,
    /// The minimum head for flow to occur. If the working head is less than this value, zero flow
    /// is returned. Default to `0.0`.
    pub min_head: Option<f64>,
    /// The upper bound on the calculated flow. If set the flow returned by this
    /// parameter will be at most this value. This is optional and can be a constant, a value from
    /// a table, a parameter name or an inline parameter (see [`DynamicFloatValue`]).
    pub max_flow: Option<DynamicFloatValue>,
    /// The lower bound on the calculated flow. If set the flow returned by this
    /// parameter will be at least this value. This is optional and can be a constant, a value from
    /// a table, a parameter name or an inline parameter (see [`DynamicFloatValue`]).
    pub min_flow: Option<DynamicFloatValue>,
    /// The efficiency of the turbine. Default to `1.0`.
    pub efficiency: Option<f64>,
    /// The density of water. Default to `1000.0`.
    pub water_density: Option<f64>,
    /// A factor used to transform the units of flow to be compatible with the equation above.
    /// This should convert flow to units of m<sup>3</sup> day<sup>-1</sup>. Default to `1.0`.
    pub flow_unit_conversion: Option<f64>,
    /// A factor used to transform the units of total energy. Defaults to 1e<sup>-6</sup> to return `MJ`.
    pub energy_unit_conversion: Option<f64>,
}

impl HydropowerTargetParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, SchemaError> {
        let target = self.target.load(model, tables, data_path)?;

        let water_elevation = self
            .water_elevation
            .as_ref()
            .map(|f| f.load(model, tables, data_path))
            .transpose()?;
        let max_flow = self
            .max_flow
            .as_ref()
            .map(|f| f.load(model, tables, data_path))
            .transpose()?;
        let min_flow = self
            .min_flow
            .as_ref()
            .map(|f| f.load(model, tables, data_path))
            .transpose()?;

        let p = pywr_core::parameters::HydropowerTargetParameter::new(
            &self.meta.name,
            target,
            water_elevation,
            self.turbine_elevation,
            self.min_head,
            max_flow,
            min_flow,
            self.efficiency,
            self.water_density,
            self.flow_unit_conversion,
            self.energy_unit_conversion,
        );
        Ok(model.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<HydropowerTargetParameterV1> for HydropowerTargetParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: HydropowerTargetParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);
        let target = v1.target.try_into_v2_parameter(Some(&meta.name), unnamed_count)?;
        let water_elevation = v1
            .water_elevation_parameter
            .map(|f| f.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .transpose()?;
        let min_flow = v1
            .min_flow
            .map(|f| f.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .transpose()?;
        let max_flow = v1
            .max_flow
            .map(|f| f.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .transpose()?;

        Ok(Self {
            meta,
            target,
            water_elevation,
            turbine_elevation: v1.turbine_elevation,
            min_head: v1.min_head,
            max_flow,
            min_flow,
            efficiency: v1.efficiency,
            water_density: v1.density,
            flow_unit_conversion: v1.flow_unit_conversion,
            energy_unit_conversion: v1.energy_unit_conversion,
        })
    }
}
