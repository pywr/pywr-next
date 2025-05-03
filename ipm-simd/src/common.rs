use nalgebra_sparse::CsrMatrix;
use wide::f64x4;

pub struct Matrix {
    pub indptr: Vec<usize>,
    pub indices: Vec<usize>,
    pub data: Vec<f64x4>,
    pub size: usize,
}

impl Matrix {
    pub fn from_sparse_matrix(a: &CsrMatrix<f64>) -> Self {
        let data = a.values().iter().map(|&v| f64x4::splat(v)).collect();
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
pub fn matrix_vector_product(matrix: &Matrix, x: &[f64x4], out: &mut [f64x4]) {
    for (row, o) in out.iter_mut().enumerate().take(matrix.size) {
        let mut val = f64x4::splat(0.0);

        let first_index = matrix.indptr[row];
        let last_index = matrix.indptr[row + 1];

        for index in first_index..last_index {
            let col = matrix.indices[index];
            val += matrix.data[index] * x[col];
        }

        *o = val;
    }
}

/// Return dot product of x and y
pub fn dot_product(x: &[f64x4], y: &[f64x4]) -> f64x4 {
    x.iter().zip(y.iter()).map(|(&a, &b)| a * b).sum()
}

/// `x = x*xscale + y*yscale`
pub fn vector_update(x: &mut [f64x4], y: &[f64x4], xscale: f64x4, yscale: f64x4) {
    for i in 0..x.len() {
        x[i] = xscale * x[i] + yscale * y[i];
    }
}

/// `x = scalar`
pub fn vector_set(x: &mut [f64x4], scalar: f64x4) {
    x.iter_mut().for_each(|a| *a = scalar)
}

/// return max(x)
pub fn vector_norm(x: &[f64x4]) -> f64x4 {
    x.iter().map(|&a| a * a).sum::<f64x4>().sqrt()
}

/// Compute the right-hand side of the system of primal normal equations
///
/// `rhs = -(b - A.dot(x) - mu/y - A.dot(x * (c - At.dot(y) + mu/x)/z))`
///
#[allow(clippy::too_many_arguments)]
pub fn normal_eqn_rhs(
    a: &Matrix,  // Sparse A matrix
    at: &Matrix, // Sparse transpose of A matrix
    x: &[f64x4],
    z: &[f64x4],
    y: &[f64x4],
    b: &[f64x4],
    c: &[f64x4],
    mu: f64x4,
    wsize: usize,
    tmp: &mut [f64x4], // work array size of x
    out: &mut [f64x4], // work array size of b
) {
    // Calculate tmp = At.dot(y)
    matrix_vector_product(at, y, tmp);

    // Calculate tmp = x * (c - At.dot(y) + mu/x)/z
    for row in 0..at.size {
        tmp[row] = x[row] * (c[row] - tmp[row] + mu / x[row]) / z[row];
    }
    // Calculate tmp2 = A.dot(tmp)
    matrix_vector_product(a, tmp, out);

    // Compute out = -(b - A.dot(x) - mu/y -out)
    for row in 0..a.size {
        // The mu/y term is only applied to rows where w is defined.
        let mut val = if row < wsize { mu / y[row] } else { f64x4::splat(0.0) };

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
pub fn primal_feasibility(
    a: &Matrix, // Sparse A matrix
    x: &[f64x4],
    w: &[f64x4],
    b: &[f64x4],
) -> f64x4 {
    // Compute ||x||
    let normx: f64x4 = x.iter().map(|&a| a * a).sum();

    // Compute primal feasibility
    let mut normr = f64x4::splat(0.0);
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

    normr.sqrt() / (1.0 + normx.sqrt())
}

/// Calculate dual-feasibility
///     `norms = || c - AT.dot(y) + z || / max(|| c ||, 1)`
///
pub fn dual_feasibility(
    at: &Matrix, // Sparse A matrix
    y: &[f64x4],
    c: &[f64x4],
    z: &[f64x4],
) -> f64x4 {
    let normy: f64x4 = y.iter().map(|&a| a * a).sum();

    let mut norms = f64x4::splat(0.0);
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

    norms.sqrt() / (1.0 + normy.sqrt())
}

/// Compute the path step changes given known dy and return maximum value of theta.
///
/// Theta value is the max(-dx/x, -dz/z, -dw/w, -dy/y).
///
///     dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
///     dz = (mu - z*dx)/x - z
///     dw = (mu - w*dy)/y - w
///
#[allow(clippy::too_many_arguments)]
pub fn compute_dx_dz_dw(
    at: &Matrix, // Sparse A matrix
    x: &[f64x4],
    z: &[f64x4],
    y: &[f64x4],
    w: &[f64x4],
    c: &[f64x4],
    dy: &[f64x4],
    mu: f64x4,
    dx: &mut [f64x4],
    dz: &mut [f64x4],
    dw: &mut [f64x4],
) -> f64x4 {
    let mut theta_xz = f64x4::splat(0.0);
    let mut theta_wy = f64x4::splat(0.0);

    for row in 0..at.size {
        let mut val = f64x4::splat(0.0);
        let mut val2 = f64x4::splat(0.0);

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
