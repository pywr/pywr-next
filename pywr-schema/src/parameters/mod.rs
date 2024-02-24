//! Parameter schema definitions.
//!
//! The enum [`Parameter`] contains all of the valid Pywr parameter schemas. The parameter
//! variants define separate schemas for different parameter types. When a network is generated
//! from a schema the parameter schemas are added to the network using [`Parameter::add_to_model`].
//! This typically adds a struct from [`crate::parameters`] to the network using the data
//! defined in the schema.
//!
//! Serializing and deserializing is accomplished using [`serde`].
mod aggregated;
mod asymmetric_switch;
mod control_curves;
mod core;
mod data_frame;
mod delay;
mod discount_factor;
mod indexed_array;
mod interpolated;
mod offset;
mod polynomial;
mod profiles;
mod python;
mod tables;
mod thresholds;

pub use super::data_tables::{LoadedTableCollection, TableDataRef};
pub use super::parameters::aggregated::{AggFunc, AggregatedIndexParameter, AggregatedParameter, IndexAggFunc};
pub use super::parameters::asymmetric_switch::AsymmetricSwitchIndexParameter;
pub use super::parameters::control_curves::{
    ControlCurveIndexParameter, ControlCurveInterpolatedParameter, ControlCurveParameter,
    ControlCurvePiecewiseInterpolatedParameter,
};
pub use super::parameters::core::{
    ActivationFunction, ConstantParameter, MaxParameter, MinParameter, NegativeParameter, VariableSettings,
};
pub use super::parameters::delay::DelayParameter;
pub use super::parameters::discount_factor::DiscountFactorParameter;
pub use super::parameters::indexed_array::IndexedArrayParameter;
pub use super::parameters::polynomial::Polynomial1DParameter;
pub use super::parameters::profiles::{
    DailyProfileParameter, MonthlyProfileParameter, RadialBasisFunction, RbfProfileParameter,
    RbfProfileVariableSettings, UniformDrawdownProfileParameter, WeeklyProfileParameter,
};
pub use super::parameters::python::PythonParameter;
pub use super::parameters::tables::TablesArrayParameter;
pub use super::parameters::thresholds::ParameterThresholdParameter;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::nodes::NodeAttribute;
use crate::parameters::core::DivisionParameter;
pub use crate::parameters::data_frame::DataFrameParameter;
use crate::parameters::interpolated::InterpolatedParameter;
pub use offset::OffsetParameter;
use pywr_core::metric::Metric;
use pywr_core::models::{ModelDomain, MultiNetworkTransferIndex};
use pywr_core::parameters::{IndexParameterIndex, IndexValue, ParameterType};
use pywr_v1_schema::parameters::{
    CoreParameter, ExternalDataRef as ExternalDataRefV1, Parameter as ParameterV1, ParameterMeta as ParameterMetaV1,
    ParameterValue as ParameterValueV1, TableIndex as TableIndexV1, TableIndexEntry as TableIndexEntryV1,
};
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ParameterMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

