use super::{
    BuiltParameter, ConstCalculationError, ConstParameter, ConstParameterIndex, GeneralAfterParameter,
    GeneralAfterParameterHook, GeneralBeforeParameter, GeneralCalculationError, GeneralParameter,
    GeneralParameterContext, GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterSetupError, ParameterState, SimpleCalculationError,
    SimpleParameter, SimpleParameterContext, SimpleParameterIndex,
};
use crate::metric::{ConstantMetricF64Error, SimpleMetricF64Error};
use crate::network::ResolutionMaps;
use crate::scenario::ScenarioIndex;
use crate::state::{ConstParameterValues, ConstParameterValuesError, MultiValue, SimpleParameterValues};
use crate::timestep::Timestep;
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) type EventLog = Arc<Mutex<Vec<String>>>;

#[derive(Debug, Clone, Copy)]
pub(crate) enum TestBuildKind {
    Const,
    Simple,
    General,
}

#[derive(Debug, Clone, Copy)]
enum TestGeneralPhase {
    Before,
    Both,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TestValueType {
    F64,
    U64,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestParameterFailure {
    None,
    Setup,
    Const,
    Simple,
    GeneralBefore,
    GeneralAfter,
    GeneralHook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestParameterMode {
    Default,
    Phase,
    NetworkLifecycle,
    ScenarioCounter,
}

pub(crate) trait TestValue: Debug + Send + Sync + 'static {
    fn before_value(mode: TestParameterMode, state: Option<&TestParameterState>) -> Self;
    fn after_value(mode: TestParameterMode) -> Self;

    fn const_value(
        mode: TestParameterMode,
        state: Option<&TestParameterState>,
        _values: &ConstParameterValues<'_>,
        _dependency: Option<ConstParameterIndex<f64>>,
    ) -> Result<Self, ConstCalculationError>
    where
        Self: Sized,
    {
        Ok(Self::before_value(mode, state))
    }

    fn simple_value(
        mode: TestParameterMode,
        state: Option<&TestParameterState>,
        _values: &SimpleParameterValues<'_>,
        _dependency: Option<SimpleParameterIndex<f64>>,
    ) -> Result<Self, SimpleCalculationError>
    where
        Self: Sized,
    {
        Ok(Self::before_value(mode, state))
    }
}

impl TestValue for f64 {
    fn before_value(mode: TestParameterMode, state: Option<&TestParameterState>) -> Self {
        match mode {
            TestParameterMode::Default | TestParameterMode::Phase => 11.0,
            TestParameterMode::NetworkLifecycle => 30.0,
            TestParameterMode::ScenarioCounter => {
                let state = state.expect("scenario counter requires internal state");
                state.scenario_id as f64 * 100.0 + state.calls as f64
            }
        }
    }

    fn after_value(mode: TestParameterMode) -> Self {
        match mode {
            TestParameterMode::Default | TestParameterMode::ScenarioCounter => 21.0,
            TestParameterMode::Phase => 22.0,
            TestParameterMode::NetworkLifecycle => 40.0,
        }
    }

    fn const_value(
        mode: TestParameterMode,
        state: Option<&TestParameterState>,
        values: &ConstParameterValues<'_>,
        dependency: Option<ConstParameterIndex<f64>>,
    ) -> Result<Self, ConstCalculationError> {
        dependency.map_or_else(
            || Ok(Self::before_value(mode, state)),
            |index| {
                values.get_f64(index).map(|value| value + 1.0).map_err(|source| {
                    ConstCalculationError::ConstantMetricF64Error(ConstantMetricF64Error::ConstParameterValuesError(
                        source,
                    ))
                })
            },
        )
    }

    fn simple_value(
        mode: TestParameterMode,
        state: Option<&TestParameterState>,
        values: &SimpleParameterValues<'_>,
        dependency: Option<SimpleParameterIndex<f64>>,
    ) -> Result<Self, SimpleCalculationError> {
        dependency.map_or_else(
            || Ok(Self::before_value(mode, state)),
            |index| {
                values.get_f64(index).map(|value| value + 1.0).map_err(|source| {
                    SimpleCalculationError::SimpleMetricF64Error(SimpleMetricF64Error::SimpleParameterValuesError(
                        source,
                    ))
                })
            },
        )
    }
}

impl TestValue for u64 {
    fn before_value(_mode: TestParameterMode, _state: Option<&TestParameterState>) -> Self {
        12
    }

    fn after_value(_mode: TestParameterMode) -> Self {
        22
    }
}

impl TestValue for MultiValue {
    fn before_value(_mode: TestParameterMode, _state: Option<&TestParameterState>) -> Self {
        MultiValue::new(
            HashMap::from([("value".to_string(), 13.0)]),
            HashMap::from([("index".to_string(), 14)]),
        )
    }

    fn after_value(_mode: TestParameterMode) -> Self {
        MultiValue::new(
            HashMap::from([("value".to_string(), 23.0)]),
            HashMap::from([("index".to_string(), 24)]),
        )
    }
}

#[derive(Debug)]
pub(crate) struct TestParameterState {
    owner: String,
    calls: usize,
    timestep_count: usize,
    scenario_id: usize,
}

impl TestParameterState {
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls
    }

    pub(crate) fn timestep_count(&self) -> usize {
        self.timestep_count
    }

    pub(crate) fn scenario_id(&self) -> usize {
        self.scenario_id
    }
}

pub(crate) fn test_parameter_state(state: &Option<Box<dyn ParameterState>>) -> Option<&TestParameterState> {
    state
        .as_deref()
        .and_then(|state| state.as_any().downcast_ref::<TestParameterState>())
}

#[derive(Debug)]
pub(crate) struct TestParameter<T> {
    meta: ParameterMeta,
    events: Option<EventLog>,
    setup_calls: Arc<AtomicUsize>,
    failure: TestParameterFailure,
    state_enabled: bool,
    record_setup: bool,
    mode: TestParameterMode,
    const_dependency: Option<ConstParameterIndex<f64>>,
    simple_dependency: Option<SimpleParameterIndex<f64>>,
    phantom: PhantomData<T>,
}

impl<T> TestParameter<T> {
    pub(crate) fn named(name: &str) -> Self {
        Self {
            meta: ParameterMeta::new(name.into()),
            events: None,
            setup_calls: Arc::new(AtomicUsize::new(0)),
            failure: TestParameterFailure::None,
            state_enabled: false,
            record_setup: false,
            mode: TestParameterMode::Default,
            const_dependency: None,
            simple_dependency: None,
            phantom: PhantomData,
        }
    }

