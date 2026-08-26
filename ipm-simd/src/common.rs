use fearless_simd::{Simd, SimdBase, SimdFloat};
use fearless_simd_macros::simd;
use nalgebra_sparse::CsrMatrix;

pub struct Matrix<S: Simd> {
    pub indptr: Vec<usize>,
    pub indices: Vec<usize>,
    pub data: Vec<S::f64s>,
    pub size: usize,
}

impl<S: Simd> Matrix<S>
where
    S: Simd,
{
    #[simd]
    pub fn from_sparse_matrix(simd: S, a: &CsrMatrix<f64>) -> Self {
        let data = a.values().iter().map(|&v| S::f64s::splat(simd, v)).collect();
        let indptr = a.row_offsets().to_vec();
        let indices = a.col_indices().to_vec();

        Self {
            indptr,
            indices,
            data,
            size: a.nrows(),
        }
    }
}

/// Compute `out = Ax`
#[simd]
pub fn matrix_vector_product<S: Simd>(simd: S, matrix: &Matrix<S>, x: &[S::f64s], out: &mut [S::f64s]) {
    for (row, o) in out.iter_mut().enumerate().take(matrix.size) {
        *o = S::f64s::splat(simd, 0.0);

        let first_index = matrix.indptr[row];
        let last_index = matrix.indptr[row + 1];

        for index in first_index..last_index {
            let col = matrix.indices[index];
            *o += matrix.data[index] * x[col];
        }
    }
}

/// Return dot product of x and y
#[simd]
pub fn dot_product<S: Simd>(simd: S, x: &[S::f64s], y: &[S::f64s]) -> S::f64s {
    let mut out = S::f64s::splat(simd, 0.0);

    for (a, b) in x.iter().zip(y.iter()) {
        out += *a * *b;
    }

    out
}

/// `x = x*xscale + y*yscale`
#[simd]
pub fn vector_update<S: Simd>(_: S, x: &mut [S::f64s], y: &[S::f64s], xscale: S::f64s, yscale: S::f64s) {
    for i in 0..x.len() {
        x[i] = xscale * x[i] + yscale * y[i];
    }
}

/// `x = scalar`
#[simd]
pub fn vector_set<S: Simd>(_: S, x: &mut [S::f64s], scalar: S::f64s) {
    for a in x.iter_mut() {
        *a = scalar;
    }
}

/// return max(x)
#[simd]
pub fn vector_norm<S: Simd>(simd: S, x: &[S::f64s]) -> S::f64s {
    let mut out = S::f64s::splat(simd, 0.0);
    for &a in x.iter() {
        out += a * a;
    }
    out.sqrt()
}

/// Compute the right-hand side of the system of primal normal equations
///
/// `rhs = -(b - A.dot(x) - mu/y - A.dot(x * (c - At.dot(y) + mu/x)/z))`
///
#[allow(clippy::too_many_arguments)]
#[simd]
pub fn normal_eqn_rhs<S: Simd>(
    simd: S,
    a: &Matrix<S>,  // Sparse A matrix
    at: &Matrix<S>, // Sparse transpose of A matrix
    x: &[S::f64s],
    z: &[S::f64s],
    y: &[S::f64s],
    b: &[S::f64s],
    c: &[S::f64s],
    mu: S::f64s,
    wsize: usize,
    tmp: &mut [S::f64s], // work array size of x
    out: &mut [S::f64s], // work array size of b
) {
    // Calculate tmp = At.dot(y)
    matrix_vector_product(simd, at, y, tmp);

    // Calculate tmp = x * (c - At.dot(y) + mu/x)/z
    for row in 0..at.size {
        tmp[row] = x[row] * (c[row] - tmp[row] + mu / x[row]) / z[row];
    }
    // Calculate tmp2 = A.dot(tmp)
    matrix_vector_product(simd, a, tmp, out);

    // Compute out = -(b - A.dot(x) - mu/y -out)
    for row in 0..a.size {
        // The mu/y term is only applied to rows where w is defined.
        let mut val = if row < wsize {
            mu / y[row]
        } else {
            S::f64s::splat(simd, 0.0)
        };

        let first_index = a.indptr[row];
        let last_index = a.indptr[row + 1];

        for index in first_index..last_index {
            let col = a.indices[index];
            val += a.data[index] * x[col];
        }

        out[row] = -(b[row] - val - out[row]);
    }
}

