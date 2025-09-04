use crate::solvers::SolverSettings;

#[derive(PartialEq, Debug, Copy, Clone)]
pub struct MicroLpSolverSettings {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
}

// Default implementation is a convenience that defers to the builder.
impl Default for MicroLpSolverSettings {
    fn default() -> Self {
        MicroLpSolverSettingsBuilder::default().build()
    }
}

impl SolverSettings for MicroLpSolverSettings {
    fn parallel(&self) -> bool {
        false
    }

    fn threads(&self) -> usize {
        1
    }

    fn ignore_feature_requirements(&self) -> bool {
        false
    }
}

/// Builder for [`MicroLpSolverSettings`].
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use pywr_core::solvers::MicroLpSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = MicroLpSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = MicroLpSolverSettingsBuilder::default();
///
/// builder = builder.parallel();
/// let settings = builder.build();
///
/// ```
#[derive(Default)]
pub struct MicroLpSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
}

impl crate::solvers::MicroLpSolverSettingsBuilder {
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    pub fn ignore_feature_requirements(mut self) -> Self {
        self.ignore_feature_requirements = true;
        self
    }

    /// Construct a [`ClpSolverSettings`] from the builder.
    pub fn build(self) -> MicroLpSolverSettings {
        MicroLpSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            ignore_feature_requirements: self.ignore_feature_requirements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MicroLpSolverSettings, MicroLpSolverSettingsBuilder};

    #[test]
    fn builder_test() {
        let _settings = MicroLpSolverSettings {
            parallel: true,
            threads: 0,
            ignore_feature_requirements: false,
        };
        let settings_from_builder = MicroLpSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings_from_builder);
    }
}