    pub(crate) fn new(name: &str, events: EventLog) -> Self {
        Self::named(name).with_events(events).with_state()
    }

    pub(crate) fn phase(name: &str, events: EventLog) -> Self {
        let mut parameter = Self::named(name).with_events(events);
        parameter.mode = TestParameterMode::Phase;
        parameter
    }

    pub(crate) fn network_lifecycle(name: &str, events: EventLog) -> Self {
        let mut parameter = Self::new(name, events);
        parameter.mode = TestParameterMode::NetworkLifecycle;
        parameter.record_setup = true;
        parameter
    }

    pub(crate) fn scenario_counter(name: &str) -> Self {
        let mut parameter = Self::named(name).with_state();
        parameter.mode = TestParameterMode::ScenarioCounter;
        parameter
    }

    pub(crate) fn with_events(mut self, events: EventLog) -> Self {
        self.events = Some(events);
        self
    }

    pub(crate) fn with_state(mut self) -> Self {
        self.state_enabled = true;
        self
    }

    pub(crate) fn failing(mut self, failure: TestParameterFailure) -> Self {
        self.failure = failure;
        self
    }

    pub(crate) fn with_const_dependency(mut self, dependency: ConstParameterIndex<f64>) -> Self {
        self.const_dependency = Some(dependency);
        self
    }

    pub(crate) fn with_simple_dependency(mut self, dependency: SimpleParameterIndex<f64>) -> Self {
        self.simple_dependency = Some(dependency);
        self
    }

    pub(crate) fn setup_calls(&self) -> Arc<AtomicUsize> {
        self.setup_calls.clone()
    }

    fn record(&self, phase: &str) {
        if let Some(events) = &self.events {
            events.lock().unwrap().push(format!("{}:{phase}", self.meta.name));
        }
    }

