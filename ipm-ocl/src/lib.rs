use ipm_common::SparseNormalCholeskyIndices;
use log::debug;
use nalgebra_sparse::csr::CsrMatrix;
use std::num::NonZeroUsize;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Tolerances {
    pub primal_feasibility: f64,
    pub dual_feasibility: f64,
    pub optimality: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            primal_feasibility: 1e-6,
            dual_feasibility: 1e-6,
            optimality: 1e-6,
            optimality: 1e-6,
        }
    }
}

pub trait GetClProgram {
    fn get_cl_program(
        context: &ocl::Context,
        device: &ocl::Device,
        tolerances: &Tolerances,
    ) -> ocl::Result<ocl::Program>;
}

impl GetClProgram for f64 {
    fn get_cl_program(
        context: &ocl::Context,
        device: &ocl::Device,
        tolerances: &Tolerances,
    ) -> ocl::Result<ocl::Program> {
        let src = [include_str!("common.cl"), include_str!("path_following_direct.cl")].join("\n");

        // TODO this was done with build argument before "-DREAL=double". Need to do a proper search
        // on the ocl docs about whether this is possible.
        let src = src
            .replace("REAL", "double")
            .replace("EPS_PRIMAL_FEASIBILITY", &format!("{}", tolerances.primal_feasibility))
            .replace("EPS_DUAL_FEASIBILITY", &format!("{}", tolerances.dual_feasibility))
            .replace("EPS_OPTIMALITY", &format!("{}", tolerances.optimality));

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
    fn get_cl_program(
        context: &ocl::Context,
        device: &ocl::Device,
        tolerances: &Tolerances,
    ) -> ocl::Result<ocl::Program> {
        let src = [include_str!("common.cl"), include_str!("path_following_direct.cl")].join("\n");

        // TODO this was done with build argument before "-DREAL=float". Need to do a proper search
        // on the ocl docs about whether this is possible.
        let src = src
            .replace("REAL", "float")
            .replace("EPS_PRIMAL_FEASIBILITY", &format!("{}", tolerances.primal_feasibility))
            .replace("EPS_DUAL_FEASIBILITY", &format!("{}", tolerances.dual_feasibility))
            .replace("EPS_OPTIMALITY", &format!("{}", tolerances.optimality));

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
        debug!("Number of ANorm indptr size: {}", indices.anorm_indptr.len());
        debug!("Number of ANorm indptr_i: {}", indices.anorm_indptr_i.len());
        debug!("Number of ANorm indptr_i: {}", indices.anorm_indptr_j.len());
        debug!("Number of ANorm ldecomp_indptr: {}", indices.ldecomp_indptr.len());
        debug!("Number of ANorm ldecomp_indptr_i: {}", indices.ldecomp_indptr_i.len());
        debug!("Number of ANorm ldecomp_indptr_j: {}", indices.ldecomp_indptr_j.len());

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
    status_buffer: ocl::Buffer<u8>,
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

        let mut normal_indices = SparseNormalCholeskyIndices::from_matrix(a);

        // Really simple models may not have any entries in these arrays.
        // However, the OCL buffers need to be at-least size 1. Add a temporary value as a work around.
        if normal_indices.ldecomp_indptr_i.is_empty() {
            normal_indices.ldecomp_indptr_i.push(0)
        }
        if normal_indices.ldecomp_indptr_j.is_empty() {
            normal_indices.ldecomp_indptr_j.push(0)
        }

        let normal_buffers = SparseNormalCholeskyClBuffers::from_indices(&normal_indices, queue)?;

        debug!("Number of L indices: {}", normal_indices.lindices.len());

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

        let status_buffer = ocl::Buffer::<u8>::builder()
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
    buffers: PathFollowingDirectClBuffers<T>,
    kernel_normal_init: ocl::Kernel,
    kernel_normal_eq_step: ocl::Kernel,
    // kernel_normal_eq_solve: ocl::Kernel,
    solution: Vec<T>,
    status: Vec<u8>,
}

impl<T> PathFollowingDirectClSolver<T>
where
    T: ocl::OclPrm + GetClProgram,
{
    pub fn from_data(
        queue: &ocl::Queue,
        program: &ocl::Program,
        num_rows: usize,
        num_cols: usize,
        row_offsets: Vec<usize>,
        col_indices: Vec<usize>,
        values: Vec<T>,
        num_inequality_constraints: u32,
        num_lps: u32,
    ) -> ocl::Result<Self> {
        let a = CsrMatrix::try_from_csr_data(num_rows, num_cols, row_offsets, col_indices, values)
            .expect("Failed to create matrix from given data");

        let buffers = PathFollowingDirectClBuffers::from_data(&a, num_lps, queue)?;

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
        let status = vec![0u8; num_lps as usize];

        Ok(Self {
            buffers,
            kernel_normal_init,
            kernel_normal_eq_step,
            solution,
            status,
        })
    }

    pub fn solve(&mut self, queue: &ocl::Queue, b: &[T], c: &[T], max_iterations: NonZeroUsize) -> ocl::Result<&[T]> {
        // Copy b & c to the device
        self.buffers.b_buffer.write(b).enq()?;
        self.buffers.c_buffer.write(c).enq()?;

        unsafe {
            self.kernel_normal_init.enq()?;
        }

        // self.buffers.path_buffers.x.read(&mut self.solution).enq()?;
        // self.queue.finish()?;
        let mut iter = 0;

        let last_iteration = loop {
            if iter >= max_iterations.get() {
                break None;
            }

            unsafe {
                self.kernel_normal_eq_step.enq()?;
            }

            self.buffers.status_buffer.read(&mut self.status).enq()?;
            // self.queue.finish()?;

            let all_complete: bool = self.status.iter().all(|&s| s == 0);
            if all_complete {
                break Some(iter);
            }

            iter += 1
        };

        if last_iteration.is_none() {
            panic!("Interior point method failed to converged all scenarios.")
        }

        // println!("Finished after iterations: {}", last_iteration);
        self.buffers.path_buffers.x.read(&mut self.solution).enq()?;
        queue.finish()?;

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

        let tolerances = Tolerances::default();
        let _ = f64::get_cl_program(&context, &device, &tolerances).unwrap();
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
