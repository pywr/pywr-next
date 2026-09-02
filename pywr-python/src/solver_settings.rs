//! Helper methods for creating solver settings from Python kwargs.
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyAnyMethods, PyDict};
use pyo3::{Bound, PyResult};
#[cfg(feature = "cbc")]
use pywr_core::solvers::{CbcSolverSettings, CbcSolverSettingsBuilder};
#[cfg(feature = "clp")]
use pywr_core::solvers::{ClpSolverSettings, ClpSolverSettingsBuilder};
#[cfg(feature = "highs")]
use pywr_core::solvers::{HighsSolverSettings, HighsSolverSettingsBuilder};
#[cfg(feature = "ipm-simd")]
use pywr_core::solvers::{SimdIpmSolverSettings, SimdIpmSolverSettingsBuilder};

/// Build CLP solver settings from Python kwargs.
#[cfg(feature = "cbc")]
pub fn build_cbc_settings_py(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<CbcSolverSettings> {
    let mut builder = CbcSolverSettingsBuilder::default();

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

/// Build CLP solver settings from Python kwargs.
#[cfg(feature = "clp")]
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

#[cfg(feature = "highs")]
pub fn build_highs_settings_py(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<HighsSolverSettings> {
    let mut builder = HighsSolverSettingsBuilder::default();

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

#[cfg(feature = "ipm-simd")]
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
