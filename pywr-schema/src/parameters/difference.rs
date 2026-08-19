use schemars::JsonSchema;
use pywr_core::parameters::ParameterName;
use crate::{LoadArgs, SchemaError};
use crate::metric::Metric;
use crate::parameters::{ParameterMeta, ParameterPhase};
use pywr_schema_macros::PywrVisitAll;

/// Schema for a parameter that computes the difference between two metrics, with optional minimum and maximum bounds.
///
/// The calculation is defined as:
/// `result = a - b`, where `a` and `b` are the values of the two metrics.
///
/// If `min` is provided, the result is clamped to be at least `min`.
/// If `max` is provided, the result is clamped to be at most `max`.
///
/// The parameter definition is applied to the network using [`DifferenceParameter`].
///
/// # JSON Examples
///
/// The example below shows the defintion of a [`DifferenceParameter`] that computes the differences between:
///  - a monthly profile
///  - a literal constant: 0.3
/// The difference has a minimum value of 0.0.
///
/// ```json
#[doc= include_str!("doc_examples/difference.json")]
/// ```


#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct DifferenceParameter {
    pub meta: ParameterMeta,
    pub phase: ParameterPhase,
    pub a: Metric,
    pub b: Metric,
    pub min: Option<Metric>,
    pub max: Option<Metric>,
}

#[cfg(feature = "core")]
impl DifferenceParameter {
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<(), SchemaError> {
        let name = ParameterName::new(&self.meta.name, parent);
        let a = self.a.load(network, args, None)?;
        let b = self.b.load(network, args, parent)?;


        let mut builder = match self.phase {
            ParameterPhase::Before => pywr_core::parameters::DifferenceParameterBuilder::before(name, a, b),
            ParameterPhase::After => pywr_core::parameters::DifferenceParameterBuilder::after(name, a, b),
            ParameterPhase::Both => pywr_core::parameters::DifferenceParameterBuilder::both(name, a, b),
        };

        if let Some(max) = &self.max {
            let max = max.load(network, args, parent)?;
            builder.max(max);
        }
        if let Some(min) = &self.min {
            let min = min.load(network, args, parent)?;
            builder.min(min);
        }

        network.parameters().f64(Box::new(builder));

        Ok(())
    }
}
