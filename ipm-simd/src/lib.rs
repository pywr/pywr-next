mod common;
mod path_following_direct;

use crate::path_following_direct::{normal_eqn_init, normal_eqn_step};
use common::{Matrix, dual_feasibility, primal_feasibility};
use fearless_simd::{Simd, SimdBase, SimdMask};
use ipm_common::SparseNormalCholeskyIndices;
use nalgebra_sparse::CsrMatrix;
use path_following_direct::ANormIndices;
use path_following_direct::LDecompositionIndices;
use path_following_direct::{LIndices, LTIndices};
use std::f64;
use std::fmt::Debug;
use std::num::NonZeroUsize;

struct PathData<S: Simd> {
    x: Vec<S::f64s>,
    z: Vec<S::f64s>,
    y: Vec<S::f64s>,
    w: Vec<S::f64s>,
}

impl<S> PathData<S>
where
    S: Simd,
{
    pub fn new(simd: S, num_rows: usize, num_cols: usize, num_inequality_constraints: usize) -> Self {
        Self {
            x: (0..num_cols).map(|_| S::f64s::splat(simd, 0.0)).collect(),
            z: (0..num_cols).map(|_| S::f64s::splat(simd, 0.0)).collect(),
            y: (0..num_rows).map(|_| S::f64s::splat(simd, 0.0)).collect(),
            w: (0..num_inequality_constraints)
                .map(|_| S::f64s::splat(simd, 0.0))
                .collect(),
        }
    }
}

pub struct PathFollowingDirectSimdData<S: Simd> {
    a: Matrix<S>,
    at: Matrix<S>,
    a_norm_ptr: ANormIndices,
    l_decomp_ptr: LDecompositionIndices,
    l_ptr: LIndices,
    lt_ptr: LTIndices,
    l_data: Vec<S::f64s>,

    path_buffers: PathData<S>,
    delta_path_buffers: PathData<S>,

    tmp: Vec<S::f64s>,
    rhs: Vec<S::f64s>,
}

impl<S> PathFollowingDirectSimdData<S>
where
    S: Simd,
{
    pub fn from_data(simd: S, a: &CsrMatrix<f64>, num_inequality_constraints: usize) -> Self {
        let num_rows = a.nrows();
        let num_cols = a.ncols();

        let a_buffers = Matrix::from_sparse_matrix(simd, a);
        let at = a.transpose();
        let at_buffers = Matrix::from_sparse_matrix(simd, &at);

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
        let l_data: Vec<_> = (0..normal_indices.lindices.len())
            .map(|_| S::f64s::splat(simd, 0.0))
            .collect();

        let path_buffers = PathData::new(simd, num_rows, num_cols, num_inequality_constraints);
        let delta_path_buffers = PathData::new(simd, num_rows, num_cols, num_inequality_constraints);

        // Work buffers
        let tmp = (0..num_cols).map(|_| S::f64s::splat(simd, 0.0)).collect();
        let rhs = (0..num_rows).map(|_| S::f64s::splat(simd, 0.0)).collect();

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
pub struct Tolerances {
    pub primal_feasibility: f64,
    pub dual_feasibility: f64,
    pub optimality: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            primal_feasibility: 1e-8,
            dual_feasibility: 1e-8,
            optimality: 1e-8,
        }
    }
}

pub struct PathFollowingDirectSimdSolver<S: Simd> {
    buffers: PathFollowingDirectSimdData<S>,
}

impl<S> PathFollowingDirectSimdSolver<S>
where
    S: Simd,
{
    pub fn from_data(
        simd: S,
        num_rows: usize,
        num_cols: usize,
        row_offsets: Vec<usize>,
        col_indices: Vec<usize>,
        values: Vec<f64>,
        num_inequality_constraints: usize,
    ) -> Self {
        let a = CsrMatrix::try_from_csr_data(num_rows, num_cols, row_offsets, col_indices, values)
            .expect("Failed to create matrix from given data");

        let buffers = PathFollowingDirectSimdData::from_data(simd, &a, num_inequality_constraints);

        Self { buffers }
    }

    pub fn solve(
        &mut self,
        simd: S,
        b: &[S::f64s],
        c: &[S::f64s],
        tolerances: &Tolerances,
        max_iterations: NonZeroUsize,
    ) -> &[S::f64s] {
        normal_eqn_init(
            simd,
            &mut self.buffers.path_buffers.x,
            &mut self.buffers.path_buffers.z,
            &mut self.buffers.path_buffers.y,
            &mut self.buffers.path_buffers.w,
        );

        let delta = S::f64s::splat(simd, 0.1);
        let mut iter = 0;

        let last_iteration = loop {
            if iter >= max_iterations.get() {
                break None;
            }
            let status = normal_eqn_step(
                simd,
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
                b,
                c,
                delta,
                &mut self.buffers.delta_path_buffers.x,
                &mut self.buffers.delta_path_buffers.z,
                &mut self.buffers.delta_path_buffers.y,
                &mut self.buffers.delta_path_buffers.w,
                &mut self.buffers.tmp,
                &mut self.buffers.rhs,
                tolerances,
            );

            if status.all_true() {
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
