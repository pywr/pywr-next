use crate::solvers::SolverSettings;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`HighsSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct HighsSolverSettings {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
}

// Default implementation is a convenience that defers to the builder.
impl Default for HighsSolverSettings {
    fn default() -> Self {
        HighsSolverSettingsBuilder::default().build()
    }
}

impl SolverSettings for HighsSolverSettings {
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

impl HighsSolverSettings {
    /// Create a new builder for the settings
    pub fn builder() -> HighsSolverSettingsBuilder {
        HighsSolverSettingsBuilder::default()
    }
}

/// Builder for [`HighsSolverSettings`].
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use pywr_core::solvers::HighsSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = HighsSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = HighsSolverSettingsBuilder::default();
/// builder = builder.parallel();
/// let settings = builder.build();
///
/// ```
#[derive(Default)]
pub struct HighsSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
}

impl HighsSolverSettingsBuilder {
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

    /// Construct a [`HighsSolverSettings`] from the builder.
    pub fn build(self) -> HighsSolverSettings {
        HighsSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            ignore_feature_requirements: self.ignore_feature_requirements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HighsSolverSettings, HighsSolverSettingsBuilder};

    #[test]
    fn builder_test() {
        let settings = HighsSolverSettings {
            parallel: true,
            threads: 0,
            ignore_feature_requirements: false,
        };
        let settings_from_builder = HighsSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings);
    }
}
