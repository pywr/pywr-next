use super::{dual_feasibility, primal_feasibility, Matrix};
use crate::common::{compute_dx_dz_dw, dot_product, normal_eqn_rhs, vector_norm, vector_set, vector_update};
use crate::Tolerances;
use ipm_common::SparseNormalCholeskyIndices;
use nalgebra_sparse::na::SimdBool;
use std::fmt::Debug;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};
use std::simd::{
    LaneCount, Mask, Simd, SimdElement, SimdFloat, SimdPartialEq, SimdPartialOrd, StdFloat, SupportedLaneCount,
};

pub struct ANormIndices {
    indptr: Vec<usize>,
    indptr_i: Vec<usize>,
    indptr_j: Vec<usize>,
    indices: Vec<usize>,
}

impl ANormIndices {
    pub fn from_indices(indices: &SparseNormalCholeskyIndices) -> Self {
        Self {
            indptr: indices.anorm_indptr.iter().map(|&i| i as usize).collect(),
            indptr_i: indices.anorm_indptr_i.iter().map(|&i| i as usize).collect(),
            indptr_j: indices.anorm_indptr_j.iter().map(|&i| i as usize).collect(),
            indices: indices.anorm_indices.iter().map(|&i| i as usize).collect(),
        }
    }
}

pub struct LDecompositionIndices {
    indptr: Vec<usize>,
    indptr_i: Vec<usize>,
    indptr_j: Vec<usize>,
}

impl LDecompositionIndices {
    pub fn from_indices(indices: &SparseNormalCholeskyIndices) -> Self {
        Self {
            indptr: indices.ldecomp_indptr.iter().map(|&i| i as usize).collect(),
            indptr_i: indices.ldecomp_indptr_i.iter().map(|&i| i as usize).collect(),
            indptr_j: indices.ldecomp_indptr_j.iter().map(|&i| i as usize).collect(),
        }
    }
}

pub struct LIndices {
    indptr: Vec<usize>,
    diag_indptr: Vec<usize>,
    indices: Vec<usize>,
}

impl LIndices {
    pub fn from_indices(indices: &SparseNormalCholeskyIndices) -> Self {
        Self {
            indptr: indices.lindptr.iter().map(|&i| i as usize).collect(),
            diag_indptr: indices.ldiag_indptr.iter().map(|&i| i as usize).collect(),
            indices: indices.lindices.iter().map(|&i| i as usize).collect(),
        }
    }
}

pub struct LTIndices {
    indptr: Vec<usize>,
    indices: Vec<usize>,
    map: Vec<usize>,
}

impl LTIndices {
    pub fn from_indices(indices: &SparseNormalCholeskyIndices) -> Self {
        Self {
            indptr: indices.ltindptr.iter().map(|&i| i as usize).collect(),
            indices: indices.ltindices.iter().map(|&i| i as usize).collect(),
            map: indices.ltmap.iter().map(|&i| i as usize).collect(),
        }
    }
}

