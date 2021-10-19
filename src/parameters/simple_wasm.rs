use super::{NetworkState, ParameterMeta, PywrError, Timestep, _Parameter};
use crate::model::Model;
use crate::parameters::Parameter;
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use wasmer::{imports, Array, Instance, Module, NativeFunc, Store, WasmPtr};

type ValueFunc = NativeFunc<(WasmPtr<f64, Array>, u32), f64>;
type SetFunc = NativeFunc<(WasmPtr<f64, Array>, u32, u32, f64), ()>;

pub struct SimpleWasmParameter {
    meta: ParameterMeta,
    src: Vec<u8>,
    parameters: Vec<Parameter>,
    func: Option<ValueFunc>,
    set_func: Option<SetFunc>,
    ptr: Option<WasmPtr<f64, Array>>,
}

impl SimpleWasmParameter {
    pub fn new(name: &str, src: Vec<u8>, parameters: Vec<Parameter>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            src,
            parameters,
            func: None,
            set_func: None,
            ptr: None,
        }
    }
}

impl _Parameter for SimpleWasmParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &mut self,
        _model: &Model,
        _timesteps: &Vec<Timestep>,
        _scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        let store = Store::default();
        let module = Module::new(&store, &self.src).unwrap();

        // Create an empty import object.
        let import_object = imports! {};

        // Let's instantiate the Wasm module.
        // TODO handle these WASM errors.
        let instance = Instance::new(&module, &import_object).unwrap();
        self.func = Some(instance.exports.get_function("value").unwrap().native().unwrap());

        self.set_func = Some(instance.exports.get_function("set").unwrap().native().unwrap());

        let alloc = instance
            .exports
            .get_function("alloc")
            .unwrap()
            .native::<u32, WasmPtr<f64, Array>>()
            .unwrap();

        self.ptr = Some(alloc.call(self.parameters.len() as u32).unwrap());

        Ok(())
    }
    fn compute(
        &mut self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        _state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<f64, PywrError> {
        let ptr = self
            .ptr
            .ok_or_else(|| PywrError::InternalParameterError("Wasm memory not initialised.".to_string()))?;

        let set_func = self
            .set_func
            .as_ref()
            .ok_or_else(|| PywrError::InternalParameterError("Wasm function not generated.".to_string()))?;

        // Assign the parameter values to the WASM's internal memory
        let len = self.parameters.len() as u32;
        for (idx, p) in self.parameters.iter().enumerate() {
            let v = parameter_state.get_value(p.index())?;

            set_func.call(ptr, len, idx as u32, v).map_err(|e| {
                PywrError::InternalParameterError(format!("Error calling WASM imported function: {:?}.", e))
            })?;
        }

        // Calculate the parameter's final value using the WASM function.
        let value: f64 = self
            .func
            .as_ref()
            .ok_or_else(|| PywrError::InternalParameterError("Wasm function not generated.".to_string()))?
            .call(ptr, len)
            .map_err(|e| {
                PywrError::InternalParameterError(format!("Error calling WASM imported function: {:?}.", e))
            })?;

        Ok(value)
    }
}