    fn state_mut<'a>(
        &self,
        internal_state: &'a mut Option<Box<dyn ParameterState>>,
        scenario_id: usize,
    ) -> Result<Option<&'a mut TestParameterState>, String> {
        if !self.state_enabled {
            return Ok(None);
        }
        let state = internal_state
            .as_deref_mut()
            .and_then(|state| state.as_any_mut().downcast_mut::<TestParameterState>())
            .ok_or_else(|| "missing or invalid probe state".to_string())?;
        if state.owner != self.meta.name.to_string() {
            return Err("probe state belongs to another parameter".to_string());
        }
        if state.scenario_id != scenario_id {
            return Err("probe state belongs to another scenario".to_string());
        }
        state.calls += 1;
        Ok(Some(state))
    }
}

impl<T: TestValue> Parameter for TestParameter<T> {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        self.setup_calls.fetch_add(1, Ordering::Relaxed);
        if self.failure == TestParameterFailure::Setup {
            return Err(ParameterSetupError::TestError("lifecycle-probe".to_string()));
        }
        if self.record_setup {
            self.record("setup");
        }
        Ok(self.state_enabled.then(|| {
            Box::new(TestParameterState {
                owner: self.meta.name.to_string(),
                calls: 0,
                timestep_count: timesteps.len(),
                scenario_id: scenario_index.simulation_id(),
            }) as Box<dyn ParameterState>
        }))
    }
}

fn intentional_const_error() -> ConstCalculationError {
    ConstCalculationError::ConstantMetricF64Error(ConstantMetricF64Error::ConstParameterValuesError(
        ConstParameterValuesError::ConstParameterIndexNotFound(ConstParameterIndex::new(usize::MAX)),
    ))
}

