use crate::metric::{IndexMetric, Metric};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

/// A trait for recursively visiting [`Metric`] in a schema.
///
/// This trait is used to visit all the metrics in a schema. This is useful for search for
/// specific metrics, parameters, or other values in a schema.
///
/// This trait is implemented for all the types that can be used in a schema. Additional
/// implementations can be added as needed.
pub trait VisitMetrics {
    fn visit_metrics<F: FnMut(&Metric)>(&self, _visitor: &mut F) {}

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, _visitor: &mut F) {}
}

impl VisitMetrics for Metric {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        visitor(self);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        visitor(self);
    }
}

impl VisitMetrics for IndexMetric {
    fn visit_metrics<F: FnMut(&Metric)>(&self, _visitor: &mut F) {}

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, _visitor: &mut F) {}
}

impl<T> VisitMetrics for Option<T>
where
    T: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_metrics(visitor);
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_metrics_mut(visitor);
        }
    }
}

impl<T> VisitMetrics for Vec<T>
where
    T: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_metrics(visitor);
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_metrics_mut(visitor);
        }
    }
}

impl<A, B> VisitMetrics for (A, B)
where
    A: VisitMetrics,
    B: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        self.0.visit_metrics(visitor);
        self.1.visit_metrics(visitor);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        self.0.visit_metrics_mut(visitor);
        self.1.visit_metrics_mut(visitor);
    }
}

/// Visit all the metrics in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitMetrics for HashMap<K, V>
where
    V: VisitMetrics,
{
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_metrics(visitor);
        }
    }

    /// Mutably visit all the paths in the map.
    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_metrics_mut(visitor);
        }
    }
}

impl VisitMetrics for u8 {}
impl VisitMetrics for u16 {}
impl VisitMetrics for u32 {}
impl VisitMetrics for i32 {}
impl VisitMetrics for chrono::Month {}
impl VisitMetrics for f32 {}
impl VisitMetrics for f64 {}
impl<const N: usize> VisitMetrics for [f64; N] {}
impl<const N: usize> VisitMetrics for [Metric; N] {}
impl VisitMetrics for bool {}
impl VisitMetrics for u64 {}
impl VisitMetrics for String {}
impl VisitMetrics for PathBuf {}
impl VisitMetrics for NonZeroUsize {}

impl VisitMetrics for serde_json::Value {}

/// A trait for recursively visiting paths in a schema.
///
/// This trait is used to visit all the paths in a schema. This is useful for finding
/// all the external files that need to be loaded.
///
/// This trait is implemented for all the types that can be used in a schema. Additional
/// implementations can be added as needed.
pub trait VisitPaths {
    fn visit_paths<F: FnMut(&Path)>(&self, _visitor: &mut F) {}

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, _visitor: &mut F) {}
}

impl VisitPaths for Metric {}
impl VisitPaths for IndexMetric {}

impl<T> VisitPaths for Option<T>
where
    T: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_paths(visitor);
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        if let Some(inner) = self {
            inner.visit_paths_mut(visitor);
        }
    }
}

impl<T> VisitPaths for Vec<T>
where
    T: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        for item in self {
            item.visit_paths(visitor);
        }
    }

    /// Visit all the paths in the vector.
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for item in self {
            item.visit_paths_mut(visitor);
        }
    }
}

/// Visit all the paths in a [`HashMap`]'s values.
///
/// Note this does *not* visit the keys of the map.
impl<K, V> VisitPaths for HashMap<K, V>
where
    V: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        for value in self.values() {
            value.visit_paths(visitor);
        }
    }

    /// Mutably visit all the paths in the map.
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for value in self.values_mut() {
            value.visit_paths_mut(visitor);
        }
    }
}

impl<A, B> VisitPaths for (A, B)
where
    A: VisitPaths,
    B: VisitPaths,
{
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        self.0.visit_paths(visitor);
        self.1.visit_paths(visitor);
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        self.0.visit_paths_mut(visitor);
        self.1.visit_paths_mut(visitor);
    }
}

impl VisitPaths for u8 {}
impl VisitPaths for u16 {}
impl VisitPaths for u32 {}
impl VisitPaths for i32 {}
impl VisitPaths for chrono::Month {}
impl VisitPaths for f32 {}
impl VisitPaths for f64 {}
impl<const N: usize> VisitPaths for [f64; N] {}
impl<const N: usize> VisitPaths for [Metric; N] {}
impl VisitPaths for bool {}
impl VisitPaths for u64 {}
impl VisitPaths for String {}
impl VisitPaths for PathBuf {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        visitor(self.as_path());
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        visitor(self);
    }
}
impl VisitPaths for NonZeroUsize {}

impl VisitPaths for serde_json::Value {}
