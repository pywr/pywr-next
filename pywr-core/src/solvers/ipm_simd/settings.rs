use crate::solvers::SolverSettings;
use ipm_simd::Tolerances;
#[cfg(feature = "pyo3")]
use pyo3::{Bound, PyResult, exceptions::PyRuntimeError, prelude::PyAnyMethods, types::PyDict};
use std::num::NonZeroUsize;
use wide::f64x4;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`SimdIpmSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug)]
pub struct SimdIpmSolverSettings {
    parallel: bool,
    threads: usize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
    ignore_feature_requirements: bool,
}

// Default implementation is a convenience that defers to the builder.
impl Default for SimdIpmSolverSettings {
    fn default() -> Self {
        SimdIpmSolverSettingsBuilder::default().build()
    }
}

impl SolverSettings for SimdIpmSolverSettings {
    fn parallel(&self) -> bool {
        self.parallel
    }

    fn threads(&self) -> usize {
        self.threads
    }

    fn ignore_feature_requirements(&self) -> bool {
        self.ignore_feature_requirements
    }
}

impl SimdIpmSolverSettings {
    /// Create a new builder for the settings
    pub fn builder() -> SimdIpmSolverSettingsBuilder {
        SimdIpmSolverSettingsBuilder::default()
    }

    pub fn tolerances(&self) -> Tolerances {
        self.tolerances
    }

    pub fn max_iterations(&self) -> NonZeroUsize {
        self.max_iterations
    }
}

/// Builder for [`SimdIpmSolverSettings`].
///
/// # Examples
///
/// ```
/// use std::num::NonZero;
/// use pywr_core::solvers::{SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
/// // Settings with parallel enabled and 4 threads.
/// let settings = SimdIpmSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = SimdIpmSolverSettingsBuilder::default();
/// builder = builder.max_iterations(NonZero::new(50).unwrap());
/// let settings = builder.build();
///
/// let mut builder = SimdIpmSolverSettingsBuilder::default();
/// builder = builder.max_iterations(NonZero::new(50).unwrap());
/// builder = builder.parallel();
/// let settings = builder.build();
///
/// ```
pub struct SimdIpmSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
    ignore_feature_requirements: bool,
}

impl Default for SimdIpmSolverSettingsBuilder {
    fn default() -> Self {
        Self {
            parallel: false,
            threads: 0,
            tolerances: Tolerances::default(),
            // Unwrap is safe as the value is non-zero!
            max_iterations: NonZeroUsize::new(200).unwrap(),
            ignore_feature_requirements: false,
        }
    }
}

impl SimdIpmSolverSettingsBuilder {
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    pub fn primal_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.primal_feasibility = f64x4::splat(tolerance);
        self
    }

    pub fn dual_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.dual_feasibility = f64x4::splat(tolerance);
        self
    }

    pub fn optimality(mut self, tolerance: f64) -> Self {
        self.tolerances.optimality = f64x4::splat(tolerance);
        self
    }

    pub fn max_iterations(mut self, max_iterations: NonZeroUsize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    pub fn ignore_feature_requirements(mut self) -> Self {
        self.ignore_feature_requirements = true;
        self
    }

    /// Construct a [`SimdIpmSolverSettings`] from the builder.
    pub fn build(self) -> SimdIpmSolverSettings {
        SimdIpmSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            tolerances: self.tolerances,
            max_iterations: self.max_iterations,
            ignore_feature_requirements: self.ignore_feature_requirements,
        }
    }
}

#[cfg(feature = "pyo3")]
pub fn build_ipm_simd_settings_py(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<SimdIpmSolverSettings> {
    let mut builder = SimdIpmSolverSettingsBuilder::default();

    if let Some(kwargs) = kwargs {
        if let Ok(threads) = kwargs.get_item("threads") {
            builder = builder.threads(threads.extract::<usize>()?);

            kwargs.del_item("threads")?;
        }

        if let Ok(parallel) = kwargs.get_item("parallel") {
            if parallel.extract::<bool>()? {
                builder = builder.parallel();
            }

            kwargs.del_item("parallel")?;
        }

        if let Ok(ignore) = kwargs.get_item("ignore_feature_requirements") {
            if ignore.extract::<bool>()? {
                builder = builder.ignore_feature_requirements();
            }

            kwargs.del_item("ignore_feature_requirements")?;
        }

        if !kwargs.is_empty()? {
            return Err(PyRuntimeError::new_err(format!(
                "Unknown keyword arguments: {kwargs:?}",
            )));
        }
    }

    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::{SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};
    use ipm_simd::Tolerances;
    use std::num::NonZeroUsize;

    #[test]
    fn builder_test() {
        let settings = SimdIpmSolverSettings {
            parallel: true,
            threads: 0,
            tolerances: Tolerances::default(),
            max_iterations: NonZeroUsize::new(200).unwrap(),
            ignore_feature_requirements: false,
        };
        let settings_from_builder = SimdIpmSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings);
    }
}
