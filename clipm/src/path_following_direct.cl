


__kernel void normal_matrix_cholesky_decomposition(
    __constant REAL* Adata, uint Asize,
    __constant uint* Anorm_indptr,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __constant uint* Anorm_indices,
    __constant uint* Ldecomp_indptr,
    __constant uint* Ldecomp_indptr_i,
    __constant uint* Ldecomp_indptr_j,
    __global REAL* restrict x,
    __global REAL* restrict z,
    __global REAL* restrict y,
    __global REAL* restrict w,
    uint wsize,
    __constant uint* Lindptr,
    __constant uint* Ldiag_indptr,
    __constant uint* Lindices,
    __global REAL* restrict Ldata
) {
    /*
    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);
    uint row, col, ii, jj;
    uint row_gid, xind;
    uint row_ind, row_ind_end;
    uint ind, ind_end;
    REAL val;

    uint Lentry = 0;

    for (row = 0; row<Asize; row++) {
        row_gid = row*gsize + gid;

        row_ind = Lindptr[row];
        row_ind_end = Lindptr[row + 1];

        // Iterate the columns of L
        for (; row_ind < row_ind_end; row_ind++) {
            col = Lindices[row_ind];

            // Compute the normal equation element AAT[i, j]
            if ((row == col) && (row < wsize)) {
                val = w[row_gid] / y[row_gid];
            } else {
                val = 0.0;
            }

            ind = Anorm_indptr[Lentry];
            ind_end = Anorm_indptr[Lentry + 1];

            for (; ind < ind_end; ind++) {
                xind = Anorm_indices[ind] * gsize + gid;
                val += Adata[Anorm_indptr_i[ind]] * Adata[Anorm_indptr_j[ind]] * x[xind] / z[xind];
            }
            // Now remove the previous L entries
            ind = Ldecomp_indptr[Lentry];
            ind_end = Ldecomp_indptr[Lentry + 1];

            for (; ind < ind_end; ind++) {
                val -= Ldata[Ldecomp_indptr_i[ind] * gsize + gid] * Ldata[Ldecomp_indptr_j[ind] * gsize + gid];
            }

            if (row == col) {
                val = sqrt(fabs(val));
            } else {
                val = val / Ldata[Ldiag_indptr[col]*gsize + gid];
            }
            Ldata[Lentry * gsize + gid] = val;
            Lentry++;
        }
    }
}

__kernel void cholesky_solve(
    uint Asize,
    __constant uint* Lindptr,
    __constant uint* Lindices,
    __constant uint* LTindptr,
    __constant uint* LTindices,
    __constant uint* LTmap,
    __global REAL* restrict Ldata,
    __global REAL* restrict b,
    __global REAL* restrict x
) {
  /* Solve a system Ax = b for x given the decomposition of A as L.

  L is a lower triangular matrix. Entries are stored such that the lth
  entry of L is the i(i + 1)/2 + j entry in dense i, j  coordinates.
  */
  int i, j, jk, jkk;
  uint gid = get_global_id(0);
  uint gsize = get_global_size(0);

  // Forward substitution
  for (i=0; i<Asize; i++) {

    x[i*gsize+gid] = b[i*gsize+gid];

    jk = Lindptr[i];
    j = Lindices[jk];

    while (j < i) {
      x[i*gsize+gid] -= x[j*gsize+gid]*Ldata[jk*gsize+gid];
      jk += 1;
      j = Lindices[jk];
    }
    // jk should now point to the (i, i) entry.
    x[i*gsize+gid] /= Ldata[jk*gsize+gid];
  }

  // Backward substitution
  for (i=Asize-1; i>=0; i--) {
    ///printf("%d %d\n", i, Asize);

    jk = LTindptr[i]+1;
    jkk = LTindptr[i+1];
    j = LTindices[jk];

    while(jk < jkk) {
        x[i*gsize+gid] -= x[j*gsize+gid]*Ldata[LTmap[jk]*gsize+gid];
        jk += 1;
        j = LTindices[jk];
    }

    jk = Lindptr[i+1]-1;
    x[i*gsize+gid] /= Ldata[jk*gsize+gid];
  }
}