pub trait FromV1Parameter<T>: Sized {
    fn from_v1_parameter(v1: T, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self;
}

pub trait IntoV2Parameter<T> {
    fn into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> T;
}

// FromV1Parameter implies IntoV2Parameter
impl<T, U> IntoV2Parameter<U> for T
where
    U: FromV1Parameter<T>,
{
    fn into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> U {
        U::from_v1_parameter(self, parent_node, unnamed_count)
    }
}

pub trait TryFromV1Parameter<T>: Sized {
    type Error;
    fn try_from_v1_parameter(v1: T, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<Self, Self::Error>;
}

pub trait TryIntoV2Parameter<T> {
    type Error;
    fn try_into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<T, Self::Error>;
}

// TryFromV1Parameter implies TryIntoV2Parameter
impl<T, U> TryIntoV2Parameter<U> for T
where
    U: TryFromV1Parameter<T>,
{
    type Error = U::Error;

    fn try_into_v2_parameter(self, parent_node: Option<&str>, unnamed_count: &mut usize) -> Result<U, Self::Error> {
        U::try_from_v1_parameter(self, parent_node, unnamed_count)
    }
}

impl FromV1Parameter<ParameterMetaV1> for ParameterMeta {
    fn from_v1_parameter(v1: ParameterMetaV1, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self {
        Self {
            name: v1.name.unwrap_or_else(|| {
                let pname = match parent_node {
                    Some(pn) => format!("{pn}-p{unnamed_count}"),
                    None => format!("unnamed-{unnamed_count}"),
                };
                *unnamed_count += 1;
                pname
            }),
            comment: v1.comment,
        }
    }
}

impl FromV1Parameter<Option<ParameterMetaV1>> for ParameterMeta {
    fn from_v1_parameter(v1: Option<ParameterMetaV1>, parent_node: Option<&str>, unnamed_count: &mut usize) -> Self {
        match v1 {
            Some(meta) => meta.into_v2_parameter(parent_node, unnamed_count),
            None => {
                let meta = Self {
                    name: format!("unnamed-{unnamed_count}"),
                    comment: None,
                };
                *unnamed_count += 1;
                meta
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Parameter {
    Aggregated(AggregatedParameter),
    AggregatedIndex(AggregatedIndexParameter),
    AsymmetricSwitchIndex(AsymmetricSwitchIndexParameter),
    Constant(ConstantParameter),
    ControlCurvePiecewiseInterpolated(ControlCurvePiecewiseInterpolatedParameter),
    ControlCurveInterpolated(ControlCurveInterpolatedParameter),
    ControlCurveIndex(ControlCurveIndexParameter),
    ControlCurve(ControlCurveParameter),
    DailyProfile(DailyProfileParameter),
    IndexedArray(IndexedArrayParameter),
    MonthlyProfile(MonthlyProfileParameter),
    WeeklyProfile(WeeklyProfileParameter),
    UniformDrawdownProfile(UniformDrawdownProfileParameter),
    Max(MaxParameter),
    Min(MinParameter),
    Negative(NegativeParameter),
    Polynomial1D(Polynomial1DParameter),
    ParameterThreshold(ParameterThresholdParameter),
    TablesArray(TablesArrayParameter),
    Python(PythonParameter),
    DataFrame(DataFrameParameter),
    Delay(DelayParameter),
    Division(DivisionParameter),
    Offset(OffsetParameter),
    DiscountFactor(DiscountFactorParameter),
    Interpolated(InterpolatedParameter),
    RbfProfile(RbfProfileParameter),
}

impl Parameter {
    pub fn name(&self) -> &str {
        match self {
            Self::Constant(p) => p.meta.name.as_str(),
            Self::ControlCurveInterpolated(p) => p.meta.name.as_str(),
            Self::Aggregated(p) => p.meta.name.as_str(),
            Self::AggregatedIndex(p) => p.meta.name.as_str(),
            Self::AsymmetricSwitchIndex(p) => p.meta.name.as_str(),
            Self::ControlCurvePiecewiseInterpolated(p) => p.meta.name.as_str(),
            Self::ControlCurveIndex(p) => p.meta.name.as_str(),
            Self::ControlCurve(p) => p.meta.name.as_str(),
            Self::DailyProfile(p) => p.meta.name.as_str(),
            Self::IndexedArray(p) => p.meta.name.as_str(),
            Self::MonthlyProfile(p) => p.meta.name.as_str(),
            Self::WeeklyProfile(p) => p.meta.name.as_str(),
            Self::UniformDrawdownProfile(p) => p.meta.name.as_str(),
            Self::Max(p) => p.meta.name.as_str(),
            Self::Min(p) => p.meta.name.as_str(),
            Self::Negative(p) => p.meta.name.as_str(),
            Self::Polynomial1D(p) => p.meta.name.as_str(),
            Self::ParameterThreshold(p) => p.meta.name.as_str(),
            Self::TablesArray(p) => p.meta.name.as_str(),
            Self::Python(p) => p.meta.name.as_str(),
            Self::DataFrame(p) => p.meta.name.as_str(),
            Self::Division(p) => p.meta.name.as_str(),
            Self::Delay(p) => p.meta.name.as_str(),
            Self::Offset(p) => p.meta.name.as_str(),
            Self::DiscountFactor(p) => p.meta.name.as_str(),
            Self::Interpolated(p) => p.meta.name.as_str(),
            Self::RbfProfile(p) => p.meta.name.as_str(),
        }
    }

    pub fn ty(&self) -> &str {
        match self {
            Self::Constant(_) => "Constant",
            Self::ControlCurveInterpolated(_) => "ControlCurveInterpolated",
            Self::Aggregated(_) => "Aggregated",
            Self::AggregatedIndex(_) => "AggregatedIndex",
            Self::AsymmetricSwitchIndex(_) => "AsymmetricSwitchIndex",
            Self::ControlCurvePiecewiseInterpolated(_) => "ControlCurvePiecewiseInterpolated",
            Self::ControlCurveIndex(_) => "ControlCurveIndex",
            Self::ControlCurve(_) => "ControlCurve",
            Self::DailyProfile(_) => "DailyProfile",
            Self::IndexedArray(_) => "IndexedArray",
            Self::MonthlyProfile(_) => "MonthlyProfile",
            Self::WeeklyProfile(_) => "WeeklyProfile",
            Self::UniformDrawdownProfile(_) => "UniformDrawdownProfile",
            Self::Max(_) => "Max",
            Self::Min(_) => "Min",
            Self::Negative(_) => "Negative",
            Self::Polynomial1D(_) => "Polynomial1D",
            Self::ParameterThreshold(_) => "ParameterThreshold",
            Self::TablesArray(_) => "TablesArray",
            Self::Python(_) => "Python",
            Self::DataFrame(_) => "DataFrame",
            Self::Delay(_) => "Delay",
            Self::Division(_) => "Division",
            Self::Offset(_) => "Offset",
            Self::DiscountFactor(_) => "DiscountFactor",
            Self::Interpolated(_) => "Interpolated",
            Self::RbfProfile(_) => "RbfProfile",
        }
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<ParameterType, SchemaError> {
        let ty = match self {
            Self::Constant(p) => ParameterType::Parameter(p.add_to_model(network, tables)?),
            Self::ControlCurveInterpolated(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Aggregated(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::AggregatedIndex(p) => ParameterType::Index(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::AsymmetricSwitchIndex(p) => ParameterType::Index(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::ControlCurvePiecewiseInterpolated(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::ControlCurveIndex(p) => ParameterType::Index(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::ControlCurve(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::DailyProfile(p) => ParameterType::Parameter(p.add_to_model(network, tables)?),
            Self::IndexedArray(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::MonthlyProfile(p) => ParameterType::Parameter(p.add_to_model(network, tables)?),
            Self::WeeklyProfile(p) => ParameterType::Parameter(p.add_to_model(network, tables)?),
            Self::UniformDrawdownProfile(p) => ParameterType::Parameter(p.add_to_model(network, tables)?),
            Self::Max(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Min(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Negative(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Polynomial1D(p) => ParameterType::Parameter(p.add_to_model(network)?),
            Self::ParameterThreshold(p) => ParameterType::Index(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::TablesArray(p) => ParameterType::Parameter(p.add_to_model(network, domain, data_path)?),
            Self::Python(p) => p.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)?,
            Self::DataFrame(p) => ParameterType::Parameter(p.add_to_model(network, domain, data_path)?),
            Self::Delay(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Division(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Offset(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::DiscountFactor(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::Interpolated(p) => ParameterType::Parameter(p.add_to_model(
                network,
                schema,
                domain,
                tables,
                data_path,
                inter_network_transfers,
            )?),
            Self::RbfProfile(p) => ParameterType::Parameter(p.add_to_model(network)?),
        };

        Ok(ty)
    }
}

impl TryFromV1Parameter<ParameterV1> for Parameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterV1::Core(v1) => match v1 {
                CoreParameter::Aggregated(p) => {
                    Parameter::Aggregated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::AggregatedIndex(p) => {
                    Parameter::AggregatedIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::AsymmetricSwitchIndex(p) => {
                    Parameter::AsymmetricSwitchIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Constant(p) => Parameter::Constant(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::ControlCurvePiecewiseInterpolated(p) => {
                    Parameter::ControlCurvePiecewiseInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurveInterpolated(p) => {
                    Parameter::ControlCurveInterpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurveIndex(p) => {
                    Parameter::ControlCurveIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ControlCurve(p) => match p.clone().try_into_v2_parameter(parent_node, unnamed_count) {
                    Ok(p) => Parameter::ControlCurve(p),
                    Err(_) => Parameter::ControlCurveIndex(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                },
                CoreParameter::DailyProfile(p) => {
                    Parameter::DailyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::IndexedArray(p) => {
                    Parameter::IndexedArray(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::MonthlyProfile(p) => {
                    Parameter::MonthlyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::UniformDrawdownProfile(p) => {
                    Parameter::UniformDrawdownProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Max(p) => Parameter::Max(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::Negative(p) => Parameter::Negative(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::Polynomial1D(p) => {
                    Parameter::Polynomial1D(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::ParameterThreshold(p) => {
                    Parameter::ParameterThreshold(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::TablesArray(p) => {
                    Parameter::TablesArray(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Min(p) => Parameter::Min(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::Division(p) => Parameter::Division(p.try_into_v2_parameter(parent_node, unnamed_count)?),
                CoreParameter::DataFrame(p) => {
                    Parameter::DataFrame(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Deficit(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "DeficitParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::DiscountFactor(p) => {
                    Parameter::DiscountFactor(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::InterpolatedVolume(p) => {
                    Parameter::Interpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::InterpolatedFlow(p) => {
                    Parameter::Interpolated(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::NegativeMax(_) => todo!("Implement NegativeMaxParameter"),
                CoreParameter::NegativeMin(_) => todo!("Implement NegativeMinParameter"),
                CoreParameter::HydropowerTarget(_) => todo!("Implement HydropowerTargetParameter"),
                CoreParameter::WeeklyProfile(p) => {
                    Parameter::WeeklyProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
                CoreParameter::Storage(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "StorageParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::RollingMeanFlowNode(_) => todo!("Implement RollingMeanFlowNodeParameter"),
                CoreParameter::ScenarioWrapper(_) => todo!("Implement ScenarioWrapperParameter"),
                CoreParameter::Flow(p) => {
                    return Err(ConversionError::DeprecatedParameter {
                        ty: "FlowParameter".to_string(),
                        name: p.meta.and_then(|m| m.name).unwrap_or("unnamed".to_string()),
                        instead: "Use a derived metric instead.".to_string(),
                    })
                }
                CoreParameter::RbfProfile(p) => {
                    Parameter::RbfProfile(p.try_into_v2_parameter(parent_node, unnamed_count)?)
                }
            },
            ParameterV1::Custom(p) => {
                println!("Custom parameter: {:?} ({})", p.meta.name, p.ty);
                // TODO do something better with custom parameters

                let mut comment = format!("V1 CUSTOM PARAMETER ({}) UNCONVERTED!", p.ty);
                if let Some(c) = p.meta.comment {
                    comment.push_str(" ORIGINAL COMMENT: ");
                    comment.push_str(c.as_str());
                }

                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: p.meta.name.unwrap_or_else(|| "unnamed-custom-parameter".to_string()),
                        comment: Some(comment),
                    },
                    value: ConstantValue::Literal(0.0),
                })
            }
        };

        Ok(p)
    }
}

/// An non-variable constant floating-point (f64) value
///
/// This value can be a literal float or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConstantValue<T> {
    Literal(T),
    Table(TableDataRef),
    External(ExternalDataRef),
}

impl Default for ConstantValue<f64> {
    fn default() -> Self {
        Self::Literal(0.0)
    }
}

impl ConstantValue<f64> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<f64, SchemaError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_f64(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl ConstantValue<usize> {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<usize, SchemaError> {
        match self {
            Self::Literal(v) => Ok(*v),
            Self::Table(tbl_ref) => Ok(tables.get_scalar_usize(tbl_ref)?),
            Self::External(_) => todo!("Load the float from the external source!"),
        }
    }
}

impl TryFrom<ParameterValueV1> for ConstantValue<f64> {
    type Error = ConversionError;

    fn try_from(v1: ParameterValueV1) -> Result<Self, Self::Error> {
        match v1 {
            ParameterValueV1::Constant(v) => Ok(Self::Literal(v)),
            ParameterValueV1::Reference(_) => Err(ConversionError::ConstantFloatReferencesParameter),
            ParameterValueV1::Table(tbl) => Ok(Self::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(_) => Err(ConversionError::ConstantFloatInlineParameter),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct NodeReference {
    /// The name of the node
    pub name: String,
    /// The attribute of the node. If this is `None` then the default attribute is used.
    pub attribute: Option<NodeAttribute>,
}

impl NodeReference {
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
    ) -> Result<Metric, SchemaError> {
        // This is the associated node in the schema
        let node = schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        node.create_metric(network, self.attribute)
    }
}

/// A floating-point(f64) value from a metric in the network.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MetricFloatReference {
    Node(NodeReference),
    Parameter { name: String, key: Option<String> },
    InterNetworkTransfer { name: String },
}

impl MetricFloatReference {
    /// Load the metric definition into a `Metric` containing the appropriate internal references.
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<Metric, SchemaError> {
        match self {
            Self::Node(node_ref) => node_ref.load(network, schema),
            Self::Parameter { name, key } => {
                match key {
                    Some(key) => {
                        // Key given; this should be a multi-valued parameter
                        Ok(Metric::MultiParameterValue((
                            network.get_multi_valued_parameter_index_by_name(name)?,
                            key.clone(),
                        )))
                    }
                    None => {
                        // This should be an existing parameter
                        Ok(Metric::ParameterValue(network.get_parameter_index_by_name(name)?))
                    }
                }
            }
            Self::InterNetworkTransfer { name } => {
                // Find the matching inter model transfer
                match inter_network_transfers.iter().position(|t| &t.name == name) {
                    Some(idx) => Ok(Metric::InterNetworkTransfer(MultiNetworkTransferIndex(idx))),
                    None => Err(SchemaError::InterNetworkTransferNotFound(name.to_string())),
                }
            }
        }
    }
}

/// A floating-point(f64) value from a metric in the network.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum MetricFloatValue {
    Reference(MetricFloatReference),
    InlineParameter { definition: Box<Parameter> },
}

impl MetricFloatValue {
    /// Load the metric definition into a `Metric` containing the appropriate internal references.
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<Metric, SchemaError> {
        match self {
            Self::Reference(reference) => Ok(reference.load(network, schema, inter_network_transfers)?),
            Self::InlineParameter { definition } => {
                // This inline parameter could already have been loaded on a previous attempt
                // Let's see if exists first.
                // TODO this will create strange issues if there are duplicate names in the
                // parameter definitions. I.e. we will only ever load the first one and then
                // assume it is the correct one for future references to that name. This could be
                // improved by checking the parameter returned by name matches the definition here.

                match network.get_parameter_index_by_name(definition.name()) {
                    Ok(p) => {
                        // Found a parameter with the name; assume it is the right one!
                        Ok(Metric::ParameterValue(p))
                    }
                    Err(_) => {
                        // An error retrieving a parameter with this name; assume it needs creating.
                        match definition.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)? {
                            ParameterType::Parameter(idx) => Ok(Metric::ParameterValue(idx)),
                            ParameterType::Index(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found index parameter of type '{}' with name '{}' where an float parameter was expected.",
                        definition.ty(),
                        definition.name(),
                    ))),
                            ParameterType::Multi(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found an inline definition of a multi valued parameter of type '{}' with name '{}' where an float parameter was expected. Multi valued parameters cannot be defined inline.",
                        definition.ty(),
                        definition.name(),
                    ))),
                        }
                    }
                }
            }
        }
    }
}

/// An integer (i64) value from another parameter
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ParameterIndexValue {
    Reference(String),
    Inline(Box<Parameter>),
}

impl ParameterIndexValue {
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<IndexParameterIndex, SchemaError> {
        match self {
            Self::Reference(name) => {
                // This should be an existing parameter
                Ok(network.get_index_parameter_index_by_name(name)?)
            }
            Self::Inline(parameter) => {
                // Inline parameter needs to be added
                match parameter.add_to_model(network, schema, domain, tables, data_path, inter_network_transfers)? {
                    ParameterType::Index(idx) => Ok(idx),
                    ParameterType::Parameter(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found float parameter of type '{}' with name '{}' where an index parameter was expected.",
                        parameter.ty(),
                        parameter.name(),
                    ))),
                            ParameterType::Multi(_) => Err(SchemaError::UnexpectedParameterType(format!(
                        "Found an inline definition of a multi valued parameter of type '{}' with name '{}' where an index parameter was expected. Multi valued parameters cannot be defined inline.",
                        parameter.ty(),
                        parameter.name(),
                    ))),
                }
            }
        }
    }
}

/// A potentially dynamic floating-point (f64) value
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum DynamicFloatValue {
    Constant(ConstantValue<f64>),
    Dynamic(MetricFloatValue),
}

impl Default for DynamicFloatValue {
    fn default() -> Self {
        Self::Constant(ConstantValue::default())
    }
}

impl DynamicFloatValue {
    pub fn from_f64(v: f64) -> Self {
        Self::Constant(ConstantValue::Literal(v))
    }

    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<Metric, SchemaError> {
        let parameter_ref = match self {
            DynamicFloatValue::Constant(v) => Metric::Constant(v.load(tables)?),
            DynamicFloatValue::Dynamic(v) => {
                v.load(network, schema, domain, tables, data_path, inter_network_transfers)?
            }
        };
        Ok(parameter_ref)
    }
}

impl TryFromV1Parameter<ParameterValueV1> for DynamicFloatValue {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterValueV1::Constant(v) => Self::Constant(ConstantValue::Literal(v)),
            ParameterValueV1::Reference(p_name) => {
                Self::Dynamic(MetricFloatValue::Reference(MetricFloatReference::Parameter {
                    name: p_name,
                    key: None,
                }))
            }
            ParameterValueV1::Table(tbl) => Self::Constant(ConstantValue::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(param) => Self::Dynamic(MetricFloatValue::InlineParameter {
                definition: Box::new((*param).try_into_v2_parameter(parent_node, unnamed_count)?),
            }),
        };
        Ok(p)
    }
}

/// A potentially dynamic integer (usize) value
///
/// This value can be a constant (literal or otherwise) or a dynamic value provided
/// by another parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum DynamicIndexValue {
    Constant(ConstantValue<usize>),
    Dynamic(ParameterIndexValue),
}

impl DynamicIndexValue {
    pub fn from_usize(v: usize) -> Self {
        Self::Constant(ConstantValue::Literal(v))
    }

    ///
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<IndexValue, SchemaError> {
        let parameter_ref = match self {
            DynamicIndexValue::Constant(v) => IndexValue::Constant(v.load(tables)?),
            DynamicIndexValue::Dynamic(v) => {
                IndexValue::Dynamic(v.load(network, schema, domain, tables, data_path, inter_network_transfers)?)
            }
        };
        Ok(parameter_ref)
    }
}

impl TryFromV1Parameter<ParameterValueV1> for DynamicIndexValue {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            // There was no such thing as s constant index in Pywr v1
            // TODO this could print a warning and do a cast to usize instead.
            ParameterValueV1::Constant(_) => return Err(ConversionError::FloatToIndex),
            ParameterValueV1::Reference(p_name) => Self::Dynamic(ParameterIndexValue::Reference(p_name)),
            ParameterValueV1::Table(tbl) => Self::Constant(ConstantValue::Table(tbl.try_into()?)),
            ParameterValueV1::Inline(param) => Self::Dynamic(ParameterIndexValue::Inline(Box::new(
                (*param).try_into_v2_parameter(parent_node, unnamed_count)?,
            ))),
        };
        Ok(p)
    }
}

/// An non-variable vector of constant floating-point (f64) values
///
/// This value can be a literal vector of floats or an external reference to an input table.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum ConstantFloatVec {
    Literal(Vec<f64>),
    Table(TableDataRef),
    External(ExternalDataRef),
}

impl ConstantFloatVec {
    /// Return the value loading from a table if required.
    pub fn load(&self, tables: &LoadedTableCollection) -> Result<Vec<f64>, SchemaError> {
        match self {
            Self::Literal(v) => Ok(v.clone()),
            Self::Table(tbl_ref) => Ok(tables.get_vec_f64(tbl_ref)?.clone()),
            Self::External(_) => todo!("Load the float vector from the external source!"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ExternalDataRef {
    url: PathBuf,
    column: Option<TableIndex>,
    index: Option<TableIndex>,
}

impl TryFrom<ExternalDataRefV1> for ExternalDataRef {
    type Error = ConversionError;
    fn try_from(v1: ExternalDataRefV1) -> Result<Self, Self::Error> {
        let column = match v1.column {
            None => None,
            Some(c) => Some(c.try_into()?),
        };
        let index = match v1.index {
            None => None,
            Some(i) => Some(i.try_into()?),
        };
        Ok(Self {
            url: v1.url,
            column,
            index,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum TableIndex {
    Single(String),
    Multi(Vec<String>),
}

impl TryFrom<TableIndexV1> for TableIndex {
    type Error = ConversionError;

    fn try_from(v1: TableIndexV1) -> Result<Self, Self::Error> {
        match v1 {
            TableIndexV1::Single(s) => match s {
                TableIndexEntryV1::Name(s) => Ok(TableIndex::Single(s)),
                TableIndexEntryV1::Index(_) => Err(ConversionError::IntegerTableIndicesNotSupported),
            },
            TableIndexV1::Multi(s) => {
                let names = s
                    .into_iter()
                    .map(|e| match e {
                        TableIndexEntryV1::Name(s) => Ok(s),
                        TableIndexEntryV1::Index(_) => Err(ConversionError::IntegerTableIndicesNotSupported),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Multi(names))
            }
        }
    }
}

pub enum DynamicFloatValueType<'a> {
    Single(&'a DynamicFloatValue),
    List(&'a Vec<DynamicFloatValue>),
}

impl<'a> From<&'a DynamicFloatValue> for DynamicFloatValueType<'a> {
    fn from(v: &'a DynamicFloatValue) -> Self {
        Self::Single(v)
    }
}

impl<'a> From<&'a Vec<DynamicFloatValue>> for DynamicFloatValueType<'a> {
    fn from(v: &'a Vec<DynamicFloatValue>) -> Self {
        Self::List(v)
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::Parameter;
    use std::fs;
    use std::path::PathBuf;

    /// Test all of the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/parameters/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(p).unwrap();
                let _: Parameter = serde_json::from_str(&data).unwrap();
            }
        }
    }
}
