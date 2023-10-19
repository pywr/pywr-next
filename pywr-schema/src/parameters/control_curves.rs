use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::parameters::{
    DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
use pywr_core::models::ModelDomain;
use pywr_core::parameters::{IndexParameterIndex, ParameterIndex};
use pywr_v1_schema::parameters::{
    ControlCurveIndexParameter as ControlCurveIndexParameterV1,
    ControlCurveInterpolatedParameter as ControlCurveInterpolatedParameterV1,
    ControlCurveParameter as ControlCurveParameterV1,
    ControlCurvePiecewiseInterpolatedParameter as ControlCurvePiecewiseInterpolatedParameterV1,
};
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ControlCurveInterpolatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub storage_node: String,
    pub values: Vec<DynamicFloatValue>,
}

impl ControlCurveInterpolatedParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        vec![("storage_node", self.storage_node.as_str())].into_iter().collect()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let cc = &self.control_curves;
        attributes.insert("control_curves", cc.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, SchemaError> {
        let metric = network.get_storage_node_metric(&self.storage_node, None, true)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let values = self
            .values
            .iter()
            .map(|val| val.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::InterpolatedParameter::new(&self.meta.name, metric, control_curves, values);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<ControlCurveInterpolatedParameterV1> for ControlCurveInterpolatedParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ControlCurveInterpolatedParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let control_curves = if let Some(control_curves) = v1.control_curves {
            control_curves
                .into_iter()
                .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(control_curve) = v1.control_curve {
            vec![control_curve.try_into_v2_parameter(Some(&meta.name), unnamed_count)?]
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["control_curves".to_string(), "control_curve".to_string()],
            });
        };

        let values = v1.values.into_iter().map(DynamicFloatValue::from_f64).collect();

        let p = Self {
            meta,
            control_curves,
            storage_node: v1.storage_node,
            values,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ControlCurveIndexParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub storage_node: String,
}

impl ControlCurveIndexParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        vec![("storage_node", self.storage_node.as_str())].into_iter().collect()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let cc = &self.control_curves;
        attributes.insert("control_curves", cc.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<IndexParameterIndex, SchemaError> {
        let metric = network.get_storage_node_metric(&self.storage_node, None, true)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::ControlCurveIndexParameter::new(&self.meta.name, metric, control_curves);
        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<ControlCurveIndexParameterV1> for ControlCurveIndexParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ControlCurveIndexParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let control_curves = v1
            .control_curves
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            control_curves,
            storage_node: v1.storage_node,
        };
        Ok(p)
    }
}

/// Pywr v1.x ControlCurveParameter can be an index parameter if it is not given "values"
/// or "parameters" keys.
impl TryFromV1Parameter<ControlCurveParameterV1> for ControlCurveIndexParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ControlCurveParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let control_curves = if let Some(control_curves) = v1.control_curves {
            control_curves
                .into_iter()
                .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(control_curve) = v1.control_curve {
            vec![control_curve.try_into_v2_parameter(Some(&meta.name), unnamed_count)?]
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["control_curves".to_string(), "control_curve".to_string()],
            });
        };

        if v1.values.is_some() || v1.parameters.is_some() {
            return Err(ConversionError::UnexpectedAttribute {
                name: meta.name,
                attrs: vec!["values".to_string(), "parameters".to_string()],
            });
        };

        let p = Self {
            meta,
            control_curves,
            storage_node: v1.storage_node,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ControlCurveParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub storage_node: String,
    pub values: Vec<DynamicFloatValue>,
}

impl ControlCurveParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        vec![("storage_node", self.storage_node.as_str())].into_iter().collect()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let cc = &self.control_curves;
        attributes.insert("control_curves", cc.into());
        let values = &self.values;
        attributes.insert("values", values.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, SchemaError> {
        let metric = network.get_storage_node_metric(&self.storage_node, None, true)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let values = self
            .values
            .iter()
            .map(|val| val.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::ControlCurveParameter::new(&self.meta.name, metric, control_curves, values);
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<ControlCurveParameterV1> for ControlCurveParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ControlCurveParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let control_curves = if let Some(control_curves) = v1.control_curves {
            control_curves
                .into_iter()
                .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(control_curve) = v1.control_curve {
            vec![control_curve.try_into_v2_parameter(Some(&meta.name), unnamed_count)?]
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["control_curves".to_string(), "control_curve".to_string()],
            });
        };

        let values = if let Some(values) = v1.values {
            values.into_iter().map(DynamicFloatValue::from_f64).collect()
        } else if let Some(parameters) = v1.parameters {
            parameters
                .into_iter()
                .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["values".to_string(), "parameters".to_string()],
            });
        };

        let p = Self {
            meta,
            control_curves,
            storage_node: v1.storage_node,
            values,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ControlCurvePiecewiseInterpolatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub storage_node: String,
    pub values: Option<Vec<[f64; 2]>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

impl ControlCurvePiecewiseInterpolatedParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        vec![("storage_node", self.storage_node.as_str())].into_iter().collect()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let cc = &self.control_curves;
        attributes.insert("control_curves", cc.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, SchemaError> {
        let metric = network.get_storage_node_metric(&self.storage_node, None, true)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, domain, tables, data_path))
            .collect::<Result<_, _>>()?;

        let values = match &self.values {
            None => Vec::new(),
            Some(values) => values.clone(),
        };

        let p = pywr_core::parameters::PiecewiseInterpolatedParameter::new(
            &self.meta.name,
            metric,
            control_curves,
            values,
            self.maximum.unwrap_or(1.0),
            self.minimum.unwrap_or(0.0),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<ControlCurvePiecewiseInterpolatedParameterV1> for ControlCurvePiecewiseInterpolatedParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ControlCurvePiecewiseInterpolatedParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        let control_curves = if let Some(control_curves) = v1.control_curves {
            control_curves
                .into_iter()
                .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
                .collect::<Result<Vec<_>, _>>()?
        } else if let Some(control_curve) = v1.control_curve {
            vec![control_curve.try_into_v2_parameter(Some(&meta.name), unnamed_count)?]
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["control_curves".to_string(), "control_curve".to_string()],
            });
        };

        let p = Self {
            meta,
            control_curves,
            storage_node: v1.storage_node,
            values: v1.values,
            minimum: Some(v1.minimum),
            maximum: None,
        };
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use crate::parameters::control_curves::ControlCurvePiecewiseInterpolatedParameter;
    use crate::parameters::DynamicFloatValueType;

    #[test]
    fn test_control_curve_piecewise_interpolated() {
        let data = r#"
            {
                "name": "My control curve",
                "type": "ControlCurvePiecewiseInterpolated",
                "storage_node": "Reservoir",
                "control_curves": [
                    {"type": "Parameter", "name": "reservoir_cc"},
                    0.2
                ],
                "comment": "A witty comment",
                "values": [
                    [-0.1, -1.0],
                    [-100, -200],
                    [-300, -400]
                ],
                "minimum": 0.05
            }
            "#;

        let param: ControlCurvePiecewiseInterpolatedParameter = serde_json::from_str(data).unwrap();

        assert_eq!(param.node_references().len(), 1);
        assert_eq!(param.node_references().remove("storage_node"), Some("Reservoir"));

        assert_eq!(param.parameters().len(), 1);
        match param.parameters().remove("control_curves").unwrap() {
            DynamicFloatValueType::List(p) => assert_eq!(p.len(), 2),
            _ => panic!("Wrong variant for control_curves."),
        };
    }
}
