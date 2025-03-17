use crate::solvers::SolverSettings;
use ipm_ocl::Tolerances;
use std::num::NonZeroUsize;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`ClIpmSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug)]
pub struct ClIpmSolverSettings {
    parallel: bool,
    threads: usize,
    num_chunks: NonZeroUsize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
    ignore_feature_requirements: bool,
}

// Default implementation is a convenience that defers to the builder.
impl Default for ClIpmSolverSettings {
    fn default() -> Self {
        ClIpmSolverSettingsBuilder::default().build()
    }
}

impl SolverSettings for ClIpmSolverSettings {
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

impl ClIpmSolverSettings {
    /// Create a new builder for the settings
    pub fn builder() -> ClIpmSolverSettingsBuilder {
        ClIpmSolverSettingsBuilder::default()
    }

    pub fn num_chunks(&self) -> NonZeroUsize {
        self.num_chunks
    }

    pub fn tolerances(&self) -> Tolerances {
        self.tolerances
    }

    pub fn max_iterations(&self) -> NonZeroUsize {
        self.max_iterations
    }
}

/// Builder for [`ClIpmSolverSettings`].
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use pywr_core::solvers::ClIpmSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = ClIpmSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let builder = ClIpmSolverSettingsBuilder::default()
///     .num_chunks(NonZeroUsize::new(8).unwrap());
/// let settings = builder.build();
///
/// let builder = ClIpmSolverSettingsBuilder::default()
///     .parallel();
/// let settings = builder.build();
///
/// ```
pub struct ClIpmSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    num_chunks: NonZeroUsize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
    ignore_feature_requirements: bool,
}

impl Default for ClIpmSolverSettingsBuilder {
    fn default() -> Self {
        Self {
            parallel: false,
            threads: 0,
            // Unwrap is safe as the value is non-zero!
            num_chunks: NonZeroUsize::new(4).unwrap(),
            tolerances: Tolerances::default(),
            max_iterations: NonZeroUsize::new(200).unwrap(),
            ignore_feature_requirements: false,
        }
    }
}

impl ClIpmSolverSettingsBuilder {
    pub fn num_chunks(mut self, num_chunks: NonZeroUsize) -> Self {
        self.num_chunks = num_chunks;
        self
    }

    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    pub fn primal_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.primal_feasibility = tolerance;
        self
    }

    pub fn dual_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.dual_feasibility = tolerance;
        self
    }

    pub fn optimality(mut self, tolerance: f64) -> Self {
        self.tolerances.optimality = tolerance;
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

    /// Construct a [`ClIpmSolverSettings`] from the builder.
    pub fn build(self) -> ClIpmSolverSettings {
        ClIpmSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            num_chunks: self.num_chunks,
            tolerances: self.tolerances,
            max_iterations: self.max_iterations,
            ignore_feature_requirements: self.ignore_feature_requirements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClIpmSolverSettings, ClIpmSolverSettingsBuilder};
    use ipm_ocl::Tolerances;
    use std::num::NonZeroUsize;

    #[test]
    fn builder_test() {
        let settings = ClIpmSolverSettings {
            parallel: true,
            threads: 0,
            num_chunks: NonZeroUsize::new(4).unwrap(),
            max_iterations: NonZeroUsize::new(200).unwrap(),
            tolerances: Tolerances::default(),
            ignore_feature_requirements: false,
        };
        let settings_from_builder = ClIpmSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings, settings_from_builder);
    }
}
