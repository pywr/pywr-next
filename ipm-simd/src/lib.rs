#![feature(portable_simd)]

mod common;
mod path_following_direct;

use crate::path_following_direct::{normal_eqn_init, normal_eqn_step};
use common::{dual_feasibility, primal_feasibility, Matrix};
use ipm_common::SparseNormalCholeskyIndices;
use nalgebra_sparse::CsrMatrix;
use path_following_direct::ANormIndices;
use path_following_direct::LDecompositionIndices;
use path_following_direct::{LIndices, LTIndices};
use std::f64;
use std::fmt::Debug;
use std::iter::Sum;
use std::num::NonZeroUsize;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};
use std::simd::{
    LaneCount, Mask, Simd, SimdElement, SimdFloat, SimdPartialEq, SimdPartialOrd, StdFloat, SupportedLaneCount,
};

struct PathData<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    x: Vec<Simd<T, N>>,
    z: Vec<Simd<T, N>>,
    y: Vec<Simd<T, N>>,
    w: Vec<Simd<T, N>>,
}

impl<T, const N: usize> PathData<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    pub fn new(num_rows: usize, num_cols: usize, num_inequality_constraints: usize) -> Self {
        Self {
            x: (0..num_cols)
                .into_iter()
                .map(|_| Simd::<T, N>::splat(0.0.into()))
                .collect(),
            z: (0..num_cols)
                .into_iter()
                .map(|_| Simd::<T, N>::splat(0.0.into()))
                .collect(),
            y: (0..num_rows)
                .into_iter()
                .map(|_| Simd::<T, N>::splat(0.0.into()))
                .collect(),
            w: (0..num_inequality_constraints)
                .into_iter()
                .map(|_| Simd::<T, N>::splat(0.0.into()))
                .collect(),
        }
    }
}

pub struct PathFollowingDirectSimdData<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    a: Matrix<T, N>,
    at: Matrix<T, N>,
    a_norm_ptr: ANormIndices,
    l_decomp_ptr: LDecompositionIndices,
    l_ptr: LIndices,
    lt_ptr: LTIndices,
    l_data: Vec<Simd<T, N>>,

    path_buffers: PathData<T, N>,
    delta_path_buffers: PathData<T, N>,

    tmp: Vec<Simd<T, N>>,
    rhs: Vec<Simd<T, N>>,
}

impl<T, const N: usize> PathFollowingDirectSimdData<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    pub fn from_data(a: &CsrMatrix<f64>, num_inequality_constraints: usize) -> Self {
        let num_rows = a.nrows();
        let num_cols = a.ncols();

        let a_buffers = Matrix::from_sparse_matrix(a);
        let at = a.transpose();
        let at_buffers = Matrix::from_sparse_matrix(&at);

        let normal_indices = SparseNormalCholeskyIndices::from_matrix(a);

        let a_norm_ptr = ANormIndices::from_indices(&normal_indices);
        let l_decomp_ptr = LDecompositionIndices::from_indices(&normal_indices);
        let l_ptr = LIndices::from_indices(&normal_indices);
        let lt_ptr = LTIndices::from_indices(&normal_indices);

        // println!("anorm_indptr: {}", normal_indices.anorm_indptr.len());
        // println!("anorm_indptr_i: {}", normal_indices.anorm_indptr_i.len());
        // println!("anorm_indptr_j: {}", normal_indices.anorm_indptr_j.len());
        // println!("anorm_indices: {}", normal_indices.anorm_indices.len());
        // println!("ldecomp_indptr: {}", normal_indices.ldecomp_indptr.len());
        // println!("ldecomp_indptr_i: {}", normal_indices.ldecomp_indptr_i.len());
        // println!("ldecomp_indptr_j: {}", normal_indices.ldecomp_indptr_j.len());
        // println!("lindptr: {}", normal_indices.lindptr.len());
        // println!("ldiag_indptr: {}", normal_indices.ldiag_indptr.len());
        // println!("lindices: {}", normal_indices.lindices.len());
        // println!("ltindptr: {}", normal_indices.ltindptr.len());
        // println!("ltindices: {}", normal_indices.ltindices.len());
        // println!("ltmap: {}", normal_indices.ltmap.len());

        // Require ldata for every SIMD lane
        let l_data: Vec<Simd<T, N>> = (0..normal_indices.lindices.len())
            .into_iter()
            .map(|_| Simd::<T, N>::splat(0.0.into()))
            .collect();

