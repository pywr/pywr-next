use nalgebra_sparse::csr::CsrMatrix;
use std::cmp::Ordering;

pub trait GetClProgram {
    fn get_cl_program(context: &ocl::Context, device: &ocl::Device) -> ocl::Result<ocl::Program>;
}

impl GetClProgram for f64 {
    fn get_cl_program(context: &ocl::Context, device: &ocl::Device) -> ocl::Result<ocl::Program> {
        let src = [include_str!("common.cl"), include_str!("path_following_direct.cl")].join("\n");

        // TODO this was done with build argument before "-DREAL=double". Need to do a proper search
        // on the ocl docs about whether this is possible.
        let src = src.replace("REAL", "double").replace("EPS", "1e-3");

        let opts = std::env::var("CLIPM_COMPILER_OPTS").unwrap_or_else(|_| "".to_string());
        let program = ocl::Program::builder()
            .cmplr_opt(opts)
            .devices(device)
            .src(src)
            .build(context)?;

        Ok(program)
    }
}

impl GetClProgram for f32 {
    fn get_cl_program(context: &ocl::Context, device: &ocl::Device) -> ocl::Result<ocl::Program> {
        let src = [include_str!("common.cl"), include_str!("path_following_direct.cl")].join("\n");

        // TODO this was done with build argument before "-DREAL=double". Need to do a proper search
        // on the ocl docs about whether this is possible.
        let src = src.replace("REAL", "float").replace("EPS", "1e-2");

        let opts = std::env::var("CLIPM_COMPILER_OPTS").unwrap_or_else(|_| "".to_string());
        let program = ocl::Program::builder()
            .cmplr_opt(opts)
            .devices(device)
            .src(src)
            .build(context)?;

        Ok(program)
    }
}

struct SparseMatrixBuffers<T>
where
    T: ocl::OclPrm,
{
    values: ocl::Buffer<T>,
    row_offsets: ocl::Buffer<u32>,
    col_indices: ocl::Buffer<u32>,
}

impl<T> SparseMatrixBuffers<T>
where
    T: ocl::OclPrm,
{
    fn from_sparse_matrix(a: &CsrMatrix<T>, queue: &ocl::Queue) -> ocl::Result<Self> {
        // Copy the data from the host sparse matrix to the device buffers
        let values = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(a.values())
            .len(a.values().len())
            .build()?;

        let row_offsets = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .copy_host_slice(
                a.row_offsets()
                    .into_iter()
                    .map(|r| *r as u32)
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .len(a.row_offsets().len())
            .build()?;

        let col_indices = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .copy_host_slice(
                a.col_indices()
                    .into_iter()
                    .map(|r| *r as u32)
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
            .len(a.col_indices().len())
            .build()?;

        Ok(Self {
            values,
            row_offsets,
            col_indices,
        })
    }
}

struct SparseNormalCholeskyClBuffers {
    anorm_indptr: ocl::Buffer<u32>,
    anorm_indptr_i: ocl::Buffer<u32>,
    anorm_indptr_j: ocl::Buffer<u32>,
    anorm_indices: ocl::Buffer<u32>,
    ldecomp_indptr: ocl::Buffer<u32>,
    ldecomp_indptr_i: ocl::Buffer<u32>,
    ldecomp_indptr_j: ocl::Buffer<u32>,
    lindptr: ocl::Buffer<u32>,
    ldiag_indptr: ocl::Buffer<u32>,
    lindices: ocl::Buffer<u32>,
    ltindptr: ocl::Buffer<u32>,
    ltindices: ocl::Buffer<u32>,
    ltmap: ocl::Buffer<u32>,
}

impl SparseNormalCholeskyClBuffers {
    fn from_indices(indices: &SparseNormalCholeskyIndices, queue: &ocl::Queue) -> ocl::Result<Self> {
        println!("Number of ANorm indptr size: {}", indices.anorm_indptr.len());
        println!("Number of ANorm indptr_i: {}", indices.anorm_indptr_i.len());
        println!("Number of ANorm indptr_i: {}", indices.anorm_indptr_j.len());
        println!("Number of ANorm ldecomp_indptr: {}", indices.ldecomp_indptr.len());
        println!("Number of ANorm ldecomp_indptr_i: {}", indices.ldecomp_indptr_i.len());
        println!("Number of ANorm ldecomp_indptr_j: {}", indices.ldecomp_indptr_j.len());

        // Copy the data from the host sparse matrix to the device buffers
        let anorm_indptr = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.anorm_indptr.as_slice())
            .len(indices.anorm_indptr.len())
            .build()?;

        let anorm_indptr_i = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.anorm_indptr_i.as_slice())
            .len(indices.anorm_indptr_i.len())
            .build()?;

        let anorm_indptr_j = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.anorm_indptr_j.as_slice())
            .len(indices.anorm_indptr_j.len())
            .build()?;

        let anorm_indices = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.anorm_indices.as_slice())
            .len(indices.anorm_indices.len())
            .build()?;

        let ldecomp_indptr = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ldecomp_indptr.as_slice())
            .len(indices.ldecomp_indptr.len())
            .build()?;

        let ldecomp_indptr_i = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ldecomp_indptr_i.as_slice())
            .len(indices.ldecomp_indptr_i.len())
            .build()?;

        let ldecomp_indptr_j = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ldecomp_indptr_j.as_slice())
            .len(indices.ldecomp_indptr_j.len())
            .build()?;

        let lindptr = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.lindptr.as_slice())
            .len(indices.lindptr.len())
            .build()?;

        let ldiag_indptr = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ldiag_indptr.as_slice())
            .len(indices.ldiag_indptr.len())
            .build()?;

        let lindices = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.lindices.as_slice())
            .len(indices.lindices.len())
            .build()?;

        let ltindptr = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ltindptr.as_slice())
            .len(indices.ltindptr.len())
            .build()?;

        let ltindices = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ltindices.as_slice())
            .len(indices.ltindices.len())
            .build()?;

        let ltmap = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY)
            .copy_host_slice(indices.ltmap.as_slice())
            .len(indices.ltmap.len())
            .build()?;

        Ok(Self {
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
            ltindptr,
            ltindices,
            ltmap,
        })
    }
}

