use crate::solvers::SolverSettings;

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`HighsSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct HighsSolverSettings {
    parallel: bool,
    threads: usize,
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
/// use pywr::solvers::ClpSolverSettingsBuilder;
/// // Settings with parallel enabled and 4 threads.
/// let settings = ClpSolverSettingsBuilder::default().parallel().threads(4).build();
///
/// let mut builder = ClpSolverSettingsBuilder::default();
/// builder.chunk_size(NonZeroUsize::new(1024).unwrap());
/// let settings = builder.build();
///
/// builder.parallel();
/// let settings = builder.build();
///
/// ```
pub struct HighsSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
}

impl Default for HighsSolverSettingsBuilder {
    fn default() -> Self {
        Self {
            parallel: false,
            threads: 0,
        }
    }
}

impl HighsSolverSettingsBuilder {
    pub fn parallel(&mut self) -> &mut Self {
        self.parallel = true;
        self
    }

    pub fn threads(&mut self, threads: usize) -> &mut Self {
        self.threads = threads;
        self
    }

    /// Construct a [`HighsSolverSettings`] from the builder.
    pub fn build(&self) -> HighsSolverSettings {
        HighsSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
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
        };
        let settings_from_builder = HighsSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings_from_builder);
    }
}
