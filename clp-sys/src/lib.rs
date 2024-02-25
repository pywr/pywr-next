#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!("bindings.rs");

#[cfg(test)]
mod tests {
    use super::{Clp_Version, Clp_deleteModel, Clp_newModel};

    #[test]
    fn test_clp_version() {
        unsafe {
            let c_buf = Clp_Version();
            let c_str = std::ffi::CStr::from_ptr(c_buf);
            let version = c_str.to_str().unwrap();
            println!("{}", version);
        }
    }

    #[test]
    fn test_model() {
        unsafe {
            let model = Clp_newModel();
            Clp_deleteModel(model);
        }
    }
}