/// Compute the Cholesky decomposition of the normal matrix
pub fn normal_matrix_cholesky_decomposition<T, const N: usize>(
    a: &Matrix<T, N>,
    a_norm_ptr: &ANormIndices,
    l_decomp_ptr: &LDecompositionIndices,
    x: &[Simd<T, N>],
    z: &[Simd<T, N>],
    y: &[Simd<T, N>],
    w: &[Simd<T, N>],
    l_ptr: &LIndices,
    l_data: &mut [Simd<T, N>],
) where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: AddAssign
        + Sum
        + StdFloat
        + SimdFloat
        + Mul<Output = Simd<T, N>>
        + Add<Output = Simd<T, N>>
        + Sub<Output = Simd<T, N>>
        + Div<Output = Simd<T, N>>
        + Neg<Output = Simd<T, N>>,
{
    let mut l_entry = 0;
    for row in 0..a.size {
        let row_ind_start = l_ptr.indptr[row];
        let row_ind_end = l_ptr.indptr[row + 1];

        // Iterate the columns of L
        for row_ind in row_ind_start..row_ind_end {
            let col = l_ptr.indices[row_ind];

            // Compute the normal equation element AAT[i, j]
            let mut val = if (row == col) && (row < w.len()) {
                w[row] / y[row]
            } else {
                Simd::<T, N>::splat(0.0.into())
            };

            let ind_start = a_norm_ptr.indptr[l_entry];
            let ind_end = a_norm_ptr.indptr[l_entry + 1];

            for ind in ind_start..ind_end {
                let xind = a_norm_ptr.indices[ind];
                val += a.data[a_norm_ptr.indptr_i[ind]] * a.data[a_norm_ptr.indptr_j[ind]] * x[xind] / z[xind];
            }
            // Now remove the previous L entries
            let ind_start = l_decomp_ptr.indptr[l_entry];
            let ind_end = l_decomp_ptr.indptr[l_entry + 1];

            for ind in ind_start..ind_end {
                val -= l_data[l_decomp_ptr.indptr_i[ind]] * l_data[l_decomp_ptr.indptr_j[ind]];
            }

            if row == col {
                val = val.abs().sqrt();
            } else {
                val = val / l_data[l_ptr.diag_indptr[col]];
            }
            l_data[l_entry] = val;
            l_entry += 1;
        }
    }
}

///  Solve a system Ax = b for x given the decomposition of A as L.
///
/// L is a lower triangular matrix. Entries are stored such that the lth
/// entry of L is the i(i + 1)/2 + j entry in dense i, j  coordinates.
///
fn cholesky_solve<T, const N: usize>(
    a_size: usize,
    l_ptr: &LIndices,
    lt_ptr: &LTIndices,
    l_data: &[Simd<T, N>],
    b: &[Simd<T, N>],
    x: &mut [Simd<T, N>],
) where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: AddAssign
        + Sum
        + StdFloat
        + SimdFloat
        + Mul<Output = Simd<T, N>>
        + Add<Output = Simd<T, N>>
        + Sub<Output = Simd<T, N>>
        + Div<Output = Simd<T, N>>
        + Neg<Output = Simd<T, N>>,
{
    // Forward substitution
    for i in 0..a_size {
        x[i] = b[i];

        let mut jk = l_ptr.indptr[i];
        let mut j = l_ptr.indices[jk];

        while j < i {
            x[i] -= x[j] * l_data[jk];
            jk += 1;
            j = l_ptr.indices[jk];
        }
        // jk should now point to the (i, i) entry.
        x[i] /= l_data[jk];
    }

    // Backward substitution
    for i in (0..a_size).rev() {
        // printf("%d %d\n", i, Asize);

        let mut jk = lt_ptr.indptr[i] + 1;
        let jkk = lt_ptr.indptr[i + 1];

        while jk < jkk {
            let j = lt_ptr.indices[jk];
            x[i] -= x[j] * l_data[lt_ptr.map[jk]];
            jk += 1;
        }

        jk = l_ptr.indptr[i + 1] - 1;
        x[i] /= l_data[jk];
    }
}

