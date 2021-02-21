pub mod py;
use super::{NetworkState, ParameterState, PywrError};
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use ndarray::Array2;

pub type ParameterIndex = usize;

/// Meta data common to all parameters.
#[derive(Debug)]
pub struct ParameterMeta {
    pub index: Option<ParameterIndex>,
    pub name: String,
    pub comment: String,
}

impl ParameterMeta {
    fn new(name: &str) -> Self {
        Self {
            index: None,
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

pub trait Parameter {
    fn meta(&self) -> &ParameterMeta;
    fn before(&self) {}
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError>;
}

pub struct ConstantParameter {
    meta: ParameterMeta,
    value: f64,
}

impl ConstantParameter {
    pub fn new(name: &str, value: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            value,
        }
    }
}

impl Parameter for ConstantParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        Ok(self.value)
    }
}

pub struct VectorParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl VectorParameter {
    pub fn new(name: &str, values: Vec<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values,
        }
    }
}

impl Parameter for VectorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        match self.values.get(timestep.index) {
            Some(v) => Ok(*v),
            None => return Err(PywrError::TimestepIndexOutOfRange),
        }
    }
}

pub struct Array2Parameter {
    meta: ParameterMeta,
    array: Array2<f64>,
}

impl Array2Parameter {
    pub fn new(name: &str, array: Array2<f64>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            array,
        }
    }
}

impl Parameter for Array2Parameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        _parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // This panics if out-of-bounds
        // TODO scenarios!
        Ok(self.array[[timestep.index, 0]])
    }
}

pub enum AggFunc {
    Sum,
    Product,
    Mean,
    Min,
    Max,
}

pub struct AggregatedParameter {
    meta: ParameterMeta,
    parameter_indices: Vec<ParameterIndex>,
    agg_func: AggFunc,
}

impl AggregatedParameter {
    pub fn new(name: &str, parameter_indices: Vec<ParameterIndex>, agg_func: AggFunc) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            parameter_indices,
            agg_func,
        }
    }
}

impl Parameter for AggregatedParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        // TODO scenarios!

        let value: f64 = match self.agg_func {
            AggFunc::Sum => {
                let mut total = 0.0_f64;
                for idx in &self.parameter_indices {
                    total += match parameter_state.get(idx.clone()) {
                        Some(v) => v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total
            }
            AggFunc::Mean => {
                let mut total = 0.0_f64;
                for idx in &self.parameter_indices {
                    total += match parameter_state.get(idx.clone()) {
                        Some(v) => v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total / self.parameter_indices.len() as f64
            }
            AggFunc::Max => {
                let mut total = f64::MIN;
                for idx in &self.parameter_indices {
                    total = total.max(match parameter_state.get(idx.clone()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    });
                }
                total
            }
            AggFunc::Min => {
                let mut total = f64::MAX;
                for idx in &self.parameter_indices {
                    total = total.min(match parameter_state.get(idx.clone()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    });
                }
                total
            }
            AggFunc::Product => {
                let mut total = 1.0_f64;
                for idx in &self.parameter_indices {
                    total *= match parameter_state.get(idx.clone()) {
                        Some(v) => *v,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    };
                }
                total
            }
        };

        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestep::Timestepper;
    use ndarray::prelude::*;

    fn test_timestepper() -> Timestepper {
        Timestepper::new("2020-01-01", "2020-12-31", "%Y-%m-%d", 1).unwrap()
    }

    #[test]
    /// Test `ConstantParameter` returns the correct value.
    fn test_constant_parameter() {
        let param = ConstantParameter::new("my-parameter", 3.14);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            assert_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), 3.14);
        }
    }

    #[test]
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter() {
        let data = Array::range(0.0, 366.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let param = Array2Parameter::new("my-array-parameter", data);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            assert_eq!(param.compute(ts, &si, &ns, &ps).unwrap(), ts.index as f64);
        }
    }

    #[test]
    #[should_panic] // TODO this is not great; but a problem with using ndarray slicing.
    /// Test `Array2Parameter` returns the correct value.
    fn test_array2_parameter_not_enough_data() {
        let data = Array::range(0.0, 100.0, 1.0);
        let data = data.insert_axis(Axis(1));
        let param = Array2Parameter::new("my-array-parameter", data);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            let ps = ParameterState::new();
            let value = param.compute(ts, &si, &ns, &ps);
        }
    }

    #[test]
    fn test_aggregated_parameter_sum() {
        let mut parameter_state = ParameterState::new();
        // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
        parameter_state.push(10.0);
        parameter_state.push(2.0);
        test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Sum, 12.0);
    }

    #[test]
    fn test_aggregated_parameter_mean() {
        let mut parameter_state = ParameterState::new();
        // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
        parameter_state.push(10.0);
        parameter_state.push(2.0);
        test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Mean, 6.0);
    }

    #[test]
    fn test_aggregated_parameter_max() {
        let mut parameter_state = ParameterState::new();
        // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
        parameter_state.push(10.0);
        parameter_state.push(2.0);
        test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Max, 10.0);
    }

    #[test]
    fn test_aggregated_parameter_min() {
        let mut parameter_state = ParameterState::new();
        // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
        parameter_state.push(10.0);
        parameter_state.push(2.0);
        test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Min, 2.0);
    }

    #[test]
    fn test_aggregated_parameter_product() {
        let mut parameter_state = ParameterState::new();
        // Parameter's 0 and 1 have values of 10.0 and 2.0 respectively
        parameter_state.push(10.0);
        parameter_state.push(2.0);
        test_aggregated_parameter(vec![0, 1], &parameter_state, AggFunc::Product, 20.0);
    }

    /// Test `AggregatedParameter` returns the correct value.
    fn test_aggregated_parameter(
        parameter_indices: Vec<ParameterIndex>,
        parameter_state: &ParameterState,
        agg_func: AggFunc,
        expected: f64,
    ) {
        let param = AggregatedParameter::new("my-aggregation", parameter_indices, agg_func);
        let timestepper = test_timestepper();
        let si = ScenarioIndex {
            index: 0,
            indices: vec![0],
        };

        for ts in timestepper.timesteps().iter() {
            let ns = NetworkState::new();
            assert_eq!(param.compute(ts, &si, &ns, &parameter_state).unwrap(), expected);
        }
    }
}
