use crate::error::{ConversionError, SchemaError};
use crate::metric::Metric;
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::TryIntoV2Parameter;
use pywr_core::aggregated_node::Factors;
use pywr_core::metric::MetricF64;
use pywr_core::node::NodeIndex;
use pywr_schema_macros::PywrNode;
use pywr_v1_schema::nodes::RiverSplitWithGaugeNode as RiverSplitWithGaugeNodeV1;
use std::collections::HashMap;

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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PywrNode)]
pub struct RiverSplitWithGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<Metric>,
    pub mrf_cost: Option<Metric>,
    pub splits: Vec<(Metric, String)>,
}

impl RiverSplitWithGaugeNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

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

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // TODO do this properly
        network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        let bypass_idx = network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        for (i, _) in self.splits.iter().enumerate() {
            // Each split has a link node and an aggregated node to enforce the factors
            let split_idx = network.add_link_node(self.meta.name.as_str(), Self::split_sub_name(i).as_deref())?;

            // The factors will be set during the `set_constraints` method
            network.add_aggregated_node(
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
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        for (i, (factor, _)) in self.splits.iter().enumerate() {
            // Set the factors for each split
            let factors = Factors::Proportion(vec![factor.load(network, args)?]);
            network.set_aggregated_node_factors(
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

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        // This gets the indices of all the link nodes
        // There's currently no way to isolate the flows to the individual splits
        // Therefore, the only metrics are gross inflow and outflow
        let mut indices = vec![
            network.get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())?,
            network.get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())?,
        ];

        let split_idx: Vec<NodeIndex> = self
            .splits
            .iter()
            .enumerate()
            .map(|(i, _)| network.get_node_index_by_name(self.meta.name.as_str(), Self::split_sub_name(i).as_deref()))
            .collect::<Result<_, _>>()?;

        indices.extend(split_idx);

        let metric = match attr {
            NodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            NodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "RiverSplitWithGaugeNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<RiverSplitWithGaugeNodeV1> for RiverSplitWithGaugeNode {
    type Error = ConversionError;

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
            .collect::<Result<Vec<(Metric, String)>, Self::Error>>()?;

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
    use crate::model::PywrModel;
    use pywr_core::test_utils::run_all_solvers;

    fn model_str() -> &'static str {
        include_str!("../test_models/river_split_with_gauge1.json")
    }

    #[test]
    fn test_model_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 4);
        assert_eq!(schema.network.edges.len(), 3);
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let model = schema.build_model(None, None).unwrap();

        let network = model.network();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 6);

        // Test all solvers
        run_all_solvers(&model);

        // TODO assert the results!
    }
}
