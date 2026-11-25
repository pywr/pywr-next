use crate::metric::Metric;
use crate::nodes::NodeMeta;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct BoreholeNode {
    pub meta: NodeMeta,
    #[serde(rename = "yield")]
    pub hyd_yield: Option<Metric>,
    pub capacity: Option<Metric>,
    pub ts_licence: Option<Metric>,

    // TODO needs to be a slimmed down version of AnnualVirtualStorageNode
    // that does not include factors or nodes.
    pub annual_licence: Option<AnnualVirtualStorageNode>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}
