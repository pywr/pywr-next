use crate::aggregated_node::Factors;
use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use crate::PywrError;
use pywr_schema::nodes::RiverSplitWithGaugeNode as RiverSplitWithGaugeNodeV1;
use std::path::Path;

#[doc = svgbobdoc::transform!(
/// This is used to represent a proportional split above a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///           <node>.mrf
///          .------>L -----.
///      U  | <node>.bypass  |     D[<default>]
///     -*--|------->L ------|--->*- - -
///         | <node>.split-0 |
///          '------>L -----'
///                  |             D[slot_name_0]
///                   '---------->*- - -
///
///         |                |
///         | <node>.split-i |
///          '------>L -----'
///                  |             D[slot_name_i]
///                   '---------->*- - -
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct RiverSplitWithGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<DynamicFloatValue>,
    pub mrf_cost: Option<DynamicFloatValue>,
    pub splits: Vec<(DynamicFloatValue, String)>,
}

impl RiverSplitWithGaugeNode {
    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    fn split_sub_name(i: usize) -> Option<String> {
        Some(format!("split-{i}"))
    }
    fn split_agg_sub_name(i: usize) -> Option<String> {
        Some(format!("split-agg-{i}"))
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        // TODO do this properly
        model.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        let bypass_idx = model.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        for (i, _) in self.splits.iter().enumerate() {
            // Each split has a link node and an aggregated node to enforce the factors
            let split_idx = model.add_link_node(self.meta.name.as_str(), Self::split_sub_name(i).as_deref())?;

            // The factors will be set during the `set_constraints` method
            model.add_aggregated_node(
                self.meta.name.as_str(),
                Self::split_agg_sub_name(i).as_deref(),
                &[bypass_idx, split_idx],
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
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(model, tables, data_path)?;
            model.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(model, tables, data_path)?;
            model.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        for (i, (factor, _)) in self.splits.iter().enumerate() {
            // Set the factors for each split
            let factors = Factors::Proportion(vec![factor.load(model, tables, data_path)?]);
            model.set_aggregated_node_factors(
                self.meta.name.as_str(),
                Self::split_agg_sub_name(i).as_deref(),
                Some(factors),
            )?;
        }

        Ok(())
    }

    /// These connectors are used for both incoming and outgoing edges on the default slot.
    fn default_connectors(&self) -> Vec<(&str, Option<String>)> {
        let mut connectors = vec![
            (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
            (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
        ];

        connectors.extend(
            self.splits
                .iter()
                .enumerate()
                .map(|(i, _)| (self.meta.name.as_str(), Self::split_sub_name(i))),
        );

        connectors
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        self.default_connectors()
    }

    pub fn output_connectors(&self, slot: Option<&str>) -> Vec<(&str, Option<String>)> {
        match slot {
            Some(slot) => {
                let i = self
                    .splits
                    .iter()
                    .position(|(_, s)| s == slot)
                    .expect("Invalid slot name!");

                vec![(self.meta.name.as_str(), Self::split_sub_name(i))]
            }
            None => self.default_connectors(),
        }
    }
}

impl TryFrom<RiverSplitWithGaugeNodeV1> for RiverSplitWithGaugeNode {
    type Error = PywrError;

    fn try_from(v1: RiverSplitWithGaugeNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let mrf = v1
            .mrf
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let mrf_cost = v1
            .mrf_cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let splits = v1
            .factors
            .into_iter()
            .skip(1)
            .zip(v1.slot_names.into_iter().skip(1))
            .map(|(f, slot_name)| {
                Ok((
                    f.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count)?,
                    slot_name,
                ))
            })
            .collect::<Result<Vec<(DynamicFloatValue, String)>, Self::Error>>()?;

        let n = Self {
            meta,
            mrf,
            mrf_cost,
            splits,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::RunOptions;
    use crate::schema::model::PywrModel;
    use crate::solvers::ClpSolver;
    use crate::timestep::Timestepper;

    fn model_str() -> &'static str {
        r#"
            {
                "metadata": {
                    "title": "Simple 1",
                    "description": "A very simple example.",
                    "minimum_version": "0.1"
                },
                "timestepper": {
                    "start": "2015-01-01",
                    "end": "2015-12-31",
                    "timestep": 1
                },
                "nodes": [
                    {
                        "name": "catchment1",
                        "type": "Catchment",
                        "flow": 15
                    },
                    {
                        "name": "gauge1",
                        "type": "RiverGauge",
                        "mrf": 5.0,
                        "mrf_cost": -20.0
                    },
                    {
                        "name": "term1",
                        "type": "Output"
                    },
                    {
                        "name": "demand1",
                        "type": "Output",
                        "max_flow": 15.0,
                        "cost": -10
                    }
                ],
                "edges": [
                    {
                        "from_node": "catchment1",
                        "to_node": "gauge1"
                    },
                    {
                        "from_node": "gauge1",
                        "to_node": "term1"
                    },
                    {
                        "from_node": "gauge1",
                        "to_node": "demand1"
                    }
                ]
            }
            "#
    }

    #[test]
    fn test_model_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.nodes.len(), 4);
        assert_eq!(schema.edges.len(), 3);
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let (model, timestepper): (crate::model::Model, Timestepper) = schema.try_into_model(None).unwrap();

        assert_eq!(model.nodes.len(), 5);
        assert_eq!(model.edges.len(), 6);

        model.run::<ClpSolver>(&timestepper, &RunOptions::default()).unwrap()

        // TODO assert the results!
    }
}