/// The indices for the LDL decomposition of A*AT
#[derive(Debug)]
struct SparseNormalCholeskyIndices {
    anorm_indptr: Vec<u32>,
    anorm_indptr_i: Vec<u32>,
    anorm_indptr_j: Vec<u32>,
    anorm_indices: Vec<u32>,
    ldecomp_indptr: Vec<u32>,
    ldecomp_indptr_i: Vec<u32>,
    ldecomp_indptr_j: Vec<u32>,
    lindptr: Vec<u32>,
    ldiag_indptr: Vec<u32>,
    lindices: Vec<u32>,
    ltindptr: Vec<u32>,
    ltindices: Vec<u32>,
    ltmap: Vec<u32>,
}

impl SparseNormalCholeskyIndices {
    fn from_matrix<T>(a: &CsrMatrix<T>) -> Self
    where
        T: ocl::OclPrm,
    {
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

struct PathBuffers<T>
where
    T: ocl::OclPrm,
{
    x: ocl::Buffer<T>,
    z: ocl::Buffer<T>,
    y: ocl::Buffer<T>,
    w: ocl::Buffer<T>,
}

impl<T> PathBuffers<T>
where
    T: ocl::OclPrm,
{
    fn new(num_rows: u32, num_cols: u32, num_lps: u32, queue: &ocl::Queue) -> ocl::Result<Self> {
        let x = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_cols * num_lps)
            .build()?;
        let z = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_cols * num_lps)
            .build()?;

        let y = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_rows * num_lps)
            .build()?;

        let w = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_rows * num_lps)
            .build()?;

        Ok(Self { x, z, y, w })
    }
}

pub struct PathFollowingDirectClBuffers<T>
where
    T: ocl::OclPrm,
{
    a_buffers: SparseMatrixBuffers<T>,
    at_buffers: SparseMatrixBuffers<T>,
    normal_buffers: SparseNormalCholeskyClBuffers,
    ldata: ocl::Buffer<T>,

    path_buffers: PathBuffers<T>,
    delta_path_buffers: PathBuffers<T>,

    b_buffer: ocl::Buffer<T>,
    c_buffer: ocl::Buffer<T>,
    tmp_buffer: ocl::Buffer<T>,
    rhs_buffer: ocl::Buffer<T>,
    status_buffer: ocl::Buffer<u32>,
}

