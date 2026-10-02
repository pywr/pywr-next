mod aggregated;
mod virtual_storage;

use crate::metric::{Metric, NodeComponentReference};
use crate::nodes::{Node, NodeAttribute, NodeComponent, NodeMeta, NodePosition, PlaceholderNode};
use crate::parameters::{Parameter, validate_each_parameter};
use crate::validation::{MemberProblem, NodeReferenceProblem, VirtualNodeProblem};
use crate::visit::{Reference, ReferenceMut, VisitReferences};
#[cfg(feature = "core")]
use crate::{LoadArgs, SchemaError};
use crate::{VisitMetrics, VisitPaths};
pub use aggregated::{
    AggregatedNode, AggregatedNodeAttribute, AggregatedStorageNode, AggregatedStorageNodeAttribute, Relationship,
    RelationshipType,
};
#[cfg(feature = "core")]
use pywr_core::metric::UnresolvedMetricF64;
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
pub use virtual_storage::{
    AnnualReset, RollingWindow, RollingWindowType, SeasonalReset, VirtualStorageNode, VirtualStorageNodeAttribute,
    VirtualStorageReset, VirtualStorageResetType, VirtualStorageResetVolume, VirtualStorageResetVolumeType,
};

/// Create a blank [`VirtualNode`] of the given type.
///
/// Every field of the returned node is at its default value, including the node's name.
impl From<VirtualNodeType> for VirtualNode {
    fn from(node_type: VirtualNodeType) -> Self {
        match node_type {
            VirtualNodeType::Aggregated => VirtualNode::Aggregated(AggregatedNode::default()),
            VirtualNodeType::AggregatedStorage => VirtualNode::AggregatedStorage(AggregatedStorageNode::default()),
            VirtualNodeType::VirtualStorage => VirtualNode::VirtualStorage(VirtualStorageNode::default()),
            VirtualNodeType::Placeholder => VirtualNode::Placeholder(PlaceholderNode::default()),
        }
    }
}

/// The main enum for all nodes in the model.
#[derive(serde::Deserialize, serde::Serialize, Clone, EnumDiscriminants, Debug, JsonSchema, Display)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
// This creates a separate enum called `NodeType` that is available in this module.
#[strum_discriminants(name(VirtualNodeType))]
// This is currently required by the `Reservoir` node. Rather than box it
#[allow(clippy::large_enum_variant)]
pub enum VirtualNode {
    Aggregated(AggregatedNode),
    AggregatedStorage(AggregatedStorageNode),
    VirtualStorage(VirtualStorageNode),
    Placeholder(PlaceholderNode),
}

impl VirtualNode {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn position(&self) -> Option<&NodePosition> {
        self.meta().position.as_ref()
    }

