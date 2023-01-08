



__kernel void normal_matrix_vector_product(
    __constant REAL* Adata, uint Asize,
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b,
    __global REAL* out
) {
    /* Compute the product of the normal equations (w/y + AA^T) with vector b

    i.e. (w/y + A(x/z)A^T)b
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, ii, jj;
    uint row_gid, kk;
    uint col_start, col_end, col_ptr, col_ptr_end;
    REAL val, inner_val;

    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        if (row < wsize) {
            val = b[row_gid]*w[row_gid] / y[row_gid];
        } else {
            val = 0.0;
        }

        col_ptr = Anorm_rowptr[row];
        col_ptr_end = Anorm_rowptr[row + 1];

        for (; col_ptr < col_ptr_end; col_ptr++) {
            col_start = Anorm_colptr[col_ptr];
            col_end = Anorm_colptr[col_ptr + 1];
            col = Anorm_colindices[col_ptr];
            inner_val = 0.0;

            for (; col_start < col_end; col_start++) {
                ii = Anorm_indptr_i[col_start];
                jj = Anorm_indptr_j[col_start];
                kk = gsize*Anorm_indices[col_start] + gid;
                inner_val += Adata[ii]*Adata[jj]*x[kk]/z[kk];
            }

            //printf("row [%d]; col_start, col_end [%d, %d]\n", row, col_start, col_end);
            val += inner_val*b[col*gsize + gid];
        }

        out[row_gid] = val;
        //break;
    }
}


REAL vector_normal_eqn_vector_product(
    __constant REAL* Adata,
    uint Asize,
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b
) {
    /* Compute the product of the normal equations (AA^T) with vector b

    i.e. b(w/y + A(x/z)A^T)b
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, ii, jj, ik;
    uint row_gid, kk;
    uint col_start, col_end, col_ptr, col_ptr_end;
    REAL val = 0.0;
    REAL brow;
    REAL inner_val;

    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        col_ptr = Anorm_rowptr[row];
        col_ptr_end = Anorm_rowptr[row + 1];

        brow = b[row_gid];

        if (row < wsize) {
            val += pown(brow, 2) * w[row_gid] / y[row_gid];
        }

        for (; col_ptr < col_ptr_end; col_ptr++) {
            col_start = Anorm_colptr[col_ptr];
            col_end = Anorm_colptr[col_ptr + 1];
            col = Anorm_colindices[col_ptr];
            inner_val = 0.0;

            for (; col_start < col_end; col_start++) {
                ii = Anorm_indptr_i[col_start];
                jj = Anorm_indptr_j[col_start];
                ik = Anorm_indices[col_start];
                kk = gsize*ik + gid;
                inner_val += Adata[ii]*Adata[jj]*x[kk]/z[kk];
            }


            val += brow*inner_val*b[col*gsize + gid];
            // if (gid == 0) {
            //     //printf("Alpha: %g %g\n", r_z, alpha);
            //     printf("row [%d]; %g %g %g\n", row, brow, b[col*gsize + gid], val);
            // }
        }

    }

    return val;
}


void residuals(
    __constant REAL* Adata,
    uint Asize,
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b,
    __global REAL* dy,
    __global REAL* out
) {
    /* Compute the residual, r = b - (w/y + A(x/z)A^T)dy
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row_gid;
    uint row;

    // Compute the matrix-vector product (A(x/z)A^T)y
    normal_matrix_vector_product(Adata, Asize, Anorm_rowptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
                                 Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, dy, out);

    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        //printf("residual %d: %f %f %f\n", row_gid, b[row_gid], out[row_gid], b[row_gid] - out[row_gid]);
        out[row_gid] = b[row_gid] - out[row_gid];

    }
}


void preconditioned_residuals(
    __constant REAL* Adata,
    uint Asize,
    __constant uint* Anorm_diagptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* r,
    __global REAL* out
) {
    /* Compute z = M^{-1}r

    Where M = diag(A)
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, ii, jj, ik;
    uint row_gid, kk;
    uint col_start, col_end, col_ptr, col_ptr_end;
    REAL val;

    for (row=0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        col_ptr = Anorm_diagptr[row];

        col_start = Anorm_colptr[col_ptr];
        col_end = Anorm_colptr[col_ptr + 1];

        if (row < wsize) {
            val = w[row_gid] / y[row_gid];
        } else {
            val = 0.0;
        }

        while (col_start < col_end) {
            ii = Anorm_indptr_i[col_start];
            jj = Anorm_indptr_j[col_start];
            ik = Anorm_indices[col_start];
            kk = gsize*ik + gid;
            val += Adata[ii]*Adata[jj]*x[kk]/z[kk];
            col_start += 1;
        }

        // if (gid == 0) {
        //     printf("pre-cond %d of %d:, %f %f %f\n", row, Asize, r[row_gid], val, r[row_gid]/val);
        // }
        out[row_gid] = r[row_gid]/val;
    }
}


__kernel void normal_eqn_conjugate_gradient(
    __constant REAL* Adata,
    uint Asize,
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_diagptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b,
    __global REAL* dy,
    __global REAL* r,
    __global REAL* p,
    __global REAL* s
) {
    /* Solve the normal equations for dy

    (w/y + A(x/z)A^T)dy = b

    */
    uint gid = get_global_id(0);
    uint iter;
    REAL r_z, r_z_next, alpha, rr, beta;

    // Compute the initial residuals
    residuals(Adata, Asize, Anorm_rowptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
              Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, b, dy, r);

    // Compute preconditioned residuals
    preconditioned_residuals(Adata, Asize, Anorm_diagptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
                             Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, r, s);

    r_z = dot_product(r, s, Asize);

    // initialise p
    vector_copy(s, p, Asize);

    for (iter=0; iter<1000; iter++) {
        //printf("Iteration: %d\n", iter);
        alpha = r_z / vector_normal_eqn_vector_product(Adata, Asize, Anorm_rowptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
                                                       Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, p);
        // if (gid == 0) {
        //     printf("Alpha: %g %g\n", r_z, alpha);
        // }
        // Update dy
        vector_update(dy, p, 1.0, alpha, Asize);

        // Update the residuals
        residuals(Adata, Asize, Anorm_rowptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
              Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, b, dy, r);

        rr = sqrt(dot_product(r, r, Asize));
        // if (gid == 0) {
        //      printf("d iter: %d: |residuals| %g\n", iter, rr);
        // }

        if (rr < 1e-6) {
            //printf("%d iter: %d: |residuals| %g\n", gid, iter, rr);
            // printf("Solved!\n");
            return;
        }

        // Compute preconditioned residuals
        preconditioned_residuals(Adata, Asize, Anorm_diagptr, Anorm_colptr, Anorm_colindices, Anorm_indices,
                             Anorm_indptr_i, Anorm_indptr_j, x, z, y, w, wsize, r, s);

        r_z_next = dot_product(r, s, Asize);
        beta = r_z_next / r_z;

        // if (gid == 0) {
        //     printf("Beta: %g %g %g\n", beta, r_z, r_z_next);
        // }

        vector_update(p, s, beta, 1.0, Asize);

        r_z = r_z_next;
    }
    //printf("%d failed after iter: %d: |residuals| %g\n", gid, iter, rr);
}




