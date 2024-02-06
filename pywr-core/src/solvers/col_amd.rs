/// Compute approximate minimum degree (AMD) ordering using the [`colamd`] crate.
///
/// [`colamd`] computes a column ordering to aid solving systems of the form `(A'A)x = b`.
/// However, we are solving the normal equations for `dy` which are of the form `(AA')x = b`
/// therefore we apply re-ordering to the rows of `A`.
pub fn compute_ordering(
    num_rows: usize,
    num_cols: usize,
    row_offsets: Vec<usize>,
    col_indices: Vec<usize>,
    values: Vec<f64>,
) -> (Vec<usize>, Vec<usize>, Vec<f64>, Vec<usize>, Vec<usize>) {
    use nalgebra_sparse::{CooMatrix, CscMatrix, CsrMatrix};

    let num_nnz = values.len();

    println!("num_rows: {}", num_rows);
    println!("num_cols: {}", num_cols);
    println!("num_nnz: {}", num_nnz);
    println!("row_offsets: {:?}", row_offsets);
    println!("col_indices: {:?}", col_indices);

    let csr_matrix = CsrMatrix::try_from_csr_data(num_rows, num_cols, row_offsets, col_indices, values)
        .expect("Failed to create CSR matrix.");

    let csc_matrix = CscMatrix::from(&csr_matrix);

    let a_len = colamd::recommended(num_nnz as i32, num_rows as i32, num_cols as i32) as usize;
    println!("a_len: {}", a_len);
    // Compute the ordering.
    let mut a_i: Vec<i32> = csc_matrix.row_indices().iter().map(|&i| i as i32).collect();
    println!("a_i: {:?}", a_i);
    // Make a_i as long as a_len
    if a_i.len() < a_len {
        a_i.extend(vec![0; a_len - a_i.len()])
    }

    let mut perm: Vec<i32> = csc_matrix.col_offsets().iter().map(|&i| i as i32).collect();
    println!("p: {:?}", perm);

    let mut stats = [0; 20];

    let result = colamd::colamd(
        num_rows as i32,
        num_cols as i32,
        a_len as i32,
        &mut a_i,
        &mut perm,
        None,
        &mut stats,
    );

    if !result {
        panic!("Approximate minimum degree ordering failed with status: {}", stats[3],);
    }

    // Convert permutation to usize. `perm[k] = j` means that row and column `j` of `A`.
    let perm: Vec<usize> = perm[..num_cols].into_iter().map(|&i| i as usize).collect();
    println!("Col permutation: {:#?}", perm);

    // Compute inverse permutation
    let mut inv_perm: Vec<_> = (0..perm.len()).collect();
    inv_perm.sort_by_key(|&k| perm[k]);

    // Apply the permutation to create a new matrix
    let coo_matrix = CooMatrix::from(&csr_matrix);

    let (row_indices, col_indices, values) = coo_matrix.disassemble();
    // Map the row indices to the permuted positions
    let col_indices: Vec<_> = col_indices.into_iter().map(|r| inv_perm[r]).collect();

    let coo_matrix = CooMatrix::try_from_triplets(num_rows, num_cols, row_indices, col_indices, values)
        .expect("Failed to construct permuted COO matrix.");

    // Finally turn it into a row ordered sparse matrix;
    let csr_matrix = CsrMatrix::from(&coo_matrix);

    let (row_offsets, col_indices, values) = csr_matrix.disassemble();

    println!("row_offsets: {:?}", row_offsets);
    println!("col_indices: {:?}", col_indices);

    (row_offsets, col_indices, values, perm, inv_perm)
}