    pub fn node_type(&self) -> VirtualNodeType {
        // Implementation provided by the `EnumDiscriminants` derive macro.
        self.into()
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder(_))
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            VirtualNode::Aggregated(n) => &n.meta,
            VirtualNode::AggregatedStorage(n) => &n.meta,
            VirtualNode::VirtualStorage(n) => &n.meta,
            VirtualNode::Placeholder(n) => &n.meta,
        }
    }

    /// Get a mutable reference to the node's metadata.
    pub fn meta_mut(&mut self) -> &mut NodeMeta {
        match self {
            VirtualNode::Aggregated(n) => &mut n.meta,
            VirtualNode::AggregatedStorage(n) => &mut n.meta,
            VirtualNode::VirtualStorage(n) => &mut n.meta,
            VirtualNode::Placeholder(n) => &mut n.meta,
        }
    }

    pub fn default_attribute(&self) -> NodeAttribute {
        match self {
            VirtualNode::Aggregated(n) => n.default_attribute().into(),
            VirtualNode::AggregatedStorage(n) => n.default_attribute().into(),
            VirtualNode::VirtualStorage(n) => n.default_attribute().into(),
            VirtualNode::Placeholder(n) => n.default_attribute(),
        }
    }

    /// Returns the attributes that this node has.
    pub fn attributes(&self) -> Vec<NodeAttribute> {
        match self {
            VirtualNode::Aggregated(_) => AggregatedNodeAttribute::iter().map(Into::into).collect(),
            VirtualNode::AggregatedStorage(_) => AggregatedStorageNodeAttribute::iter().map(Into::into).collect(),
            VirtualNode::VirtualStorage(_) => VirtualStorageNodeAttribute::iter().map(Into::into).collect(),
            VirtualNode::Placeholder(_) => Vec::new(),
        }
    }

    /// Check that a metric can read this virtual node's `attribute`, `None` being its default. A
    /// placeholder takes any reference.
    pub fn validate_reference(&self, attribute: Option<NodeAttribute>) -> Result<(), NodeReferenceProblem> {
        if self.is_placeholder() {
            return Ok(());
        }

        let Some(attribute) = attribute else {
            return Ok(());
        };
        let supported = self.attributes();

        if supported.contains(&attribute) {
            Ok(())
        } else {
            Err(NodeReferenceProblem::AttributeNotSupported { attribute, supported })
        }
    }

    /// Returns the default component for the node, if defined.
    pub fn default_component(&self) -> Option<NodeComponent> {
        match self {
            VirtualNode::Aggregated(_) => None,
            VirtualNode::AggregatedStorage(_) => None,
            VirtualNode::VirtualStorage(_) => None,
            VirtualNode::Placeholder(_) => None,
        }
    }

    /// The nodes this virtual node is made of, each with the component it takes.
    pub fn members(&self) -> &[NodeComponentReference] {
        match self {
            VirtualNode::Aggregated(n) => &n.nodes,
            VirtualNode::AggregatedStorage(n) => &n.storage_nodes,
            VirtualNode::VirtualStorage(n) => &n.nodes,
            VirtualNode::Placeholder(_) => &[],
        }
    }

    /// Check that this virtual node can take `node` as its `member`. The member's name is not
    /// compared with `node`'s, so a node being edited can be checked. A placeholder on either side
    /// passes.
    pub fn validate_member(&self, member: &NodeComponentReference, node: &Node) -> Result<(), MemberProblem> {
        if self.is_placeholder() || node.is_placeholder() {
            return Ok(());
        }

        if matches!(self, VirtualNode::AggregatedStorage(_)) {
            return if node.is_storage() {
                Ok(())
            } else {
                Err(MemberProblem::NotStorage)
            };
        }

        let Some(default) = node.default_component() else {
            return Err(MemberProblem::WithoutComponents);
        };
        let component = member.component.unwrap_or(default);
        let built = node.built_components();

        if built.contains(&component) {
            Ok(())
        } else {
            Err(MemberProblem::ComponentNotBuilt {
                component,
                default: member.component.is_none(),
                built,
            })
        }
    }

    /// Get the locally defined parameters for this node.
    ///
    /// This does **not** return which parameters this node might reference, but rather
    /// the parameters that are defined on this node itself.
    pub fn local_parameters(&self) -> Option<&[Parameter]> {
        match self {
            VirtualNode::Aggregated(n) => n.parameters.as_deref(),
            VirtualNode::AggregatedStorage(n) => n.parameters.as_deref(),
            VirtualNode::VirtualStorage(n) => n.parameters.as_deref(),
            VirtualNode::Placeholder(_) => None,
        }
    }

    pub fn local_parameters_mut(&mut self) -> Option<&mut [Parameter]> {
        match self {
            VirtualNode::Aggregated(n) => n.parameters.as_deref_mut(),
            VirtualNode::AggregatedStorage(n) => n.parameters.as_deref_mut(),
            VirtualNode::VirtualStorage(n) => n.parameters.as_deref_mut(),
            VirtualNode::Placeholder(_) => None,
        }
    }

    /// Get local parameter by name.
    pub fn get_local_parameter(&self, name: &str) -> Option<&Parameter> {
        self.local_parameters()
            .and_then(|params| params.iter().find(|p| p.name() == name))
    }

    /// Check the virtual node's own fields and its local parameters', and return every problem
    /// found, its own first.
    pub fn validate(&self) -> Result<(), Vec<VirtualNodeProblem>> {
        let mut problems = match self {
            VirtualNode::Aggregated(n) => n.validate(),
            VirtualNode::VirtualStorage(n) => n.validate(),
            VirtualNode::AggregatedStorage(_) | VirtualNode::Placeholder(_) => Ok(()),
        }
        .err()
        .unwrap_or_default();

        problems.extend(
            validate_each_parameter(self.local_parameters().unwrap_or_default()).map(|(parameter, problem)| {
                VirtualNodeProblem::InvalidLocalParameter {
                    parameter: parameter.to_string(),
                    problem,
                }
            }),
        );

        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }
}