/// Perform a single step of the path-following algorithm.
pub fn normal_eqn_step<T, const N: usize>(
    a: &Matrix<T, N>,  // Sparse A matrix
    at: &Matrix<T, N>, // Sparse transpose of A matrix
    a_norm_ptr: &ANormIndices,
    l_decomp_ptr: &LDecompositionIndices,
    l_ptr: &LIndices,
    lt_ptr: &LTIndices,
    l_data: &mut [Simd<T, N>],
    x: &mut [Simd<T, N>],
    z: &mut [Simd<T, N>],
    y: &mut [Simd<T, N>],
    w: &mut [Simd<T, N>],
    b: &[Simd<T, N>],
    c: &[Simd<T, N>],
    delta: Simd<T, N>,
    dx: &mut [Simd<T, N>],
    dz: &mut [Simd<T, N>],
    dy: &mut [Simd<T, N>],
    dw: &mut [Simd<T, N>],
    tmp: &mut [Simd<T, N>],
    tmp2: &mut [Simd<T, N>],
    tolerances: &Tolerances<T, N>,
) -> Mask<i64, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement<Mask = i64> + From<f64>,
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
    // printf("%d %d", gid, wsize);

    // Compute feasibilities
    let normr = primal_feasibility(a, x, w, b);
    let norms = dual_feasibility(at, y, c, z);

    // Compute optimality
    let mut gamma = dot_product(z, x) + dot_product(w, y);

    let mu = delta * gamma / Simd::<T, N>::splat(((at.size + w.len()) as f64).into());
    // update relative tolerance
    gamma = gamma / (Simd::<T, N>::splat(1.0.into()) + vector_norm(x) + vector_norm(y));

    let is_nan: Mask<i64, N> = gamma.is_nan();
    if is_nan.any() {
        panic!("NaN encountered during IPM solve!")
    }

    // #ifdef DEBUG_GID
    // if (gid == DEBUG_GID) {
    //    printf("%d %d norm-r: %g, norm-s: %g, gamma: %g\n", gid, wsize, normr, norms, gamma);
    // }
    // #endif

    let status: Mask<i64, N> = normr.simd_lt(tolerances.primal_feasibility)
        & norms.simd_lt(tolerances.dual_feasibility)
        & gamma.simd_lt(tolerances.optimality);

    if status.all() {
        // Feasible and optimal; no further work!
        return status;
    }

    // Solve normal equations
    //   1. Calculate the RHS (into tmp2)
    normal_eqn_rhs(a, at, x, z, y, b, c, mu, w.len(), tmp, tmp2);

    //   2. Compute decomposition of normal matrix
    normal_matrix_cholesky_decomposition(a, a_norm_ptr, l_decomp_ptr, x, z, y, w, l_ptr, l_data);

    //   3. Solve system directly
    cholesky_solve(a.size, l_ptr, lt_ptr, l_data, tmp2, dy);

    // Calculate dx and dz
    //     dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
    //     dz = (mu - z*dx)/x - z
    //     dw = (mu - w*dy)/y - w
    let mut theta = compute_dx_dz_dw(at, x, z, y, w, c, dy, mu, dx, dz, dw);

    // println!("x: {:?}, z: {:?}, y: {:?}, w: {:?}", x, z, y, w);
    // println!("dx: {:?}, dz: {:?}, dy: {:?}, dw: {:?}", dx, dz, dy, dw);
    // println!("Theta: {:?}", theta);

    theta = (Simd::<T, N>::splat(0.9995.into()) / theta).simd_min(Simd::<T, N>::splat(1.0.into()));
    // if (gid == 0) {
    //     printf("%d theta: %g", gid, theta);
    // }

    // println!("Theta: {:?}", theta);
    // Set theta to zero for lanes that have completed (status == True)
    theta = status.select(Simd::<T, N>::splat(0.0.into()), theta);

    vector_update(x, dx, Simd::<T, N>::splat(1.0.into()), theta);
    vector_update(z, dz, Simd::<T, N>::splat(1.0.into()), theta);
    vector_update(y, dy, Simd::<T, N>::splat(1.0.into()), theta);
    vector_update(w, dw, Simd::<T, N>::splat(1.0.into()), theta);

    return status;
}

pub fn normal_eqn_init<T, const N: usize>(
    x: &mut [Simd<T, N>],
    z: &mut [Simd<T, N>],
    y: &mut [Simd<T, N>],
    w: &mut [Simd<T, N>],
) where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    vector_set(x, Simd::<T, N>::splat(1000.0.into()));
    vector_set(z, Simd::<T, N>::splat(1000.0.into()));
    vector_set(y, Simd::<T, N>::splat(1000.0.into()));
    vector_set(w, Simd::<T, N>::splat(1000.0.into()));
}