/// Calculate primal-feasibility
///
/// `normr = || b - A.dot(x) - w || / max(|| b ||, 1)`
///
#[simd]
pub fn primal_feasibility<S: Simd>(
    simd: S,
    a: &Matrix<S>, // Sparse A matrix
    x: &[S::f64s],
    w: &[S::f64s],
    b: &[S::f64s],
) -> S::f64s {
    // Compute ||x||
    let mut normx = S::f64s::splat(simd, 0.0);
    for &a in x.iter() {
        normx += a * a;
    }

    // Compute primal feasibility
    let mut normr = S::f64s::splat(simd, 0.0);
    for row in 0..a.size {
        let mut val = b[row];

        if row < w.len() {
            val -= w[row];
        }

        let first_index = a.indptr[row];
        let last_index = a.indptr[row + 1];

        for index in first_index..last_index {
            let col = a.indices[index];
            val -= a.data[index] * x[col];
        }

        normr += val * val;
    }

    normr.sqrt() / (S::f64s::splat(simd, 1.0) + normx.sqrt())
}

/// Calculate dual-feasibility
///     `norms = || c - AT.dot(y) + z || / max(|| c ||, 1)`
///
#[simd]
pub fn dual_feasibility<S: Simd>(
    simd: S,
    at: &Matrix<S>, // Sparse A matrix
    y: &[S::f64s],
    c: &[S::f64s],
    z: &[S::f64s],
) -> S::f64s {
    let mut normy: S::f64s = S::f64s::splat(simd, 0.0);
    for &a in c.iter() {
        normy += a * a;
    }

    let mut norms = S::f64s::splat(simd, 0.0);
    // Compute primal feasibility
    for row in 0..at.size {
        let mut val = z[row];
        val += c[row];

        let first_index = at.indptr[row];
        let last_index = at.indptr[row + 1];

        for index in first_index..last_index {
            let col = at.indices[index];
            val -= at.data[index] * y[col];
        }

        norms += val * val;
    }

    norms.sqrt() / (S::f64s::splat(simd, 1.0) + normy.sqrt())
}

/// Compute the path step changes given known dy and return maximum value of theta.
///
/// Theta value is the max(-dx/x, -dz/z, -dw/w, -dy/y).
///
///  dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
///  dz = (mu - z*dx)/x - z
///  dw = (mu - w*dy)/y - w
///
#[allow(clippy::too_many_arguments)]
#[simd]
pub fn compute_dx_dz_dw<S: Simd>(
    simd: S,
    at: &Matrix<S>, // Sparse A matrix
    x: &[S::f64s],
    z: &[S::f64s],
    y: &[S::f64s],
    w: &[S::f64s],
    c: &[S::f64s],
    dy: &[S::f64s],
    mu: S::f64s,
    dx: &mut [S::f64s],
    dz: &mut [S::f64s],
    dw: &mut [S::f64s],
) -> S::f64s {
    let mut theta_xz = S::f64s::splat(simd, 0.0);
    let mut theta_wy = S::f64s::splat(simd, 0.0);

    for row in 0..at.size {
        let mut val = S::f64s::splat(simd, 0.0);
        let mut val2 = S::f64s::splat(simd, 0.0);

        let first_index = at.indptr[row];
        let last_index = at.indptr[row + 1];

        for index in first_index..last_index {
            let col = at.indices[index];
            val += at.data[index] * y[col];
            val2 += at.data[index] * dy[col];
        }

        dx[row] = (c[row] - val - val2 + mu / x[row]) * x[row] / z[row];
        dz[row] = (mu - z[row] * dx[row]) / x[row] - z[row];

        theta_xz = theta_xz.max(-dx[row] / x[row]).max(-dz[row] / z[row]);
    }

    // dw is only defined for rows with w (i.e. inequality rows with a slack variable)
    for row in 0..w.len() {
        dw[row] = (mu - w[row] * dy[row]) / y[row] - w[row];
        theta_wy = theta_wy.max(-dw[row] / w[row]).max(-dy[row] / y[row]);
    }

    theta_xz.max(theta_wy)
}
