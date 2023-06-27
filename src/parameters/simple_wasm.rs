use super::{Parameter, ParameterMeta, PywrError, Timestep};
use crate::model::Model;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::ParameterIndex;
use std::any::Any;
use wasmer::{imports, Instance, Module, Store, TypedFunction, WasmPtr};

type ValueFunc = TypedFunction<(WasmPtr<f64>, u32), f64>;
type SetFunc = TypedFunction<(WasmPtr<f64>, u32, u32, f64), ()>;

pub struct SimpleWasmParameter {
    meta: ParameterMeta,
    src: Vec<u8>,
    parameters: Vec<ParameterIndex>,
}

impl SimpleWasmParameter {
    pub fn new(name: &str, src: Vec<u8>, parameters: Vec<ParameterIndex>) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            src,
            parameters,
        }
    }
}

struct Internal {
    func: ValueFunc,
    set_func: SetFunc,
    ptr: WasmPtr<f64>,
    store: Store,
}

impl Parameter for SimpleWasmParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        let mut store = Store::default();
        let module = Module::new(&store, &self.src).unwrap();

        // Create an empty import object.
        let import_object = imports! {};

        // Let's instantiate the Wasm module.
        // TODO handle these WASM errors.
        let instance = Instance::new(&mut store, &module, &import_object).unwrap();
        let func = instance.exports.get_typed_function(&mut store, "value").unwrap();

        let set_func = instance.exports.get_typed_function(&mut store, "set").unwrap();

        let alloc = instance
            .exports
            .get_typed_function::<u32, WasmPtr<f64>>(&mut store, "alloc")
            .unwrap();

        let ptr = alloc.call(&mut store, self.parameters.len() as u32).unwrap();

        let internal_state = Internal {
            func,
            set_func,
            ptr,
            store,
        };

        Ok(Some(Box::new(internal_state)))
    }

    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Model,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Downcast the internal state to the correct type
        let funcs = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // Assign the parameter values to the WASM's internal memory
        let len = self.parameters.len() as u32;
        for (idx, p) in self.parameters.iter().enumerate() {
            let v = state.get_parameter_value(*p)?;

            funcs
                .set_func
                .call(&mut funcs.store, funcs.ptr, len, idx as u32, v)
                .map_err(|e| {
                    PywrError::InternalParameterError(format!("Error calling WASM imported function: {e:?}."))
                })?;
        }

        // Calculate the parameter's final value using the WASM function.
        let value: f64 = funcs
            .func
            .call(&mut funcs.store, funcs.ptr, len)
            .map_err(|e| PywrError::InternalParameterError(format!("Error calling WASM imported function: {e:?}.")))?;

        Ok(value)
    }
}
