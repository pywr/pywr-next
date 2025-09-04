use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
use strum_macros::{Display, EnumIter};

/// All possible attributes that could be produced by a node.
///
///
#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Display, JsonSchema, PywrVisitAll, PartialEq, EnumIter,
)]
pub enum NodeAttribute {
    Inflow,
    Outflow,
    Volume,
    MaxVolume,
    ProportionalVolume,
    Loss,
    Deficit,
    Power,
    /// The compensation flow.
    Compensation,
    /// The rainfall volume.
    Rainfall,
    /// The evaporation volume.
    Evaporation,
    /// The abstracted flow
    Abstraction,
}

/// Macro to generate a subset enum of `NodeAttribute` with conversion implementations.
///
/// Usage:
/// ```
/// use pywr_schema::node_attribute_subset_enum;
///
/// node_attribute_subset_enum! {
///     pub enum MySubset {
///         Inflow,
///         Outflow,
///         Volume,
///     }
/// }
/// ```
///
/// This generates a `MySubset` enum and implements:
/// - `From<MySubset> for NodeAttribute`
/// - `TryFrom<NodeAttribute> for MySubset`
///
#[macro_export]
macro_rules! node_attribute_subset_enum {
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

        impl std::convert::From<$name> for $crate::nodes::NodeAttribute {
            fn from(attr: $name) -> $crate::nodes::NodeAttribute {
                match attr {
                    $(
                        $name::$variant => $crate::nodes::NodeAttribute::$variant,
                    )*
                }
            }
        }

        impl std::convert::TryFrom<$crate::nodes::NodeAttribute> for $name {
            type Error = $crate::SchemaError;
            fn try_from(attr: $crate::nodes::NodeAttribute) -> Result<$name, Self::Error> {
                match attr {
                    $(
                        $crate::nodes::NodeAttribute::$variant => Ok($name::$variant),
                    )*
                    _ => Err($crate::SchemaError::NodeAttributeNotSupported {
                        attr,
                    })
                }
            }
        }
    };
}
