use crate::solvers::SolverSettings;
use ipm_simd::Tolerances;
use std::num::NonZeroUsize;
use std::simd::{LaneCount, Simd, SimdElement, SupportedLaneCount};

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`SimdIpmSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug)]
pub struct SimdIpmSolverSettings<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    parallel: bool,
    threads: usize,
    tolerances: Tolerances<T, N>,
    max_iterations: NonZeroUsize,
}

// Default implementation is a convenience that defers to the builder.
impl<T, const N: usize> Default for SimdIpmSolverSettings<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    fn default() -> Self {
        SimdIpmSolverSettingsBuilder::default().build()
    }
}

impl<T, const N: usize> SolverSettings for SimdIpmSolverSettings<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    fn parallel(&self) -> bool {
        self.parallel
    }

    fn threads(&self) -> usize {
        self.threads
    }
}

impl<T, const N: usize> SimdIpmSolverSettings<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    /// Create a new builder for the settings
    pub fn builder() -> SimdIpmSolverSettingsBuilder<T, N> {
        SimdIpmSolverSettingsBuilder::default()
    }

    pub fn tolerances(&self) -> Tolerances<T, N> {
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
/// let settings: SimdIpmSolverSettings<f64, 4> = SimdIpmSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = SimdIpmSolverSettingsBuilder::default();
/// builder = builder.max_iterations(NonZero::new(50).unwrap());
/// let settings: SimdIpmSolverSettings<f64, 4> = builder.build();
///
/// let mut builder = SimdIpmSolverSettingsBuilder::default();
/// builder = builder.max_iterations(NonZero::new(50).unwrap());
/// builder = builder.parallel();
/// let settings: SimdIpmSolverSettings<f64, 4> = builder.build();
///
/// ```
pub struct SimdIpmSolverSettingsBuilder<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    parallel: bool,
    threads: usize,
    tolerances: Tolerances<T, N>,
    max_iterations: NonZeroUsize,
}

impl<T, const N: usize> Default for SimdIpmSolverSettingsBuilder<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
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

impl<T, const N: usize> SimdIpmSolverSettingsBuilder<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    pub fn primal_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.primal_feasibility = Simd::<T, N>::splat(tolerance.into());
        self
    }

    pub fn dual_feasibility(mut self, tolerance: f64) -> Self {
        self.tolerances.dual_feasibility = Simd::<T, N>::splat(tolerance.into());
        self
    }

    pub fn optimality(mut self, tolerance: f64) -> Self {
        self.tolerances.optimality = Simd::<T, N>::splat(tolerance.into());
        self
    }

    pub fn max_iterations(mut self, max_iterations: NonZeroUsize) -> Self {
        self.max_iterations = max_iterations;
        self
    }
    /// Construct a [`SimdIpmSolverSettings`] from the builder.
    pub fn build(self) -> SimdIpmSolverSettings<T, N> {
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
        let settings = SimdIpmSolverSettings::<f64, 4> {
            parallel: true,
            threads: 0,
            tolerances: Tolerances::default(),
            max_iterations: NonZeroUsize::new(200).unwrap(),
        };
        let settings_from_builder = SimdIpmSolverSettingsBuilder::<f64, 4>::default().parallel().build();

        assert_eq!(settings_from_builder, settings);
    }
}