#[cfg(feature = "core")]
impl VirtualNode {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        match self {
            VirtualNode::Aggregated(n) => n.add_to_network(network, args),
            VirtualNode::AggregatedStorage(n) => n.add_to_network(network, args),
            VirtualNode::VirtualStorage(n) => n.add_to_network(network, args),
            VirtualNode::Placeholder(n) => n.add_to_network(),
        }
    }

    /// Create a metric for the given attribute on this node.
    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        match self {
            VirtualNode::Aggregated(n) => n.create_metric(attribute),
            VirtualNode::AggregatedStorage(n) => n.create_metric(attribute),
            VirtualNode::VirtualStorage(n) => n.create_metric(attribute),
            VirtualNode::Placeholder(n) => n.create_metric(),
        }
    }
}

impl VisitMetrics for VirtualNode {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_metrics(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_metrics(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_metrics(visitor),
            VirtualNode::Placeholder(n) => n.visit_metrics(visitor),
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_metrics_mut(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_metrics_mut(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_metrics_mut(visitor),
            VirtualNode::Placeholder(n) => n.visit_metrics_mut(visitor),
        }
    }
}

impl VisitPaths for VirtualNode {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_paths(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_paths(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_paths(visitor),
            VirtualNode::Placeholder(n) => n.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_paths_mut(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_paths_mut(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_paths_mut(visitor),
            VirtualNode::Placeholder(n) => n.visit_paths_mut(visitor),
        }
    }
}

impl VisitReferences for VirtualNode {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_references(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_references(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_references(visitor),
            VirtualNode::Placeholder(n) => n.visit_references(visitor),
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            VirtualNode::Aggregated(n) => n.visit_references_mut(visitor),
            VirtualNode::AggregatedStorage(n) => n.visit_references_mut(visitor),
            VirtualNode::VirtualStorage(n) => n.visit_references_mut(visitor),
            VirtualNode::Placeholder(n) => n.visit_references_mut(visitor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{VirtualNode, VirtualNodeType};
    use std::fs;
    use std::path::PathBuf;
    use strum::IntoEnumIterator;

    /// [`VirtualNode::validate`] should pass every default virtual node, refuse one breaking each
    /// rule, and pass one where a rule could be too strict.
    #[test]
    fn test_validate_checks_each_rule() {
        use crate::validation::VirtualNodeProblem::*;
        use crate::validation::{InitialVolumeProblem, ParameterProblem};
        use serde_json::json;

        for node_type in VirtualNodeType::iter() {
            let node: VirtualNode = node_type.into();
            assert_eq!(node.validate(), Ok(()), "a default {node_type}");
        }

        let not_a_date = |day_field, month_field, day, month| NotADate {
            day_field,
            month_field,
            day,
            month,
        };
        let x = |value: f64| json!({ "type": "Literal", "value": value });
        let members = |count: usize| {
            json!(
                (0..count)
                    .map(|i| json!({ "name": format!("n{i}") }))
                    .collect::<Vec<_>>()
            )
        };

        let cases = [
            // 29 February comes every four years.
            (
                VirtualNodeType::VirtualStorage,
                json!({ "reset": { "type": "Annual", "day": 29, "month": 2 } }),
                vec![],
            ),
            (
                VirtualNodeType::VirtualStorage,
                json!({ "reset": { "type": "Annual", "day": 30, "month": 2 } }),
                vec![not_a_date("day", "month", 30, 2)],
            ),
            (
                VirtualNodeType::VirtualStorage,
                json!({ "reset": { "type": "Seasonal", "start_day": 0, "start_month": 1, "end_day": 1, "end_month": 13 } }),
                vec![
                    not_a_date("start_day", "start_month", 0, 1),
                    not_a_date("end_day", "end_month", 1, 13),
                ],
            ),
            (
                VirtualNodeType::VirtualStorage,
                json!({ "max_volume": x(100.0), "initial_volume": { "type": "Absolute", "volume": 150.0 } }),
                vec![InitialVolume(InitialVolumeProblem::AboveMax)],
            ),
            (
                VirtualNodeType::Aggregated,
                json!({ "nodes": members(2), "relationship": { "type": "Proportion", "factors": [x(0.5)] } }),
                vec![],
            ),
            (
                VirtualNodeType::Aggregated,
                json!({ "nodes": members(2), "relationship": { "type": "Proportion", "factors": [x(0.5), x(0.5)] } }),
                vec![ProportionFactorCount { factors: 2, members: 2 }],
            ),
            (
                VirtualNodeType::Aggregated,
                json!({ "nodes": members(0), "relationship": { "type": "Ratio", "factors": [] } }),
                vec![RatioFactorCount { factors: 0, members: 0 }],
            ),
            (
                VirtualNodeType::Aggregated,
                json!({ "nodes": members(3), "relationship": { "type": "Coefficients", "factors": [x(1.0), x(1.0), x(1.0)] } }),
                vec![CoefficientsFactorCount { factors: 3, members: 3 }],
            ),
            (
                VirtualNodeType::VirtualStorage,
                json!({
                    "parameters": [{
                        "meta": { "name": "drawdown" },
                        "type": "UniformDrawdownProfile",
                        "reset_day": { "type": "Literal", "value": 30 },
                        "reset_month": { "type": "Literal", "value": 2 }
                    }]
                }),
                vec![InvalidLocalParameter {
                    parameter: "drawdown".to_string(),
                    problem: ParameterProblem::NotADate { day: 30, month: 2 },
                }],
            ),
        ];

        for (node_type, fields, problems) in cases {
            let mut data = serde_json::to_value(VirtualNode::from(node_type)).unwrap();
            for (field, value) in fields.as_object().unwrap() {
                data[field] = value.clone();
            }
            let node: VirtualNode = serde_json::from_value(data.clone()).unwrap();

            let expected = if problems.is_empty() { Ok(()) } else { Err(problems) };
            assert_eq!(node.validate(), expected, "{data}");
        }
    }

    /// [`VirtualNode::validate_reference`] should refuse an attribute the type does not have, and
    /// pass the default attribute and a placeholder.
    #[test]
    fn test_validate_reference_checks_each_rule() {
        use crate::nodes::NodeAttribute::{Inflow, Volume};
        use crate::validation::NodeReferenceProblem::AttributeNotSupported;

        let storage = VirtualNode::from(VirtualNodeType::VirtualStorage);
        let placeholder = VirtualNode::from(VirtualNodeType::Placeholder);

        let cases = [
            (&storage, None, Ok(())),
            (&storage, Some(Volume), Ok(())),
            (
                &storage,
                Some(Inflow),
                Err(AttributeNotSupported {
                    attribute: Inflow,
                    supported: storage.attributes(),
                }),
            ),
            (&placeholder, Some(Inflow), Ok(())),
        ];

        for (node, attribute, expected) in cases {
            assert_eq!(
                node.validate_reference(attribute),
                expected,
                "{} reading {attribute:?}",
                node.node_type()
            );
        }
    }

    /// [`VirtualNode::validate_member`] should refuse a member breaking each rule, and pass a
    /// placeholder on either side.
    #[test]
    fn test_validate_member_checks_each_rule() {
        use crate::metric::NodeComponentReference;
        use crate::nodes::NodeComponent::{Compensation, Inflow, Loss, Outflow};
        use crate::nodes::{Node, NodeType};
        use crate::validation::MemberProblem::*;

        let aggregated = VirtualNode::from(VirtualNodeType::Aggregated);
        let aggregated_storage = VirtualNode::from(VirtualNodeType::AggregatedStorage);
        let placeholder = VirtualNode::from(VirtualNodeType::Placeholder);

        let link = Node::from(NodeType::Link);
        let storage = Node::from(NodeType::Storage);
        // A reservoir builds no components until it has a compensation, rainfall or evaporation.
        let reservoir = Node::from(NodeType::Reservoir);
        let node_placeholder = Node::from(NodeType::Placeholder);

        let member = |component| NodeComponentReference {
            name: "member".to_string(),
            component,
        };

        let cases = [
            (&aggregated, member(None), &link, Ok(())),
            (
                &aggregated,
                member(Some(Loss)),
                &link,
                Err(ComponentNotBuilt {
                    component: Loss,
                    default: false,
                    built: vec![Inflow, Outflow],
                }),
            ),
            (
                &aggregated,
                member(None),
                &reservoir,
                Err(ComponentNotBuilt {
                    component: Compensation,
                    default: true,
                    built: vec![],
                }),
            ),
            (&aggregated, member(None), &storage, Err(WithoutComponents)),
            (&aggregated_storage, member(None), &storage, Ok(())),
            (&aggregated_storage, member(None), &link, Err(NotStorage)),
            (&aggregated_storage, member(None), &node_placeholder, Ok(())),
            (&placeholder, member(Some(Loss)), &link, Ok(())),
        ];

        for (virtual_node, member, node, expected) in cases {
            assert_eq!(
                virtual_node.validate_member(&member, node),
                expected,
                "{} taking {:?} of {}",
                virtual_node.node_type(),
                member.component,
                node.node_type()
            );
        }
    }

    /// Every [`VirtualNodeType`] should convert to the [`VirtualNode`] variant it discriminates.
    #[test]
    fn test_virtual_node_from_virtual_node_type() {
        for node_type in VirtualNodeType::iter() {
            let node: VirtualNode = node_type.into();
            assert_eq!(node.node_type(), node_type);
        }
    }

    /// The mutable metadata accessor should reach the metadata of every virtual node variant.
    #[test]
    fn test_virtual_node_meta_mut() {
        for node_type in VirtualNodeType::iter() {
            let mut node: VirtualNode = node_type.into();
            node.meta_mut().name = "renamed".to_string();
            assert_eq!(node.name(), "renamed");
        }
    }

    /// A virtual node should not list the same attribute twice.
    #[test]
    fn test_attributes_are_unique() {
        for node_type in VirtualNodeType::iter() {
            let node: VirtualNode = node_type.into();
            let attributes = node.attributes();

            for (i, attribute) in attributes.iter().enumerate() {
                assert!(
                    !attributes[i + 1..].contains(attribute),
                    "{node_type} lists the attribute {attribute} more than once"
                );
            }
        }
    }

    /// The attributes a virtual node lists should be exactly those its build accepts.
    ///
    /// This pins the schema-only list to [`VirtualNode::create_metric`], which is where an
    /// unsupported attribute is refused, so that the two cannot drift apart.
    #[cfg(feature = "core")]
    #[test]
    fn test_attributes_match_create_metric() {
        use crate::nodes::NodeAttribute;

        for node_type in VirtualNodeType::iter() {
            let node: VirtualNode = node_type.into();
            let attributes = node.attributes();

            for attribute in NodeAttribute::iter() {
                let result = node.create_metric(Some(attribute));

                if attributes.contains(&attribute) {
                    assert!(
                        result.is_ok(),
                        "{node_type} lists the attribute {attribute} but refuses it in a metric"
                    );
                } else {
                    assert!(
                        result.is_err(),
                        "{node_type} does not list the attribute {attribute} but accepts it in a metric"
                    );
                }
            }
        }
    }

    /// Test all the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/nodes/virtual_nodes/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(&p).unwrap_or_else(|_| panic!("Failed to read file: {p:?}",));

                let value: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));

                match value {
                    serde_json::Value::Object(_) => {
                        let _ = serde_json::from_value::<VirtualNode>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    serde_json::Value::Array(_) => {
                        let _ = serde_json::from_value::<Vec<VirtualNode>>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    _ => panic!("Expected JSON object or array: {p:?}",),
                }
            }
        }
    }
}
