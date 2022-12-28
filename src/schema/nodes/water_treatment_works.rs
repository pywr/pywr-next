use crate::parameters::FloatValue;
use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::DynamicFloatValue;
use crate::PywrError;
use std::path::Path;

#[doc = svgbobdoc::transform!(
/// This is used to represent a water treatment works (WTW).
///
/// The node includes
///
///
/// ```svgbob
///                          <node>.net_soft_min_flow
///                           .--->L ----.
///            <node>.net    |           |     D
///          .------>L ------|           |--->*- - -
///      U  |                |           |
///     -*--|                '--->L ----'
///         |                <node>.net_above_soft_min_flow
///          '------>O
///            <node>.loss
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct WaterTreatmentWorks {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub loss_factor: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub max_flow: Option<DynamicFloatValue>,
    pub soft_min_flow: Option<DynamicFloatValue>,
    pub soft_min_flow_cost: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl WaterTreatmentWorks {
    fn loss_sub_name() -> Option<&'static str> {
        Some("loss")
    }

    fn net_sub_name() -> Option<&'static str> {
        Some("net")
    }
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg")
    }

    fn net_soft_min_flow_sub_name() -> Option<&'static str> {
        Some("net_soft_min_flow")
    }

    fn net_above_soft_min_flow_sub_name() -> Option<&'static str> {
        Some("net_above_soft_min_flow")
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        let idx_loss = model.add_link_node(self.meta.name.as_str(), Self::loss_sub_name())?;
        let idx_net = model.add_link_node(self.meta.name.as_str(), Self::net_sub_name())?;
        let idx_soft_min_flow = model.add_link_node(self.meta.name.as_str(), Self::net_soft_min_flow_sub_name())?;
        let idx_above_soft_min_flow =
            model.add_link_node(self.meta.name.as_str(), Self::net_above_soft_min_flow_sub_name())?;

        // Create the internal connections
        model.connect_nodes(idx_net, idx_soft_min_flow)?;
        model.connect_nodes(idx_net, idx_above_soft_min_flow)?;

        if self.loss_factor.is_some() {
            // This aggregated node will contain the factors to enforce the loss
            model.add_aggregated_node(
                self.meta.name.as_str(),
                Self::agg_sub_name(),
                &[idx_net, idx_loss],
                None,
            )?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), PywrError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(model, tables, data_path)?;
            model.set_node_cost(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(model, tables, data_path)?;
            model.set_node_max_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(model, tables, data_path)?;
            model.set_node_min_flow(self.meta.name.as_str(), Self::net_sub_name(), value.into())?;
        }

        // soft min flow constraints; This typically applies a negative cost upto a maximum
        // defined by the `soft_min_flow`
        if let Some(cost) = &self.soft_min_flow_cost {
            let value = cost.load(model, tables, data_path)?;
            model.set_node_cost(
                self.meta.name.as_str(),
                Self::net_soft_min_flow_sub_name(),
                value.into(),
            )?;
        }
        if let Some(min_flow) = &self.soft_min_flow {
            let value = min_flow.load(model, tables, data_path)?;
            model.set_node_max_flow(
                self.meta.name.as_str(),
                Self::net_soft_min_flow_sub_name(),
                value.into(),
            )?;
        }

        if let Some(loss_factor) = &self.loss_factor {
            // Set the factors for the loss
            let factors = [FloatValue::Constant(1.0), loss_factor.load(model, tables, data_path)?];
            model.set_aggregated_node_factors(self.meta.name.as_str(), Self::agg_sub_name(), Some(&factors))?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        // Connect directly to the total net and loss sub-nodes.
        vec![
            (self.meta.name.as_str(), Self::loss_sub_name()),
            (self.meta.name.as_str(), Self::net_sub_name()),
        ]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        // Connect to the split of the net flow.
        vec![
            (self.meta.name.as_str(), Self::net_soft_min_flow_sub_name()),
            (self.meta.name.as_str(), Self::net_above_soft_min_flow_sub_name()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::nodes::WaterTreatmentWorks;

    #[test]
    fn test_wtw() {
        let data = r#"
                {
                  "type": "WaterTreatmentWorks",
                  "name": "My WTW",
                  "comment": null,
                  "position": null,
                  "loss_factor": {
                    "index": "My WTW",
                    "table": "loss_factors"
                  },
                  "soft_min_flow": 105,
                  "cost": 2.29,
                  "max_flow": {
                    "name": "My WTW max flow",
                    "control_curves": [
                      "A control curve"
                    ],
                    "values": [
                      "a_max_flow",
                      {
                        "name": "zero",
                        "type": "Constant",
                        "value": 0.0
                      }
                    ],
                    "storage_node": "My reservoir",
                    "type": "ControlCurve"
                  },
                  "soft_min_flow_cost": "my_min_flow_cost"
                }
            "#;

        let node: WaterTreatmentWorks = serde_json::from_str(data).unwrap();

        assert_eq!(node.meta.name, "My WTW");
    }
}
