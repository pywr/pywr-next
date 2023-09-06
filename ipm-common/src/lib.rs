use nalgebra_sparse::csr::CsrMatrix;
use std::cmp::Ordering;

/// The indices for the LDL decomposition of A*AT
#[derive(Debug)]
pub struct SparseNormalCholeskyIndices {
    pub anorm_indptr: Vec<u32>,
    pub anorm_indptr_i: Vec<u32>,
    pub anorm_indptr_j: Vec<u32>,
    pub anorm_indices: Vec<u32>,
    pub ldecomp_indptr: Vec<u32>,
    pub ldecomp_indptr_i: Vec<u32>,
    pub ldecomp_indptr_j: Vec<u32>,
    pub lindptr: Vec<u32>,
    pub ldiag_indptr: Vec<u32>,
    pub lindices: Vec<u32>,
    pub ltindptr: Vec<u32>,
    pub ltindices: Vec<u32>,
    pub ltmap: Vec<u32>,
}

impl SparseNormalCholeskyIndices {
    pub fn from_matrix<T>(a: &CsrMatrix<T>) -> Self {
        let mut anorm_indptr = vec![0u32];
        let mut anorm_indptr_i = Vec::new();
        let mut anorm_indptr_j = Vec::new();
        let mut anorm_indices = Vec::new();
        let mut ldecomp_indptr = vec![0u32];
        let mut ldecomp_indptr_i: Vec<u32> = Vec::new();
        let mut ldecomp_indptr_j: Vec<u32> = Vec::new();
        // Entries of the L matrix
        let mut lindptr = vec![0u32];
        let mut ldiag_indptr = Vec::new();
        let mut lindices: Vec<u32> = Vec::new();

        for i in 0..a.nrows() {
            for j in 0..=i {
                let i_offset = a.row_offsets()[i];
                let i_row = a.get_row(i).unwrap();
                let i_cols = i_row.col_indices();
                let j_offset = a.row_offsets()[j];
                let j_row = a.get_row(j).unwrap();
                let j_cols = j_row.col_indices();
                let mut non_zero = false;
                {
                    // Search for matching indices in the a matrix for element AAT[i, j]

                    let mut ii = 0usize;
                    let mut jj = 0usize;

                    while (ii < i_cols.len()) && (jj < j_cols.len()) {
                        let ik = i_cols[ii];
                        let jk = j_cols[jj];

                        match ik.cmp(&jk) {
                            Ordering::Equal => {
                                anorm_indptr_i.push((i_offset + ii) as u32);
                                anorm_indptr_j.push((j_offset + jj) as u32);
                                anorm_indices.push(ik as u32);
                                non_zero = true;
                                ii += 1;
                                jj += 1;
                            }
                            Ordering::Less => ii += 1,
                            Ordering::Greater => jj += 1,
                        }
                    }
                }

                // Now search for matching indices for the L[i, k]*L[j, k]
                let mut ii = lindptr[i] as usize;
                let mut jj = lindptr[j] as usize;
                let ii_max = lindices.len();

                let jj_max = if i == j { ii_max } else { lindptr[j + 1] as usize };

                while (ii < ii_max) && (jj < jj_max) {
                    let ik = lindices[ii];
                    let jk = lindices[jj];

                    match ik.cmp(&jk) {
                        Ordering::Equal => {
                            ldecomp_indptr_i.push(ii.try_into().expect("L decomposition index to overflow."));
                            ldecomp_indptr_j.push(jj.try_into().expect("L decomposition index to overflow."));
                            non_zero = true;
                            ii += 1;
                            jj += 1;
                        }
                        Ordering::Less => ii += 1,
                        Ordering::Greater => jj += 1,
                    }
                }

                if non_zero {
                    anorm_indptr.push(anorm_indptr_i.len() as u32);
                    ldecomp_indptr.push(ldecomp_indptr_i.len() as u32);
                    lindices.push(j as u32);
                }
                if i == j {
                    ldiag_indptr.push(lindices.len() as u32 - 1)
                }
            }
            lindptr.push(lindices.len() as u32)
        }

        let lvalues = vec![1.0; lindices.len()];
        let lower = CsrMatrix::try_from_csr_data(
            a.nrows(),
            a.nrows(),
            lindptr.iter().map(|r| *r as usize).collect::<Vec<_>>(),
            lindices.iter().map(|r| *r as usize).collect::<Vec<_>>(),
            lvalues,
        )
        .expect("Failed to create CSR data for Cholesky decomposition.");
        let lower_t = lower.transpose();

        let mut ltmap: Vec<_> = (0..lower.col_indices().len()).map(|i| i as u32).collect();
        ltmap.sort_by_key(|&i| lower.col_indices()[i as usize]);

        Self {
            anorm_indptr,
            anorm_indptr_i,
            anorm_indptr_j,
            anorm_indices,
            ldecomp_indptr,
            ldecomp_indptr_i,
            ldecomp_indptr_j,
            lindptr,
            ldiag_indptr,
            lindices,
            ltindptr: lower_t.row_offsets().iter().map(|r| *r as u32).collect::<Vec<_>>(),
            ltindices: lower_t.col_indices().iter().map(|r| *r as u32).collect::<Vec<_>>(),
            ltmap,
        }
    }
}
