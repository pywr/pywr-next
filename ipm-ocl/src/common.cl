/* Common OpenCL functions for path following interior point method.
 *
 *
 */


#pragma OPENCL EXTENSION cl_khr_fp64 : enable
#define matrix(N) __constant uint* N##indptr, __constant uint* N##indices, __constant REAL* N##data, uint N##size
#define write_matrix(N) __constant uint* restrict N##indptr, __constant uint* restrict N##indices, __global REAL* restrict N##data

__kernel void matrix_vector_product(
     matrix(A),
    __global REAL* restrict x,
    __global REAL* restrict out
) {
    /* Compute out = Ax
     *
     */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row_gid;
    uint row, col;
    uint index, last_index;
    REAL val;

    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;
        val = 0.0;

        index = Aindptr[row];
        last_index = Aindptr[row+1];

        for (; index < last_index; index++) {
            col = Aindices[index];
            val += Adata[index]*x[col*gsize+gid];
            // val = fma(Adata[index], x[col*gsize+gid], val);
        }

        out[row_gid] = val;
    }
}

REAL dot_product(__global REAL* restrict x, __global REAL* restrict y, int N) {
    /* Return dot product of x and y */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;
    REAL val = 0.0;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        val += x[row_gid]*y[row_gid];
    }
    return val;
}

__kernel void vector_copy(__global REAL* restrict x, __global REAL* restrict y, int N) {
    /* Copy vector x in to y */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        y[row_gid] = x[row_gid];
    }
}

__kernel void vector_copy_fd(__global float* restrict x, __global double* restrict y, int N) {
    /* Copy vector x in to y */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        y[row_gid] = x[row_gid];
    }
}

void vector_update(__global REAL* restrict x, __global REAL* restrict y, REAL xscale, REAL yscale, int N) {
    /* x = x*xscale + y*yscale */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        x[row_gid] = xscale*x[row_gid] + yscale*y[row_gid];
    }
}

void vector_set(__global REAL* restrict x, REAL scalar, int N) {
    /* x = scalar */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        x[row_gid] = scalar;
    }
}


__kernel void vector_float_to_double(__global float* restrict in, __global double* restrict out, __global double* restrict tmp, uint N) {
    /* Convert vector from float to double */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        tmp[row_gid] = convert_double(in[row_gid]);
        //printf("%d %d %g\n", gid, row_gid, in[row_gid]);
    }

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        out[row_gid] = tmp[row_gid];
        //printf("%d %d %g\n", gid, row_gid, out[row_gid]);
    }
}

REAL vector_max(__global REAL* restrict x, int N) {
    /* return max(x) */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    REAL val = -INFINITY;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        val = max(val, x[row_gid]);
    }
    return val;
}

REAL vector_norm(__global REAL* restrict x, int N) {
    /* return max(x) */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row;
    uint row_gid;

    REAL val = 0.0;

    for (row=0; row<N; row++) {
        row_gid = row*gsize + gid;
        val += pown(x[row_gid], 2);
    }
    return sqrt(val);
}