impl<T> PathFollowingDirectClBuffers<T>
where
    T: ocl::OclPrm,
{
    pub fn from_data(a: &CsrMatrix<T>, num_lps: u32, queue: &ocl::Queue) -> ocl::Result<Self> {
        let num_rows = a.nrows() as u32;
        let num_cols = a.ncols() as u32;

        let a_buffers = SparseMatrixBuffers::from_sparse_matrix(a, queue)?;

        let at = a.transpose();
        let at_buffers = SparseMatrixBuffers::from_sparse_matrix(&at, queue)?;

        let normal_indices = SparseNormalCholeskyIndices::from_matrix(a);
        let normal_buffers = SparseNormalCholeskyClBuffers::from_indices(&normal_indices, queue)?;

        println!("Number of L indices: {}", normal_indices.lindices.len());

        // Require ldata for every LP
        let ldata = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(normal_indices.lindices.len() as u32 * num_lps)
            .build()?;

        // Empty buffer for the "b" and "c" arrays;
        // These buffers are read only by the device but are written from the host ahead of
        // each set of solves.
        let b_buffer = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY & ocl::flags::MEM_HOST_WRITE_ONLY)
            .len(num_rows * num_lps)
            .build()?;

        let c_buffer = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_ONLY & ocl::flags::MEM_HOST_WRITE_ONLY)
            .len(num_cols * num_lps)
            .build()?;

        let path_buffers = PathBuffers::new(num_rows, num_cols, num_lps, queue)?;
        let delta_path_buffers = PathBuffers::new(num_rows, num_cols, num_lps, queue)?;

        // Work buffers
        let tmp_buffer = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_cols * num_lps)
            .build()?;

        let rhs_buffer = ocl::Buffer::<T>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_rows * num_lps)
            .build()?;

        let status_buffer = ocl::Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(num_lps)
            .build()?;

        Ok(Self {
            a_buffers,
            at_buffers,
            normal_buffers,
            ldata,
            path_buffers,
            delta_path_buffers,
            b_buffer,
            c_buffer,
            tmp_buffer,
            rhs_buffer,
            status_buffer,
        })
    }
}

pub struct PathFollowingDirectClSolver<T>
where
    T: ocl::OclPrm,
{
    context: ocl::Context,
    queue: ocl::Queue,
    program: ocl::Program,
    buffers: PathFollowingDirectClBuffers<T>,
    kernel_normal_init: ocl::Kernel,
    kernel_normal_eq_step: ocl::Kernel,
    solution: Vec<T>,
    status: Vec<u32>,
}

