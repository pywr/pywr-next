use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
use strum_macros::{Display, EnumIter};

/// All possible components that might be present in a node.
///
///
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Display, JsonSchema, PywrVisitAll, PartialEq, EnumIter,
)]
pub enum NodeComponent {
    Inflow,
    Outflow,
    Volume,
    MaxVolume,
    ProportionalVolume,
    Loss,
    /// The compensation flow.
    Compensation,
    /// The rainfall flow.
    Rainfall,
    /// The evaporation flow.
    Evaporation,
    /// The abstracted flow.
    Abstraction,
}

/// Macro to generate a subset enum of `NodeComponent` with conversion implementations.
///
/// Usage:
/// ```
/// use pywr_schema::node_component_subset_enum;
///
/// node_component_subset_enum! {
///     pub enum MySubset {
///         Inflow,
///         Outflow,
///         Volume,
///     }
/// }
/// ```
///
/// This generates a `MySubset` enum and implements:
/// - `From<MySubset> for NodeComponent`
/// - `TryFrom<NodeComponent> for MySubset`
///
#[macro_export]
macro_rules! node_component_subset_enum {
    (
        $(#[$meta:meta])* $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident $( ( $($field:ty),* ) )?
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant $( ( $($field),* ) )?
            ),*
        }

        impl std::convert::From<$name> for $crate::nodes::NodeComponent {
            fn from(attr: $name) -> $crate::nodes::NodeComponent {
                match attr {
                    $(
                        $name::$variant => $crate::nodes::NodeComponent::$variant,
                    )*
                }
            }
        }

        impl std::convert::TryFrom<$crate::nodes::NodeComponent> for $name {
            type Error = $crate::SchemaError;
            fn try_from(attr: $crate::nodes::NodeComponent) -> Result<$name, Self::Error> {
                match attr {
                    $(
                        $crate::nodes::NodeComponent::$variant => Ok($name::$variant),
                    )*
                    _ => Err($crate::SchemaError::NodeComponentNotSupported {
                        attr,
                    })
                }
            }
        }
    };
}
