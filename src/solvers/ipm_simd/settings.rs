use crate::solvers::SolverSettings;
use ipm_simd::Tolerances;
use std::num::NonZeroUsize;
use std::simd::f64x4;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`SimdIpmSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug)]
pub struct SimdIpmSolverSettings {
    parallel: bool,
    threads: usize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
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
/// use std::num::NonZeroUsize;
/// use pywr::solvers::SimdIpmSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = SimdIpmSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = SimdIpmSolverSettingsBuilder::default();
/// builder.max_iterations(NonZeroUsize(50).unwrap());
/// let settings = builder.build();
///
/// builder.parallel();
/// let settings = builder.build();
///
/// ```
pub struct SimdIpmSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    tolerances: Tolerances,
    max_iterations: NonZeroUsize,
}

impl Default for SimdIpmSolverSettingsBuilder {
    fn default() -> Self {
        Self {
            parallel: false,
            threads: 0,
            tolerances: Tolerances::default(),
            // Unwrap is safe as the value is non-zero!
            max_iterations: NonZeroUsize::new(200).unwrap(),
        }
    }
}

impl SimdIpmSolverSettingsBuilder {
    pub fn parallel(&mut self) -> &mut Self {
        self.parallel = true;
        self
    }

    pub fn threads(&mut self, threads: usize) -> &mut Self {
        self.threads = threads;
        self
    }

    pub fn primal_feasibility(&mut self, tolerance: f64) -> &mut Self {
        self.tolerances.primal_feasibility = f64x4::splat(tolerance);
        self
    }

    pub fn dual_feasibility(&mut self, tolerance: f64) -> &mut Self {
        self.tolerances.dual_feasibility = f64x4::splat(tolerance);
        self
    }

    pub fn optimality(&mut self, tolerance: f64) -> &mut Self {
        self.tolerances.optimality = f64x4::splat(tolerance);
        self
    }

    pub fn max_iterations(&mut self, max_iterations: NonZeroUsize) -> &mut Self {
        self.max_iterations = max_iterations;
        self
    }
    /// Construct a [`SimdIpmSolverSettings`] from the builder.
    pub fn build(&self) -> SimdIpmSolverSettings {
        // Create a Foo from the FooBuilder, applying all settings in FooBuilder
        // to Foo.
        SimdIpmSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            tolerances: self.tolerances,
            max_iterations: self.max_iterations,
        }
    }
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
        };
        let settings_from_builder = SimdIpmSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings_from_builder);
    }
}
