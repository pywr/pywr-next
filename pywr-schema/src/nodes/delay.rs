use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::nodes::NodeMeta;
use crate::parameters::ConstantValue;
use pywr_core::metric::Metric;
use pywr_v1_schema::nodes::DelayNode as DelayNodeV1;

#[doc = svgbobdoc::transform!(
/// This node is used to introduce a delay between flows entering and leaving the node.
///
/// This is often useful in long river reaches as a simply way to model time-of-travel. Internally
/// an `Output` node is used to terminate flows entering the node and an `Input` node is created
/// with flow constraints set by a [DelayParameter]. These constraints set the minimum and
/// maximum flow on the `Input` node equal to the flow reaching the `Output` node N time-steps
/// ago. The internally created [DelayParameter] is created with this node's name and the suffix
/// "-delay".
///
///
/// ```svgbob
///
///      U  <node.inflow>  D
///     -*---> O    I --->*-
///             <node.outflow>
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct DelayNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub delay: usize,
    pub initial_value: ConstantValue<f64>,
}

impl DelayNode {
    fn output_sub_name() -> Option<&'static str> {
        Some("inflow")
    }

    fn input_sub_now() -> Option<&'static str> {
        Some("outflow")
    }

    pub fn add_to_model(&self, model: &mut pywr_core::model::Model) -> Result<(), SchemaError> {
        model.add_output_node(self.meta.name.as_str(), Self::output_sub_name())?;
        model.add_input_node(self.meta.name.as_str(), Self::input_sub_now())?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
    ) -> Result<(), SchemaError> {
        // Create the delay parameter
        let name = format!("{}-delay", self.meta.name.as_str());
        let output_idx = model.get_node_index_by_name(self.meta.name.as_str(), Self::output_sub_name())?;
        let metric = Metric::NodeInFlow(output_idx);
        let p = pywr_core::parameters::DelayParameter::new(&name, metric, self.delay, self.initial_value.load(tables)?);
        let delay_idx = model.add_parameter(Box::new(p))?;

        // Apply it as a constraint on the input node.
        let metric = Metric::ParameterValue(delay_idx);
        model.set_node_max_flow(self.meta.name.as_str(), Self::input_sub_now(), metric.clone().into())?;
        model.set_node_min_flow(self.meta.name.as_str(), Self::input_sub_now(), metric.into())?;

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Inflow goes to the output node
        vec![(self.meta.name.as_str(), Self::output_sub_name().map(|s| s.to_string()))]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Outflow goes from the input node
        vec![(self.meta.name.as_str(), Self::input_sub_now().map(|s| s.to_string()))]
    }

    pub fn default_metric(&self, model: &pywr_core::model::Model) -> Result<Metric, SchemaError> {
        let idx = model.get_node_index_by_name(self.meta.name.as_str(), Self::input_sub_now().as_deref())?;
        Ok(Metric::NodeOutFlow(idx))
    }
}

impl TryFrom<DelayNodeV1> for DelayNode {
    type Error = ConversionError;

    fn try_from(v1: DelayNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        // TODO convert days & timesteps to a usize as we don;t support non-daily timesteps at the moment
        let delay = match v1.days {
            Some(days) => days,
            None => match v1.timesteps {
                Some(ts) => ts,
                None => {
                    return Err(ConversionError::MissingAttribute {
                        name: meta.name,
                        attrs: vec!["days".to_string(), "timesteps".to_string()],
                    })
                }
            },
        } as usize;

        let initial_value = ConstantValue::Literal(v1.initial_flow.unwrap_or_default());

        let n = Self {
            meta,
            delay,
            initial_value,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::PywrModel;
    use ndarray::{concatenate, Array2, Axis};
    use pywr_core::metric::Metric;
    use pywr_core::recorders::AssertionRecorder;
    use pywr_core::test_utils::run_all_solvers;
    use pywr_core::timestep::Timestepper;

    fn model_str() -> &'static str {
        include_str!("../test_models/delay1.json")
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let (mut model, timestepper): (pywr_core::model::Model, Timestepper) = schema.build_model(None, None).unwrap();

        assert_eq!(model.nodes.len(), 4);
        assert_eq!(model.edges.len(), 2);

        // TODO put this assertion data in the test model file.
        let idx = model.get_node_by_name("link1", Some("inflow")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 15.0);
        let recorder = AssertionRecorder::new("link1-inflow", Metric::NodeInFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link1", Some("outflow")).unwrap().index();
        let expected = concatenate![
            Axis(0),
            Array2::from_elem((3, 1), 0.0),
            Array2::from_elem((363, 1), 15.0)
        ];
        let recorder = AssertionRecorder::new("link1-outflow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &timestepper);
    }
}
