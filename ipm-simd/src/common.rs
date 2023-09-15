use nalgebra_sparse::CsrMatrix;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};
use std::simd::{LaneCount, Simd, SimdElement, SimdFloat, StdFloat, SupportedLaneCount};

pub struct Matrix<T, const N: usize>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    pub indptr: Vec<usize>,
    pub indices: Vec<usize>,
    pub data: Vec<Simd<T, N>>,
    pub size: usize,
}

impl<T, const N: usize> Matrix<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
{
    pub fn from_sparse_matrix(a: &CsrMatrix<f64>) -> Self {
        let data = a.values().iter().map(|&v| Simd::splat(v.into())).collect();
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
pub fn matrix_vector_product<T, const N: usize>(matrix: &Matrix<T, N>, x: &[Simd<T, N>], out: &mut [Simd<T, N>])
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: Mul<Simd<T, N>, Output = Simd<T, N>> + AddAssign,
{
    for row in 0..matrix.size {
        let mut val: Simd<T, N> = Simd::splat(0.0.into());

        let first_index = matrix.indptr[row];
        let last_index = matrix.indptr[row + 1];

        for index in first_index..last_index {
            let col = matrix.indices[index];
            val += matrix.data[index] * x[col];
        }

        out[row] = val;
    }
}

/// Return dot product of x and y
pub fn dot_product<T, const N: usize>(x: &[Simd<T, N>], y: &[Simd<T, N>]) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
    Simd<T, N>: Mul<Simd<T, N>, Output = Simd<T, N>> + Sum,
{
    x.iter().zip(y.iter()).map(|(a, b)| a * b).sum()
}

/// `x = x*xscale + y*yscale`
pub fn vector_update<T, const N: usize>(x: &mut [Simd<T, N>], y: &[Simd<T, N>], xscale: Simd<T, N>, yscale: Simd<T, N>)
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
    Simd<T, N>: Mul<Simd<T, N>, Output = Simd<T, N>> + Add<Output = Simd<T, N>>,
{
    for i in 0..x.len() {
        x[i] = xscale * x[i] + yscale * y[i];
    }
}

/// `x = scalar`
pub fn vector_set<T, const N: usize>(x: &mut [Simd<T, N>], scalar: Simd<T, N>)
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
{
    x.iter_mut().for_each(|a| *a = scalar)
}

/// return max(x)
pub fn vector_norm<T, const N: usize>(x: &[Simd<T, N>]) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement,
    Simd<T, N>: Mul<Simd<T, N>, Output = Simd<T, N>> + Sum + StdFloat,
{
    x.iter().map(|a| a * a).sum::<Simd<T, N>>().sqrt()
}

/// Compute the right-hand side of the system of primal normal equations
///
/// `rhs = -(b - A.dot(x) - mu/y - A.dot(x * (c - At.dot(y) + mu/x)/z))`
///
pub fn normal_eqn_rhs<T, const N: usize>(
    a: &Matrix<T, N>,  // Sparse A matrix
    at: &Matrix<T, N>, // Sparse transpose of A matrix
    x: &[Simd<T, N>],
    z: &[Simd<T, N>],
    y: &[Simd<T, N>],
    b: &[Simd<T, N>],
    c: &[Simd<T, N>],
    mu: Simd<T, N>,
    wsize: usize,
    tmp: &mut [Simd<T, N>], // work array size of x
    out: &mut [Simd<T, N>], // work array size of b
) where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: AddAssign
        + Sum
        + StdFloat
        + Mul<Output = Simd<T, N>>
        + Add<Output = Simd<T, N>>
        + Sub<Output = Simd<T, N>>
        + Div<Output = Simd<T, N>>
        + Neg<Output = Simd<T, N>>,
{
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
        let mut val = if row < wsize {
            mu / y[row]
        } else {
            Simd::<T, N>::splat(0.0.into())
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
pub fn primal_feasibility<T, const N: usize>(
    a: &Matrix<T, N>, // Sparse A matrix
    x: &[Simd<T, N>],
    w: &[Simd<T, N>],
    b: &[Simd<T, N>],
) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: AddAssign
        + Sum
        + StdFloat
        + Mul<Output = Simd<T, N>>
        + Add<Output = Simd<T, N>>
        + Sub<Output = Simd<T, N>>
        + Div<Output = Simd<T, N>>
        + Neg<Output = Simd<T, N>>,
{
    // Compute ||x||
    let normx: Simd<T, N> = x.iter().map(|a| a * a).sum();

    // Compute primal feasibility
    let mut normr = Simd::<T, N>::splat(0.0.into());
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

    normr.sqrt() / (Simd::<T, N>::splat(1.0.into()) + normx.sqrt())
}

/// Calculate dual-feasibility
///     `norms = || c - AT.dot(y) + z || / max(|| c ||, 1)`
///
pub fn dual_feasibility<T, const N: usize>(
    at: &Matrix<T, N>, // Sparse A matrix
    y: &[Simd<T, N>],
    c: &[Simd<T, N>],
    z: &[Simd<T, N>],
) -> Simd<T, N>
where
    LaneCount<N>: SupportedLaneCount,
    T: SimdElement + From<f64>,
    Simd<T, N>: AddAssign
        + Sum
        + StdFloat
        + Mul<Output = Simd<T, N>>
        + Add<Output = Simd<T, N>>
        + Sub<Output = Simd<T, N>>
        + Div<Output = Simd<T, N>>
        + Neg<Output = Simd<T, N>>,
{
    let normy: Simd<T, N> = y.iter().map(|a| a * a).sum();

    let mut norms = Simd::<T, N>::splat(0.0.into());
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

    norms.sqrt() / (Simd::<T, N>::splat(1.0.into()) + normy.sqrt())
}

/// Compute the path step changes given known dy and return maximum value of theta.
///
/// Theta value is the max(-dx/x, -dz/z, -dw/w, -dy/y).
///
///     dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
///     dz = (mu - z*dx)/x - z
///     dw = (mu - w*dy)/y - w
///
pub fn compute_dx_dz_dw<T, const N: usize>(
    at: &Matrix<T, N>, // Sparse A matrix
    x: &[Simd<T, N>],
    z: &[Simd<T, N>],
    y: &[Simd<T, N>],
    w: &[Simd<T, N>],
    c: &[Simd<T, N>],
    dy: &[Simd<T, N>],
    mu: Simd<T, N>,
    dx: &mut [Simd<T, N>],
    dz: &mut [Simd<T, N>],
    dw: &mut [Simd<T, N>],
) -> Simd<T, N>
where
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
    let mut theta_xz = Simd::<T, N>::splat(0.0.into());
    let mut theta_wy = Simd::<T, N>::splat(0.0.into());

    for row in 0..at.size {
        let mut val = Simd::<T, N>::splat(0.0.into());
        let mut val2 = Simd::<T, N>::splat(0.0.into());

        let first_index = at.indptr[row];
        let last_index = at.indptr[row + 1];

        for index in first_index..last_index {
            let col = at.indices[index];
            val += at.data[index] * y[col];
            val2 += at.data[index] * dy[col];
        }

        dx[row] = (c[row] - val - val2 + mu / x[row]) * x[row] / z[row];
        dz[row] = (mu - z[row] * dx[row]) / x[row] - z[row];

        theta_xz = theta_xz.simd_max(-dx[row] / x[row]).simd_max(-dz[row] / z[row]);
    }

    // dw is only defined for rows with w (i.e. inequality rows with a slack variable)
    for row in 0..w.len() {
        dw[row] = (mu - w[row] * dy[row]) / y[row] - w[row];
        theta_wy = theta_wy.simd_max(-dw[row] / w[row]).simd_max(-dy[row] / y[row]);
    }

    theta_xz.simd_max(theta_wy)
}