impl<T> PathFollowingDirectClSolver<T>
where
    T: ocl::OclPrm + GetClProgram,
{
    pub fn from_data(
        num_rows: usize,
        num_cols: usize,
        row_offsets: Vec<usize>,
        col_indices: Vec<usize>,
        values: Vec<T>,
        num_inequality_constraints: u32,
        num_lps: u32,
    ) -> ocl::Result<Self> {
        let platform = ocl::Platform::default();
        let device = ocl::Device::first(platform).unwrap();
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(device)
            .build()
            .unwrap();
        let program = T::get_cl_program(&context, &device).unwrap();

        let queue = ocl::Queue::new(&context, device, None)?;

        let a = CsrMatrix::try_from_csr_data(num_rows, num_cols, row_offsets, col_indices, values)
            .expect("Failed to create matrix from given data");

        let buffers = PathFollowingDirectClBuffers::from_data(&a, num_lps, &queue)?;

        let kernel_normal_init = ocl::Kernel::builder()
            .program(&program)
            .name("normal_eqn_init")
            .queue(queue.clone())
            .global_work_size(num_lps)
            .arg(num_rows as u32)
            .arg(num_cols as u32)
            .arg(&buffers.path_buffers.x)
            .arg(&buffers.path_buffers.z)
            .arg(&buffers.path_buffers.y)
            .arg(&buffers.path_buffers.w)
            .arg(num_inequality_constraints)
            .build()?;

        let kernel_normal_eq_step = ocl::Kernel::builder()
            .program(&program)
            .name("normal_eqn_step")
            .queue(queue.clone())
            .global_work_size(num_lps)
            // A buffers
            .arg(&buffers.a_buffers.row_offsets)
            .arg(&buffers.a_buffers.col_indices)
            .arg(&buffers.a_buffers.values)
            .arg(num_rows as u32)
            // AT buffers
            .arg(&buffers.at_buffers.row_offsets)
            .arg(&buffers.at_buffers.col_indices)
            .arg(&buffers.at_buffers.values)
            .arg(num_cols as u32)
            // Cholesky buffers
            .arg(&buffers.normal_buffers.anorm_indptr)
            .arg(&buffers.normal_buffers.anorm_indptr_i)
            .arg(&buffers.normal_buffers.anorm_indptr_j)
            .arg(&buffers.normal_buffers.anorm_indices)
            .arg(&buffers.normal_buffers.ldecomp_indptr)
            .arg(&buffers.normal_buffers.ldecomp_indptr_i)
            .arg(&buffers.normal_buffers.ldecomp_indptr_j)
            .arg(&buffers.normal_buffers.lindptr)
            .arg(&buffers.normal_buffers.ldiag_indptr)
            .arg(&buffers.normal_buffers.lindices)
            .arg(&buffers.normal_buffers.ltindptr)
            .arg(&buffers.normal_buffers.ltindices)
            .arg(&buffers.normal_buffers.ltmap)
            .arg(&buffers.ldata)
            // Path variables
            .arg(&buffers.path_buffers.x)
            .arg(&buffers.path_buffers.z)
            .arg(&buffers.path_buffers.y)
            .arg(&buffers.path_buffers.w)
            .arg(num_inequality_constraints)
            .arg(&buffers.b_buffer)
            .arg(&buffers.c_buffer)
            .arg(0.1f32)
            .arg(&buffers.delta_path_buffers.x)
            .arg(&buffers.delta_path_buffers.z)
            .arg(&buffers.delta_path_buffers.y)
            .arg(&buffers.delta_path_buffers.w)
            .arg(&buffers.tmp_buffer)
            .arg(&buffers.rhs_buffer)
            .arg(&buffers.status_buffer)
            .build()?;

        let solution: Vec<T> = vec![T::default(); num_cols * num_lps as usize];
        let status = vec![0u32; num_lps as usize];

        Ok(Self {
            context,
            queue,
            program,
            buffers,
            kernel_normal_init,
            kernel_normal_eq_step,
            solution,
            status,
        })
    }

    pub fn solve(&mut self, b: &[T], c: &[T]) -> ocl::Result<&[T]> {
        // Copy b & c to the device
        self.buffers.b_buffer.write(b).enq()?;

        self.buffers.c_buffer.write(c).enq()?;

        unsafe {
            self.kernel_normal_init.enq()?;
        }

        // self.buffers.path_buffers.x.read(&mut self.solution).enq()?;
        // self.queue.finish()?;

        let mut last_iteration = 0;
        for iter in 0..200 {
            unsafe {
                self.kernel_normal_eq_step.enq()?;
            }

            self.buffers.status_buffer.read(&mut self.status).enq()?;
            // self.queue.finish()?;

            let num_incomplete: u32 = self.status.iter().sum();

            // println!("Number incomplete: {}", num_incomplete);

            if num_incomplete == 0 {
                break;
            }

            last_iteration = iter;
        }

        // println!("Finished after iterations: {}", last_iteration);

        self.buffers.path_buffers.x.read(&mut self.solution).enq()?;
        self.queue.finish()?;

        // panic!("Testing!");

        Ok(self.solution.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra_sparse::CooMatrix;

    #[test]
    fn build_program() {
        let platform = ocl::Platform::default();
        let device = ocl::Device::first(platform).unwrap();
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(device)
            .build()
            .unwrap();
        let p = f64::get_cl_program(&context, &device).unwrap();
    }

    fn test_matrx() -> CsrMatrix<f64> {
        let coo = CooMatrix::try_from_triplets(
            4,
            4,
            vec![0, 1, 2, 3, 1, 2, 3],
            vec![0, 1, 2, 3, 0, 1, 2],
            vec![1.0, 2.0, 3.0, 4.0, 1.0, 1.0, 1.0],
        )
        .unwrap();

        CsrMatrix::from(&coo)
    }

    #[test]
    fn create_path_following_buffers() {
        let platform = ocl::Platform::default();
        let device = ocl::Device::first(platform).unwrap();
        let context = ocl::Context::builder()
            .platform(platform)
            .devices(device)
            .build()
            .unwrap();

        let queue = ocl::Queue::new(&context, device, None).unwrap();

        let a = test_matrx();
        let pf = PathFollowingDirectClBuffers::from_data(&a, 10, &queue).unwrap();
    }
}
