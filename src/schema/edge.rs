#[derive(serde::Deserialize, serde::Serialize)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_slot: Option<String>,
}

impl From<pywr_schema::edge::Edge> for Edge {
    fn from(v1: pywr_schema::edge::Edge) -> Self {
        Self {
            from_node: v1.from_node,
            to_node: v1.to_node,
            from_slot: v1.from_slot.flatten(),
            to_slot: v1.to_slot.flatten(),
        }
    }
}