        let path_buffers = PathData::new(num_rows, num_cols, num_inequality_constraints);
        let delta_path_buffers = PathData::new(num_rows, num_cols, num_inequality_constraints);

        // Work buffers
        let tmp = (0..num_cols)
            .into_iter()
            .map(|_| Simd::<T, N>::splat(0.0.into()))
            .collect();
        let rhs = (0..num_rows)
            .into_iter()
            .map(|_| Simd::<T, N>::splat(0.0.into()))
            .collect();

        Self {
            a: a_buffers,
            at: at_buffers,
            a_norm_ptr,
            l_decomp_ptr,
            l_ptr,
            lt_ptr,
            l_data,
            path_buffers,
            delta_path_buffers,
            tmp,
            rhs,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Tolerances<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    pub primal_feasibility: Simd<T, N>,
    pub dual_feasibility: Simd<T, N>,
    pub optimality: Simd<T, N>,
}

impl<T, const N: usize> Default for Tolerances<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    fn default() -> Self {
        Self {
            primal_feasibility: Simd::<T, N>::splat(1e-6.into()),
            dual_feasibility: Simd::<T, N>::splat(1e-6.into()),
            optimality: Simd::<T, N>::splat(1e-6.into()),
        }
    }
}

pub struct PathFollowingDirectSimdSolver<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    buffers: PathFollowingDirectSimdData<T, N>,
}

impl<T, const N: usize> PathFollowingDirectSimdSolver<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    pub fn from_data(
        num_rows: usize,
        num_cols: usize,
        row_offsets: Vec<usize>,
        col_indices: Vec<usize>,
        values: Vec<f64>,
        num_inequality_constraints: usize,
    ) -> Self {
        let a = CsrMatrix::try_from_csr_data(num_rows, num_cols, row_offsets, col_indices, values)
            .expect("Failed to create matrix from given data");

        let buffers = PathFollowingDirectSimdData::from_data(&a, num_inequality_constraints);

        Self { buffers }
    }

    pub fn solve(
        &mut self,
        b: &[Simd<T, N>],
        c: &[Simd<T, N>],
        tolerances: &Tolerances<T, N>,
        max_iterations: NonZeroUsize,
    ) -> &[Simd<T, N>]
    where
        LaneCount<N>: SupportedLaneCount,
        T: SimdElement<Mask = i64> + From<f64> + Debug,
        Simd<T, N>: AddAssign
            + Sum
            + StdFloat
            + SimdFloat<Mask = Mask<i64, N>>
            + SimdPartialOrd
            + SimdPartialEq<Mask = Mask<i64, N>>
            + Mul<Output = Simd<T, N>>
            + Add<Output = Simd<T, N>>
            + Sub<Output = Simd<T, N>>
            + Div<Output = Simd<T, N>>
            + Neg<Output = Simd<T, N>>,
    {
        normal_eqn_init(
            &mut self.buffers.path_buffers.x,
            &mut self.buffers.path_buffers.z,
            &mut self.buffers.path_buffers.y,
            &mut self.buffers.path_buffers.w,
        );

        let delta = Simd::<T, N>::splat(0.1.into());
        let mut iter = 0;

        let last_iteration = loop {
            if iter >= max_iterations.get() {
                break None;
            }
            let status = normal_eqn_step(
                &self.buffers.a,
                &self.buffers.at,
                &self.buffers.a_norm_ptr,
                &self.buffers.l_decomp_ptr,
                &self.buffers.l_ptr,
                &self.buffers.lt_ptr,
                &mut self.buffers.l_data,
                &mut self.buffers.path_buffers.x,
                &mut self.buffers.path_buffers.z,
                &mut self.buffers.path_buffers.y,
                &mut self.buffers.path_buffers.w,
                &b,
                &c,
                delta,
                &mut self.buffers.delta_path_buffers.x,
                &mut self.buffers.delta_path_buffers.z,
                &mut self.buffers.delta_path_buffers.y,
                &mut self.buffers.delta_path_buffers.w,
                &mut self.buffers.tmp,
                &mut self.buffers.rhs,
                tolerances,
            );

            if status.all() {
                break Some(iter);
            }

            iter += 1
        };

        if last_iteration.is_none() {
            panic!("Interior point method failed to converged all SIMD lanes.")
        }

        // println!("Finished after iterations: {}", last_iteration);
        // println!("x: {:#?}", self.buffers.path_buffers.x);
        self.buffers.path_buffers.x.as_slice()
    }
}
