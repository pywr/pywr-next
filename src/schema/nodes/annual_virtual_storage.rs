use crate::node::StorageInitialVolume;
use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::{ConstantValue, DynamicFloatValue, TryIntoV2Parameter};
use crate::PywrError;
use pywr_schema::nodes::AnnualVirtualStorageNode as AnnualVirtualStorageNodeV1;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct AnnualReset {
    pub day: u8,
    pub month: time::Month,
    pub use_initial_volume: bool,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct AnnualVirtualStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<ConstantValue<f64>>,
    pub min_volume: Option<ConstantValue<f64>>,
    pub cost: Option<DynamicFloatValue>,
    pub initial_volume: Option<f64>,
    pub initial_volume_pc: Option<f64>,
    pub reset: AnnualReset,
}

impl AnnualVirtualStorageNode {
    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        // TODO this initial volume should be used??
        let initial_volume = if let Some(iv) = self.initial_volume {
            StorageInitialVolume::Absolute(iv)
        } else if let Some(pc) = self.initial_volume_pc {
            StorageInitialVolume::Proportional(pc)
        } else {
            return Err(PywrError::MissingInitialVolume(self.meta.name.to_string()));
        };

        let node_idxs = self
            .nodes
            .iter()
            .map(|name| model.get_node_index_by_name(name.as_str(), None))
            .collect::<Result<_, _>>()?;

        // TODO this should be an annual virtual storage!
        model.add_virtual_storage_node(self.meta.name.as_str(), None, node_idxs, self.factors.clone())?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![]
    }
}

impl TryFrom<AnnualVirtualStorageNodeV1> for AnnualVirtualStorageNode {
    type Error = PywrError;

    fn try_from(v1: AnnualVirtualStorageNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            nodes: v1.nodes,
            factors: v1.factors,
            max_volume: v1.max_volume.map(|v| v.try_into()).transpose()?,
            min_volume: v1.min_volume.map(|v| v.try_into()).transpose()?,
            cost,
            initial_volume: v1.initial_volume,
            initial_volume_pc: v1.initial_volume_pc,
            reset: AnnualReset {
                day: v1.reset_day,
                month: v1.reset_month,
                use_initial_volume: v1.reset_to_initial_volume,
            },
        };
        Ok(n)
    }
}
