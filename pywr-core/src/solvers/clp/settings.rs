use crate::solvers::SolverSettings;
#[cfg(feature = "pyo3")]
use pyo3::{Bound, PyResult, exceptions::PyRuntimeError, prelude::PyAnyMethods, types::PyDict};

/// Settings for the OpenCL IPM solvers.
///
/// Create new settings using [`ClpSolverSettingsBuilder`] or use the default implementation;
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct ClpSolverSettings {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
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

    fn ignore_feature_requirements(&self) -> bool {
        self.ignore_feature_requirements
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
/// builder = builder.parallel();
/// let settings = builder.build();
///
/// ```
#[derive(Default)]
pub struct ClpSolverSettingsBuilder {
    parallel: bool,
    threads: usize,
    ignore_feature_requirements: bool,
}

impl ClpSolverSettingsBuilder {
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
    pub fn build(self) -> ClpSolverSettings {
        ClpSolverSettings {
            parallel: self.parallel,
            threads: self.threads,
            ignore_feature_requirements: self.ignore_feature_requirements,
        }
    }
}

#[cfg(feature = "pyo3")]
/// Build CLP solver settings from Python kwargs.
pub fn build_clp_settings_py(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<ClpSolverSettings> {
    let mut builder = ClpSolverSettingsBuilder::default();

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
    use super::{ClpSolverSettings, ClpSolverSettingsBuilder};

    #[test]
    fn builder_test() {
        let _settings = ClpSolverSettings {
            parallel: true,
            threads: 0,
            ignore_feature_requirements: false,
        };
        let settings_from_builder = ClpSolverSettingsBuilder::default().parallel().build();

        assert_eq!(settings_from_builder, settings_from_builder);
    }
}
