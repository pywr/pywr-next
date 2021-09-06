/// Allocate memory for parameter dependencies
#[no_mangle]
pub fn alloc(len: u32) -> *mut f64 {
    let mut buf: Vec<f64> = Vec::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr();
    // Ensure the memory is not deallocated at the end of this function.
    std::mem::forget(buf);
    return ptr;
}

/// Set an entry in a vector
///
/// This will only work if the ptr has been created with Rust's Vec type
/// (i.e. use the alloc function in this module to get the ptr).
#[no_mangle]
pub fn set(ptr: *mut f64, len: u32, idx: u32, val: f64) {
    // Re-create the vector from the raw ptr
    let mut data: Vec<f64> = unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) };
    // Update the entry
    data[idx as usize] = val;
    // Ensure the memory is not deallocated at the end of this function.
    std::mem::forget(data);
}

/// Compute the sum of the dependent parameters
///
/// This will only work if the ptr has been created with Rust's Vec type
/// (i.e. use the alloc function in this module to get the ptr).
#[no_mangle]
pub fn value(ptr: *mut f64, len: u32) -> f64 {
    if ptr.is_null() {
        return 0.0;
    }

    let data: Vec<f64> = unsafe { Vec::from_raw_parts(ptr, len as usize, len as usize) };

    // Calculate the sum of the dependent parameters.
    let result = match data.len() {
        0 => 0.0,
        _ => data.iter().sum::<f64>(),
    };
    // Ensure the memory is not deallocated at the end of this function.
    std::mem::forget(data);

    result
}
