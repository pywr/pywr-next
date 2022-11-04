#[derive(serde::Deserialize)]
pub struct Edge {
    pub from_node: String,
    pub to_node: String,
    pub from_slot: Option<String>,
    pub to_slot: Option<String>,
}