impl<T: TestValue> ConstParameter<T> for TestParameter<T> {
    fn compute(
        &self,
        scenario_index: &ScenarioIndex,
        values: &ConstParameterValues,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, ConstCalculationError> {
        if self.failure == TestParameterFailure::Const {
            return Err(intentional_const_error());
        }
        let state = self
            .state_mut(internal_state, scenario_index.simulation_id())
            .map_err(|_| intentional_const_error())?;
        self.record("const");
        T::const_value(self.mode, state.as_deref(), values, self.const_dependency)
    }

    fn as_parameter(&self) -> &dyn Parameter {
        self
    }
}

impl<T: TestValue> SimpleParameter<T> for TestParameter<T> {
    fn compute(
        &self,
        context: SimpleParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, SimpleCalculationError> {
        if self.failure == TestParameterFailure::Simple {
            return Err(SimpleCalculationError::Internal {
                message: "intentional simple failure".to_string(),
            });
        }
        let state = self
            .state_mut(internal_state, context.scenario_index.simulation_id())
            .map_err(|message| SimpleCalculationError::Internal { message })?;
        self.record(if self.mode == TestParameterMode::Phase {
            "before"
        } else {
            "simple"
        });
        T::simple_value(self.mode, state.as_deref(), context.values, self.simple_dependency)
    }

    fn as_parameter(&self) -> &dyn Parameter {
        self
    }
}

impl<T: TestValue> GeneralParameter for TestParameter<T> {
    fn as_parameter(&self) -> &dyn Parameter {
        self
    }
}

impl<T: TestValue> GeneralBeforeParameter<T> for TestParameter<T> {
    fn before(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, GeneralCalculationError> {
        if self.failure == TestParameterFailure::GeneralBefore {
            return Err(GeneralCalculationError::Internal {
                message: "intentional general before failure".to_string(),
            });
        }
        let state = self
            .state_mut(internal_state, context.scenario_index.simulation_id())
            .map_err(|message| GeneralCalculationError::Internal { message })?;
        self.record("before");
        Ok(T::before_value(self.mode, state.as_deref()))
    }
}

impl<T: TestValue> GeneralAfterParameter<T> for TestParameter<T> {
    fn after(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<T, GeneralCalculationError> {
        if self.failure == TestParameterFailure::GeneralAfter {
            return Err(GeneralCalculationError::Internal {
                message: "intentional general after failure".to_string(),
            });
        }
        self.state_mut(internal_state, context.scenario_index.simulation_id())
            .map_err(|message| GeneralCalculationError::Internal { message })?;
        self.record("after");
        Ok(T::after_value(self.mode))
    }
}

impl<T: TestValue> GeneralAfterParameterHook<T> for TestParameter<T> {
    fn after(
        &self,
        context: GeneralParameterContext<'_>,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<(), GeneralCalculationError> {
        if self.failure == TestParameterFailure::GeneralHook {
            return Err(GeneralCalculationError::Internal {
                message: "intentional general hook failure".to_string(),
            });
        }
        self.state_mut(internal_state, context.scenario_index.simulation_id())
            .map_err(|message| GeneralCalculationError::Internal { message })?;
        self.record("hook");
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct TestParameterBuilder {
    meta: ParameterMeta,
    kind: TestBuildKind,
    general_phase: TestGeneralPhase,
    dependency: Option<ParameterName>,
    build_error: Option<String>,
    attempts: Arc<AtomicUsize>,
    build_order: EventLog,
    events: Option<EventLog>,
    mode: TestParameterMode,
}

impl Default for TestParameterBuilder {
    fn default() -> Self {
        Self::new("test-parameter", TestBuildKind::Const)
    }
}

impl TestParameterBuilder {
    pub(crate) fn new(name: &str, kind: TestBuildKind) -> Self {
        Self::with_build_order(name, kind, Arc::new(Mutex::new(Vec::new())))
    }

    pub(crate) fn with_build_order(name: &str, kind: TestBuildKind, build_order: EventLog) -> Self {
        Self {
            meta: ParameterMeta::new(name.into()),
            kind,
            general_phase: TestGeneralPhase::Before,
            dependency: None,
            build_error: None,
            attempts: Arc::new(AtomicUsize::new(0)),
            build_order,
            events: None,
            mode: TestParameterMode::Default,
        }
    }

    pub(crate) fn network_lifecycle(name: &str, events: EventLog) -> Self {
        let mut builder = Self::new(name, TestBuildKind::General);
        builder.general_phase = TestGeneralPhase::Both;
        builder.events = Some(events);
        builder.mode = TestParameterMode::NetworkLifecycle;
        builder
    }

    pub(crate) fn scenario_counter(name: &str) -> Self {
        let mut builder = Self::new(name, TestBuildKind::General);
        builder.mode = TestParameterMode::ScenarioCounter;
        builder
    }

    pub(crate) fn depending_on(mut self, dependency: &str) -> Self {
        self.dependency = Some(dependency.into());
        self
    }

    pub(crate) fn failing(mut self, detail: &str) -> Self {
        self.build_error = Some(detail.to_string());
        self
    }

    pub(crate) fn attempts(&self) -> Arc<AtomicUsize> {
        self.attempts.clone()
    }

    fn parameter<T: TestValue>(&self) -> TestParameter<T> {
        match self.mode {
            TestParameterMode::Default | TestParameterMode::Phase => TestParameter::named(&self.meta.name.to_string()),
            TestParameterMode::NetworkLifecycle => {
                TestParameter::network_lifecycle(&self.meta.name.to_string(), self.events.as_ref().unwrap().clone())
            }
            TestParameterMode::ScenarioCounter => TestParameter::scenario_counter(&self.meta.name.to_string()),
        }
    }
}

impl<T: TestValue> ParameterBuilder<T> for TestParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<MaybeBuiltParameter<T>, ParameterBuildError> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        if let Some(detail) = &self.build_error {
            return Err(ParameterBuildError::NoCalculationPhase { detail: detail.clone() });
        }
        if let Some(dependency) = &self.dependency
            && !resolution_maps.parameters_f64.contains_key(dependency)
            && !resolution_maps.parameters_u64.contains_key(dependency)
            && !resolution_maps.parameters_multi.contains_key(dependency)
        {
            return Ok(MaybeBuiltParameter::Retry {
                parameter_not_found: dependency.clone(),
                builder: self,
            });
        }

        self.build_order.lock().unwrap().push(self.meta.name.to_string());
        let parameter = self.parameter::<T>();
        let built = match self.kind {
            TestBuildKind::Const => BuiltParameter::Const(Box::new(parameter)),
            TestBuildKind::Simple => BuiltParameter::Simple(Box::new(parameter)),
            TestBuildKind::General => BuiltParameter::General(match self.general_phase {
                TestGeneralPhase::Before => GeneralParameterEntry::before(parameter),
                TestGeneralPhase::Both => GeneralParameterEntry::both(parameter),
            }),
        };
        Ok(MaybeBuiltParameter::Built(built))
    }
}