__kernel void normal_eqn_step(
    matrix(A),  // Sparse A matrix
    matrix(AT),  // Sparse transpose of A matrix
    __constant uint* Anorm_indptr,
    __constant uint* Anorm_indptr_i,
    __constant uint* Anorm_indptr_j,
    __constant uint* Anorm_indices,
    __constant uint* Ldecomp_indptr,
    __constant uint* Ldecomp_indptr_i,
    __constant uint* Ldecomp_indptr_j,
    __constant uint* Lindptr,
    __constant uint* Ldiag_indptr,
    __constant uint* Lindices,
    __constant uint* LTindptr,
    __constant uint* LTindices,
    __constant uint* LTmap,
    __global REAL* restrict Ldata,
    __global REAL* restrict x,
    __global REAL* restrict z,
    __global REAL* restrict y,
    __global REAL* restrict w,
    uint wsize,
    __global REAL* restrict b,
    __global REAL* restrict c,
    REAL delta,
    __global REAL* restrict dx,
    __global REAL* restrict dz,
    __global REAL* restrict dy,
    __global REAL* restrict dw,
    __global REAL* restrict tmp,
    __global REAL* restrict tmp2,
    __global uint* restrict status
) {
    /* Perform a single step of the path-following algorithm.

    */
    uint gid = get_global_id(0);
    uint gsize = get_global_size(0);

    // printf("%d %d", gid, wsize);

    // Compute feasibilities
    REAL normr = primal_feasibility(Aindptr, Aindices, Adata, Asize, ATsize, x, w, wsize, b);
    REAL norms = dual_feasibility(ATindptr, ATindices, ATdata, ATsize, Asize, y, c, z);
    // Compute optimality
    REAL gamma = dot_product(z, x, ATsize) + dot_product(w, y, wsize);
    REAL mu = delta * gamma / (ATsize + wsize);
    // update relative feasibility tolerance
    gamma = gamma / (1 + vector_norm(x, ATsize) + vector_norm(y, Asize));

    // #ifdef DEBUG_GID
    // if (gid == DEBUG_GID) {
    //    printf("%d %d norm-r: %g, norm-s: %g, gamma: %g\n", gid, wsize, normr, norms, gamma);
    // }
    // #endif
    if ((normr < EPS) && (norms < EPS) && (gamma < EPS)) {
        // Feasible and optimal; no further work!
        status[gid] = 0;
        return;
    }

    // Solve normal equations
    //   1. Calculate the RHS (into tmp2)
    normal_eqn_rhs(
        Aindptr, Aindices, Adata, Asize, ATindptr, ATindices, ATdata, ATsize,
        x, z, y, b, c, mu, wsize, tmp, tmp2
    );

    //   2. Compute decomposition of normal matrix
    normal_matrix_cholesky_decomposition(
        Adata,
        Asize,
        Anorm_indptr,
        Anorm_indptr_i,
        Anorm_indptr_j,
        Anorm_indices,
        Ldecomp_indptr,
        Ldecomp_indptr_i,
        Ldecomp_indptr_j,
        x,
        z,
        y,
        w,
        wsize,
        Lindptr,
        Ldiag_indptr,
        Lindices,
        Ldata
    );

    //   3. Solve system directly
    cholesky_solve(
        Asize,
        Lindptr,
        Lindices,
        LTindptr,
        LTindices,
        LTmap,
        Ldata,
        tmp2,
        dy
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

    status[gid] = 1;
}

__kernel void normal_eqn_init(
    uint Asize,
    uint ATsize,
    __global REAL* restrict x,
    __global REAL* restrict z,
    __global REAL* restrict y,
    __global REAL* restrict w,
    uint wsize
) {
    vector_set(x, 1.0, ATsize);
    vector_set(z, 1.0, ATsize);
    vector_set(y, 1.0, Asize);
    vector_set(w, 1.0, wsize);
}
