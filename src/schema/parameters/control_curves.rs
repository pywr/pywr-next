use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Debug)]
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
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ControlCurveIndexParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub values: Vec<DynamicFloatValue>,
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
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
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
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ControlCurvePiecewiseInterpolatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub control_curves: Vec<DynamicFloatValue>,
    pub storage_node: String,
    pub values: Option<Vec<[f64; 2]>>,
    pub minimum: f64,
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
}

#[cfg(test)]
mod tests {
    use crate::schema::parameters::control_curves::ControlCurvePiecewiseInterpolatedParameter;
    use crate::schema::parameters::DynamicFloatValueType;

    #[test]
    fn test_control_curve_piecewise_interpolated() {
        let data = r#"
            {
                "name": "My control curve",
                "type": "ControlCurvePiecewiseInterpolated",
                "storage_node": "Reservoir",
                "control_curves": [
                    "reservoir_cc",
                    {"name": "a-constant", "type": "Constant", "value":  0.2}
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
