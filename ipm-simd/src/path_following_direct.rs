use super::{Matrix, dual_feasibility, primal_feasibility};
use crate::Tolerances;
use crate::common::{compute_dx_dz_dw, dot_product, normal_eqn_rhs, vector_norm, vector_set, vector_update};
use fearless_simd::{Select, Simd, SimdBase, SimdFloat, SimdMask};
use ipm_common::SparseNormalCholeskyIndices;

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
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn normal_matrix_cholesky_decomposition<S: Simd>(
    simd: S,
    a: &Matrix<S>,
    a_norm_ptr: &ANormIndices,
    l_decomp_ptr: &LDecompositionIndices,
    x: &[S::f64s],
    z: &[S::f64s],
    y: &[S::f64s],
    w: &[S::f64s],
    l_ptr: &LIndices,
    l_data: &mut [S::f64s],
) {
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
                S::f64s::splat(simd, 0.0)
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
                val /= l_data[l_ptr.diag_indptr[col]];
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
#[inline(always)]
fn cholesky_solve<S: Simd>(
    _: S,
    a_size: usize,
    l_ptr: &LIndices,
    lt_ptr: &LTIndices,
    l_data: &[S::f64s],
    b: &[S::f64s],
    x: &mut [S::f64s],
) {
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
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn normal_eqn_step<S: Simd>(
    simd: S,
    a: &Matrix<S>,  // Sparse A matrix
    at: &Matrix<S>, // Sparse transpose of A matrix
    a_norm_ptr: &ANormIndices,
    l_decomp_ptr: &LDecompositionIndices,
    l_ptr: &LIndices,
    lt_ptr: &LTIndices,
    l_data: &mut [S::f64s],
    x: &mut [S::f64s],
    z: &mut [S::f64s],
    y: &mut [S::f64s],
    w: &mut [S::f64s],
    b: &[S::f64s],
    c: &[S::f64s],
    delta: S::f64s,
    dx: &mut [S::f64s],
    dz: &mut [S::f64s],
    dy: &mut [S::f64s],
    dw: &mut [S::f64s],
    tmp: &mut [S::f64s],
    tmp2: &mut [S::f64s],
    tolerances: &Tolerances,
) -> S::mask64s {
    // printf("%d %d", gid, wsize);

    // Compute feasibilities
    let normr = primal_feasibility(simd, a, x, w, b);
    let norms = dual_feasibility(simd, at, y, c, z);

    // Compute optimality
    let mut gamma = dot_product(simd, z, x) + dot_product(simd, w, y);

    let mu = delta * gamma / (at.size + w.len()) as f64;
    // update relative tolerance
    gamma /= S::f64s::splat(simd, 1.0) + vector_norm(simd, x) + vector_norm(simd, y);

    // #ifdef DEBUG_GID
    // if (gid == DEBUG_GID) {
    //    printf("%d %d norm-r: %g, norm-s: %g, gamma: %g\n", gid, wsize, normr, norms, gamma);
    // }
    // #endif

    let status = normr.simd_lt(tolerances.primal_feasibility)
        & norms.simd_lt(tolerances.dual_feasibility)
        & gamma.simd_lt(tolerances.optimality);

    if status.all_true() {
        // Feasible and optimal; no further work!
        return status;
    }

    // Solve normal equations
    //   1. Calculate the RHS (into tmp2)
    normal_eqn_rhs(simd, a, at, x, z, y, b, c, mu, w.len(), tmp, tmp2);

    //   2. Compute decomposition of normal matrix
    normal_matrix_cholesky_decomposition(simd, a, a_norm_ptr, l_decomp_ptr, x, z, y, w, l_ptr, l_data);

    //   3. Solve system directly
    cholesky_solve(simd, a.size, l_ptr, lt_ptr, l_data, tmp2, dy);

    // Calculate dx and dz
    //     dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
    //     dz = (mu - z*dx)/x - z
    //     dw = (mu - w*dy)/y - w
    let mut theta = compute_dx_dz_dw(simd, at, x, z, y, w, c, dy, mu, dx, dz, dw);

    // println!("x: {:?}, z: {:?}, y: {:?}, w: {:?}", x, z, y, w);
    // println!("dx: {:?}, dz: {:?}, dy: {:?}, dw: {:?}", dx, dz, dy, dw);
    // println!("Theta: {:?}", theta);

    theta = (S::f64s::splat(simd, 0.9995) / theta).min(S::f64s::splat(simd, 1.0));
    // if (gid == 0) {
    //     printf("%d theta: %g", gid, theta);
    // }

    // println!("Theta: {:?}", theta);
    // Set theta to zero for lanes that have completed (status == True)
    theta = status.select(S::f64s::splat(simd, 0.0), theta);

    vector_update(simd, x, dx, S::f64s::splat(simd, 1.0), theta);
    vector_update(simd, z, dz, S::f64s::splat(simd, 1.0), theta);
    vector_update(simd, y, dy, S::f64s::splat(simd, 1.0), theta);
    vector_update(simd, w, dw, S::f64s::splat(simd, 1.0), theta);

    status
}

#[inline(always)]
pub fn normal_eqn_init<S: Simd>(simd: S, x: &mut [S::f64s], z: &mut [S::f64s], y: &mut [S::f64s], w: &mut [S::f64s]) {
    vector_set(simd, x, S::f64s::splat(simd, 1000.0));
    vector_set(simd, z, S::f64s::splat(simd, 1000.0));
    vector_set(simd, y, S::f64s::splat(simd, 1000.0));
    vector_set(simd, w, S::f64s::splat(simd, 1000.0));
}