uint normal_eqn_step(
    matrix(A),  // Sparse A matrix
    matrix(AT),  // Sparse transpose of A matrix
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_diagptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b,
    __global REAL* c,
    REAL delta,
    __global REAL* dx,
    __global REAL* dz,
    __global REAL* dy,
    __global REAL* dw,
    __global REAL* r, __global REAL* p, __global REAL* s,  // Work arrays for conjugate gradient method
    __global REAL* tmp, // work array size of x
    __global REAL* tmp2 // work array size of b
) {
    /* Perform a single step of the path-following algorithm.

    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);

    // Compute feasibilities
    REAL normr = primal_feasibility(Aindptr, Aindices, Adata, Asize, ATsize, x, w, wsize, b);
    REAL norms = dual_feasibility(ATindptr, ATindices, ATdata, ATsize, Asize, y, c, z);
    // Compute optimality
    REAL gamma = dot_product(z, x, ATsize) + dot_product(w, y, wsize);
    REAL mu = delta * gamma / (ATsize + wsize);

    REAL max_x = vector_max(x, ATsize);
    REAL max_y = vector_max(y, Asize);

    if (gid == 0) {
        printf("%d %d norm-r: %g, norm-s: %g, gamma: %g, max(x): %g, max(y): %g\n", gid, wsize, normr, norms, gamma, max_x, max_y);
    }
    if ((normr < 1e-8) && (norms < 1e-8) && (gamma < 1e-8)) {
        // Feasible and optimal; no further work!
        // TODO set a status output?
        return 0;
    }

    // Solve normal equations
    //   1. Calculate the RHS (into tmp2)
    normal_eqn_rhs(
        Aindptr, Aindices, Adata, Asize, ATindptr, ATindices, ATdata, ATsize,
        x, z, y, b, c, mu, wsize, tmp, tmp2
    );

    //   2. Set initial guess of dy
    vector_set(dy, 0.0, Asize);

    //   3. Solve the normal equations for dy
    normal_eqn_conjugate_gradient(
        Adata, Asize, Anorm_rowptr, Anorm_diagptr, Anorm_colptr, Anorm_colindices, Anorm_indices, Anorm_indptr_i, Anorm_indptr_j,
        x, z, y, w, wsize, tmp2, dy, r, p, s
    );

    // Calculate dx and dz
    //     dx = (c - AT.dot(y) - AT.dot(dy) + mu/x)*x/z
    //     dz = (mu - z*dx)/x - z
    //     dw = (mu - w*dy)/y - w
    REAL theta = compute_dx_dz_dw(
        Asize, ATindptr, ATindices, ATdata, ATsize,
        x, z, y, w, wsize, c, dy, mu, dx, dz, dw
    );

    theta = min(0.9/theta, 1.0);
    // if (gid == 0) {
    //     printf("%d theta: %g", gid, theta);
    // }

    vector_update(x, dx, 1.0, theta, ATsize);
    vector_update(z, dz, 1.0, theta, ATsize);
    vector_update(y, dy, 1.0, theta, Asize);
    vector_update(w, dw, 1.0, theta, wsize);

    return 1;
}

__kernel void normal_eqn_solve(
    matrix(A),  // Sparse A matrix
    matrix(AT),  // Sparse transpose of A matrix
    __constant uint* Anorm_rowptr,
    __constant uint* Anorm_diagptr,
    __constant uint* Anorm_colptr,
    __constant uint* Anorm_colindices,
    __constant uint* Anorm_indices,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __global REAL* x,
    __global REAL* z,
    __global REAL* y,
    __global REAL* w,
    uint wsize,
    __global REAL* b,
    __global REAL* c,
    REAL delta,
    __global REAL* dx,
    __global REAL* dz,
    __global REAL* dy,
    __global REAL* dw,
    __global REAL* r,
    __global REAL* p,
    __global REAL* s,  // Work arrays for conjugate gradient method
    __global REAL* tmp, // work array size of x
    __global REAL* tmp2, // work array size of b
    uint init
) {
    uint i;
    uint status;
    uint gid = get_global_id(0);
    //printf("Starting solve kernel ...");

    if (init == 1) {
        //printf("Resetting vectors ...");
        vector_set(x, 1.0, ATsize);
        vector_set(z, 1.0, ATsize);
        vector_set(y, 1.0, Asize);
        vector_set(w, 1.0, wsize);
    }

    for (i=0; i<50; i++) {
        status = normal_eqn_step(
            Aindptr, Aindices, Adata, Asize,
            ATindptr, ATindices, ATdata, ATsize,
            Anorm_rowptr, Anorm_diagptr, Anorm_colptr, Anorm_colindices, Anorm_indices, Anorm_indptr_i, Anorm_indptr_j,
            x, z, y, w, wsize, b, c, delta, dx, dz, dy, dw,
            r, p, s, tmp, tmp2
        );
        if (status == 0) {
            return;
        }
    }
    printf("Run %d failed!", gid);
}