__kernel void normal_eqn_rhs(
    matrix(A),  // Sparse A matrix
    matrix(AT),  // Sparse transpose of A matrix
    __global REAL* restrict x,
    __global REAL* restrict z,
    __global REAL* restrict y,
    __global REAL* restrict b,
    __global REAL* restrict c,
    REAL mu,
    uint wsize,
    __global REAL* restrict tmp, // work array size of x
    __global REAL* restrict out // work array size of b
) {
    /* Compute the right-hand side of the system of primal normal equations

    rhs = -(b - A.dot(x) - mu/y - A.dot(x * (c - At.dot(y) + mu/x)/z))
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, index, last_index;
    uint row_gid;
    REAL val;

    // Calculate tmp = At.dot(y)
    matrix_vector_product(ATindptr, ATindices, ATdata, ATsize, y, tmp);


    // Calculate tmp = x * (c - At.dot(y) + mu/x)/z
    for (row=0; row<ATsize; row++) {
        row_gid = row*gsize + gid;
        tmp[row_gid] = x[row_gid]*(c[row_gid] - tmp[row_gid] + mu/x[row_gid])/z[row_gid];
    }

    // Calculate tmp2 = A.dot(tmp)
    matrix_vector_product(Aindptr, Aindices, Adata, Asize, tmp, out);

    // Compute out = -(b - A.dot(x) - mu/y -out)
    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        // The mu/y term is only applied to rows where w is defined.
        if (row < wsize) {
            val = mu / y[row_gid];
        } else {
            val = 0.0;
        }

        index = Aindptr[row];
        last_index = Aindptr[row+1];

        while (index < last_index) {
            col = Aindices[index];
            val += Adata[index]*x[col*gsize+gid];
            index += 1;
        }

        out[row_gid] = -(b[row_gid] - val - out[row_gid]);
    }
}

REAL primal_feasibility(
    matrix(A),  // Sparse A matrix
    uint ATsize,
    __global REAL* restrict x,
    __global REAL* restrict w,
    uint wsize,
    __global REAL* restrict b
) {
    /* Calculate primal-feasibility

        normr = || b - A.dot(x) - w || / max(|| b ||, 1)
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, index, last_index;
    uint row_gid;
    REAL val;

    REAL normx = 0.0;
    REAL normr = 0.0;

    // Compute ||x||
    for (col=0; col<ATsize; col++) {
        normx += pown(x[col*gsize+gid], 2);
    }

    // Compute primal feasibility
    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;
        val = b[row_gid];

        if (row < wsize) {
            val -= w[row_gid];
        }

        index = Aindptr[row];
        last_index = Aindptr[row+1];

        while (index < last_index) {
            col = Aindices[index];
            val -= Adata[index]*x[col*gsize+gid];
            index += 1;
        }

        normr += pown(val, 2);
    }

    return sqrt(normr) / (1 + sqrt(normx));
}

REAL dual_feasibility(
    matrix(AT),  // Sparse A matrix
    uint Asize,
    __global REAL* restrict y, __global REAL* restrict c, __global REAL* restrict z
) {
    /* Calculate dual-feasibility

        norms = || c - AT.dot(y) + z || / max(|| c ||, 1)
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, index, last_index;
    uint row_gid;
    REAL val;

    REAL normy = 0.0;
    REAL norms = 0.0;

    for (col=0; col<Asize; col++) {
        normy += pown(y[col*gsize+gid], 2);
    }

    // Compute primal feasibility
    for (row=0; row<ATsize; row++) {
        row_gid = row*gsize + gid;
        val = z[row_gid];
        val += c[row_gid];

        index = ATindptr[row];
        last_index = ATindptr[row+1];

        while (index < last_index) {
            col = ATindices[index];
            val -= ATdata[index]*y[col*gsize+gid];
            index += 1;
        }

        norms += pown(val, 2);
    }

    return sqrt(norms)/(1 + sqrt(normy));
}

REAL compute_dx_dz_dw(
    uint Asize,
    matrix(AT), // Sparse transpose of A matrix
    __global REAL* restrict x,
    __global REAL* restrict z,
    __global REAL* restrict y,
    __global REAL* restrict w,
    uint wsize,
    __global REAL* restrict c,
    __global REAL* restrict dy,
    REAL mu,
    __global REAL* restrict dx,
    __global REAL* restrict dz,
    __global REAL* restrict dw
) {
    /*  Compute the path step changes given known dy and return maximum value of theta.

        Theta value is the max(-dx/x, -dz/z, -dw/w, -dy/y).

        dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
        dz = (mu - z*dx)/x - z
        dw = (mu - w*dy)/y - w
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, index, last_index;
    uint row_gid;
    REAL val, val2;

    REAL theta_xz = 0.0;
    REAL theta_wy = 0.0;

    for (row=0; row<ATsize; row++) {
        row_gid = row*gsize + gid;
        val = 0.0;
        val2 = 0.0;

        index = ATindptr[row];
        last_index = ATindptr[row+1];

        while (index < last_index) {
            col = ATindices[index];
            val += ATdata[index]*y[col*gsize+gid];
            val2 += ATdata[index]*dy[col*gsize+gid];
            index += 1;
        }

        dx[row_gid] = (c[row_gid] - val - val2 + mu/x[row_gid])*x[row_gid]/z[row_gid];
        dz[row_gid] = (mu - z[row_gid]*dx[row_gid])/x[row_gid] - z[row_gid];

        theta_xz = max(max(theta_xz, -dx[row_gid]/x[row_gid]), -dz[row_gid]/z[row_gid]);
    }

    // dw is only defined for rows with w (i.e. inequality rows with a slack variable)
    for (row=0; row<wsize; row++) {
        row_gid = row*gsize + gid;

        dw[row_gid] = (mu - w[row_gid]*dy[row_gid])/y[row_gid] - w[row_gid];
        theta_wy = max(max(theta_wy, -dw[row_gid]/w[row_gid]), -dy[row_gid]/y[row_gid]);
    }

    return max(theta_xz, theta_wy);
}
