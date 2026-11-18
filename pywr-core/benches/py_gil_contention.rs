/// Benchmarks for GIL contention when running scenarios in parallel with Python parameters.
///
/// Each scenario runs on its own Rayon thread, but every call into a Python parameter must
/// acquire the (single) GIL. These benchmarks run the same model with Python parameters and with
/// equivalent Rust parameters across a range of thread counts. If the GIL is a bottleneck then the
/// Python variants will stop scaling with the number of threads while the Rust variants continue to.
///
/// The model is made of a number of independent `input -> link -> output` systems. The max flow
/// of each output is given by a parameter. In the Python variants this is a custom parameter that
/// performs a configurable amount of work (a loop of `work` iterations) before returning.
///
/// Run with:
///   cargo bench -p pywr-core --no-default-features --features clp,pyo3 --bench py_gil_contention
use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jiff::civil::date;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use pywr_core::metric::UnresolvedMetricF64;
use pywr_core::models::{Model, ModelBuilder, ModelDomainBuilder, ModelTimings};
use pywr_core::network::NetworkBuilder;
use pywr_core::node::NodeBuilder;
use pywr_core::parameters::{ConstantParameterBuilder, PyClassParameterBuilder};
use pywr_core::scenario::{ScenarioDomainBuilder, ScenarioGroupBuilder};
use pywr_core::solvers::ClpSolverSettingsBuilder;
use pywr_core::timestep::{TimeDomainBuilder, TimestepDuration};
use std::num::NonZeroU64;

const NUM_SYSTEMS: usize = 10;
const NUM_SCENARIOS: usize = 32;
const NUM_DAYS: i8 = 30;
const THREADS: [usize; 5] = [1, 2, 4, 8, 16];
/// Number of loop iterations each Python parameter performs per call. Zero measures the bare
/// call overhead (GIL acquire, argument conversion and method dispatch).
const PY_WORK: [usize; 3] = [0, 100, 1000];

const PY_PARAMETER: &std::ffi::CStr = c"
class WorkParameter:
    def __init__(self, work):
        self.work = work

    def before(self, info):
        s = 0
        for i in range(self.work):
            s += i
        return 10.0
";

#[derive(Clone, Copy)]
enum ParameterKind {
    Rust,
    Python { work: usize },
}

fn make_model(kind: ParameterKind) -> Model {
    let mut builder = NetworkBuilder::default();

    let py_class: Option<Py<PyAny>> = match kind {
        ParameterKind::Rust => None,
        ParameterKind::Python { .. } => Some(Python::attach(|py| {
            PyModule::from_code(py, PY_PARAMETER, c"gil_bench.py", c"gil_bench")
                .unwrap()
                .getattr("WorkParameter")
                .unwrap()
                .unbind()
        })),
    };

    for i in 0..NUM_SYSTEMS {
        let input = format!("input-{i}");
        let link = format!("link-{i}");
        let output = format!("output-{i}");
        let demand = format!("demand-{i}");

        let mut input_node = NodeBuilder::input(input.as_str());
        input_node.max_flow(100.0.into());
        builder.node(input_node);
        builder.node(NodeBuilder::link(link.as_str()));

        let mut output_node = NodeBuilder::output(output.as_str());
        output_node
            .max_flow(UnresolvedMetricF64::new_parameter_before(demand.as_str()))
            .cost((-10.0).into());
        builder.node(output_node);

        builder.connect(input.as_str(), link.as_str());
        builder.connect(link.as_str(), output.as_str());

        match (kind, &py_class) {
            (ParameterKind::Python { work }, Some(class)) => {
                let (class, args, kwargs) = Python::attach(|py| {
                    (
                        class.clone_ref(py),
                        PyTuple::new(py, [work]).unwrap().unbind(),
                        PyDict::new(py).unbind(),
                    )
                });
                let p = PyClassParameterBuilder::new(demand.as_str().into(), class, args, kwargs);
                builder.parameters().f64(Box::new(p));
            }
            _ => {
                let p = ConstantParameterBuilder::new(demand.as_str().into(), 10.0);
                builder.parameters().f64(Box::new(p));
            }
        }
    }

    let start = date(2020, 1, 1).at(0, 0, 0, 0);
    let end = date(2020, 1, NUM_DAYS).at(0, 0, 0, 0);
    let time = TimeDomainBuilder::new(start, end, TimestepDuration::Days(NonZeroU64::new(1).unwrap()));

    let mut scenarios = ScenarioDomainBuilder::default();
    scenarios.with_group(ScenarioGroupBuilder::new("scenarios", NUM_SCENARIOS));

    let mut domain = ModelDomainBuilder::new(time);
    domain.scenario(scenarios);

    ModelBuilder::new(domain.build().unwrap(), builder)
        .build()
        .expect("Failed to build model")
}

fn bench_gil_contention(c: &mut Criterion) {
    Python::initialize();

    let mut variants = vec![("rust".to_string(), ParameterKind::Rust)];
    for work in PY_WORK {
        variants.push((format!("python-work-{work}"), ParameterKind::Python { work }));
    }

    let mut group = c.benchmark_group("py-gil-contention");
    group.sample_size(10);
    // Throughput is the number of parameter evaluations per run.
    group.throughput(Throughput::Elements(
        (NUM_SYSTEMS * NUM_SCENARIOS * NUM_DAYS as usize) as u64,
    ));

    for (label, kind) in variants {
        let model = make_model(kind);

        for threads in THREADS {
            let settings = ClpSolverSettingsBuilder::default().parallel().threads(threads).build();

            group.bench_function(BenchmarkId::new(&label, threads), |b| {
                b.iter_batched(
                    || {
                        let state = model.setup(&settings).expect("Failed to setup the model.");
                        let timings = ModelTimings::new_with_component_timings(&model);
                        (state, timings)
                    },
                    |(mut state, mut timings)| {
                        model
                            .run_with_state(&mut state, &settings, &mut timings)
                            .expect("Failed to run the model.")
                    },
                    BatchSize::LargeInput,
                )
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_gil_contention);
criterion_main!(benches);
