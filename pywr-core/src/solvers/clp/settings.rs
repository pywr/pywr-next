use crate::solvers::SolverSettings;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`ClpSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct ClpSolverSettings {
    parallel: bool,
    threads: usize,
}

// Default implementation is a convenience that defers to the builder.
impl Default for ClpSolverSettings {
    fn default() -> Self {
        ClpSolverSettingsBuilder::default().build()
    }
}

impl SolverSettings for ClpSolverSettings {
    fn parallel(&self) -> bool {
        self.parallel
    }

    fn threads(&self) -> usize {
        self.threads
    }
}

impl ClpSolverSettings {
    /// Create a new builder for the settings
    pub fn builder() -> ClpSolverSettingsBuilder {
        ClpSolverSettingsBuilder::default()
    }
}

/// Builder for [`ClpSolverSettings`].
///
/// # Examples
///
/// ```
/// use std::num::NonZeroUsize;
/// use pywr_core::solvers::ClpSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = ClpSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = ClpSolverSettingsBuilder::default();
///
/// builder.parallel();
/// let settings = builder.build();
///
/// ```
#[derive(Default)]
pub struct ClpSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
}

impl ClpSolverSettingsBuilder {
    pub fn parallel(&mut self) -> &mut Self {
        self.parallel = true;
        self
    }

    pub fn threads(&mut self, threads: usize) -> &mut Self {
        self.threads = threads;
        self
    }

    /// Construct a [`ClpSolverSettings`] from the builder.
    pub fn build(&self) -> ClpSolverSettings {
        ClpSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClpSolverSettings, ClpSolverSettingsBuilder};

    #[test]
    fn builder_test() {
        let _settings = ClpSolverSettings {
            parallel: true,
            threads: 0,
        };
        let settings_from_builder = ClpSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings_from_builder);
    }
}
