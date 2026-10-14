#[cfg(feature = "cbc")]
use super::{CbcSolver, CbcSolverSettings};
#[cfg(feature = "ipm-ocl")]
use super::{ClIpmF32Settings, ClIpmF32Solver, ClIpmF64Settings, ClIpmF64Solver};
#[cfg(feature = "clp")]
use super::{ClpSolver, ClpSolverSettings};
#[cfg(feature = "highs")]
use super::{HighsSolver, HighsSolverSettings};
#[cfg(feature = "microlp")]
use super::{MicroLpSolver, MicroLpSolverSettings};
#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
use super::{MultiStateSolver, MultiStateSolverConfig};
#[cfg(feature = "ipm-simd")]
use super::{SimdIpmF64Solver, SimdIpmSolverSettings};
#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
use super::{Solver, SolverConfig};
#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
use crate::state::ConstParameterValues;
use crate::{
    network::Network,
    solvers::{SolverFeatures, SolverSettings, SolverSetupError, SolverSolveError, SolverTimings},
    state::State,
    timestep::Timestep,
};

#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
pub enum BuiltInSolverConfig {
    #[cfg(feature = "clp")]
    Clp(ClpSolverSettings),

    #[cfg(feature = "cbc")]
    Cbc(CbcSolverSettings),

    #[cfg(feature = "highs")]
    Highs(HighsSolverSettings),

    #[cfg(feature = "microlp")]
    MicroLp(MicroLpSolverSettings),
}

#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
impl SolverSettings for BuiltInSolverConfig {
    fn parallel(&self) -> bool {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => settings.parallel(),
            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => settings.parallel(),
            #[cfg(feature = "highs")]
            Self::Highs(settings) => settings.parallel(),
            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => settings.parallel(),
        }
    }

    fn threads(&self) -> usize {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => settings.threads(),
            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => settings.threads(),
            #[cfg(feature = "highs")]
            Self::Highs(settings) => settings.threads(),
            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => settings.threads(),
        }
    }

    fn ignore_feature_requirements(&self) -> bool {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => settings.ignore_feature_requirements(),
            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => settings.ignore_feature_requirements(),
            #[cfg(feature = "highs")]
            Self::Highs(settings) => settings.ignore_feature_requirements(),
            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => settings.ignore_feature_requirements(),
        }
    }
}

#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
impl SolverConfig for BuiltInSolverConfig {
    type Solver = BuiltInSolver;

    fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => settings.name(),
            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => settings.name(),
            #[cfg(feature = "highs")]
            Self::Highs(settings) => settings.name(),
            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => settings.name(),
        }
    }

    fn features(&self) -> &'static [SolverFeatures] {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => settings.features(),
            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => settings.features(),
            #[cfg(feature = "highs")]
            Self::Highs(settings) => settings.features(),
            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => settings.features(),
        }
    }

    fn setup(&self, network: &Network, values: &ConstParameterValues) -> Result<Box<Self::Solver>, SolverSetupError> {
        let solver = match self {
            #[cfg(feature = "clp")]
            Self::Clp(settings) => BuiltInSolver::Clp(*settings.setup(network, values)?),

            #[cfg(feature = "cbc")]
            Self::Cbc(settings) => BuiltInSolver::Cbc(*settings.setup(network, values)?),

            #[cfg(feature = "highs")]
            Self::Highs(settings) => BuiltInSolver::Highs(*settings.setup(network, values)?),

            #[cfg(feature = "microlp")]
            Self::MicroLp(settings) => BuiltInSolver::MicroLp(*settings.setup(network, values)?),
        };

        Ok(Box::new(solver))
    }
}

#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
pub enum BuiltInSolver {
    #[cfg(feature = "clp")]
    Clp(ClpSolver),

    #[cfg(feature = "cbc")]
    Cbc(CbcSolver),

    #[cfg(feature = "highs")]
    Highs(HighsSolver),

    #[cfg(feature = "microlp")]
    MicroLp(MicroLpSolver),
}

#[cfg(any(feature = "clp", feature = "cbc", feature = "highs", feature = "microlp"))]
impl Solver for BuiltInSolver {
    fn solve(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        state: &mut State,
    ) -> Result<SolverTimings, SolverSolveError> {
        match self {
            #[cfg(feature = "clp")]
            Self::Clp(solver) => solver.solve(network, timestep, state),
            #[cfg(feature = "cbc")]
            Self::Cbc(solver) => solver.solve(network, timestep, state),
            #[cfg(feature = "highs")]
            Self::Highs(solver) => solver.solve(network, timestep, state),
            #[cfg(feature = "microlp")]
            Self::MicroLp(solver) => solver.solve(network, timestep, state),
        }
    }
}

#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
pub enum BuiltInMultiStateSolverConfig {
    #[cfg(feature = "ipm-simd")]
    SimdIpm(SimdIpmSolverSettings),
    #[cfg(feature = "ipm-ocl")]
    ClIpmF64(ClIpmF64Settings),
    #[cfg(feature = "ipm-ocl")]
    ClIpmF32(ClIpmF32Settings),
}

#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
impl SolverSettings for BuiltInMultiStateSolverConfig {
    fn parallel(&self) -> bool {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => settings.parallel(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => settings.parallel(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => settings.parallel(),
        }
    }

    fn threads(&self) -> usize {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => settings.threads(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => settings.threads(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => settings.threads(),
        }
    }

    fn ignore_feature_requirements(&self) -> bool {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => settings.ignore_feature_requirements(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => settings.ignore_feature_requirements(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => settings.ignore_feature_requirements(),
        }
    }
}

#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
impl MultiStateSolverConfig for BuiltInMultiStateSolverConfig {
    type Solver = BuiltInMultiStateSolver;

    fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => settings.name(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => settings.name(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => settings.name(),
        }
    }

    fn features(&self) -> &'static [SolverFeatures] {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => settings.features(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => settings.features(),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => settings.features(),
        }
    }

    fn setup(&self, network: &Network, num_scenarios: usize) -> Result<Box<Self::Solver>, SolverSetupError> {
        let solver = match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(settings) => BuiltInMultiStateSolver::SimdIpm(*settings.setup(network, num_scenarios)?),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(settings) => BuiltInMultiStateSolver::ClIpmF64(*settings.setup(network, num_scenarios)?),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(settings) => BuiltInMultiStateSolver::ClIpmF32(*settings.setup(network, num_scenarios)?),
        };

        Ok(Box::new(solver))
    }
}

#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
pub enum BuiltInMultiStateSolver {
    #[cfg(feature = "ipm-simd")]
    SimdIpm(SimdIpmF64Solver),
    #[cfg(feature = "ipm-ocl")]
    ClIpmF64(ClIpmF64Solver),
    #[cfg(feature = "ipm-ocl")]
    ClIpmF32(ClIpmF32Solver),
}

#[cfg(any(feature = "ipm-ocl", feature = "ipm-simd"))]
impl MultiStateSolver for BuiltInMultiStateSolver {
    fn solve(
        &mut self,
        network: &Network,
        timestep: &Timestep,
        states: &mut [State],
    ) -> Result<SolverTimings, SolverSolveError> {
        match self {
            #[cfg(feature = "ipm-simd")]
            Self::SimdIpm(solver) => solver.solve(network, timestep, states),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF64(solver) => solver.solve(network, timestep, states),
            #[cfg(feature = "ipm-ocl")]
            Self::ClIpmF32(solver) => solver.solve(network, timestep, states),
        }
    }
}
