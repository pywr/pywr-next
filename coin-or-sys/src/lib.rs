#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub mod clp {
    include!("clp.rs");
}

pub mod cbc {
    include!("cbc.rs");
}

#[cfg(test)]
mod tests {
    use super::cbc::{Cbc_deleteModel, Cbc_getVersion, Cbc_newModel};
    use super::clp::{Clp_Version, Clp_deleteModel, Clp_newModel};

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
    fn test_clp_model() {
        unsafe {
            let model = Clp_newModel();
            Clp_deleteModel(model);
        }
    }

    #[test]
    fn test_cbc_version() {
        unsafe {
            let c_buf = Cbc_getVersion();
            let c_str = std::ffi::CStr::from_ptr(c_buf);
            let version = c_str.to_str().unwrap();
            println!("{}", version);
        }
    }

    #[test]
    fn test_cbc_model() {
        unsafe {
            let model = Cbc_newModel();
            Cbc_deleteModel(model);
        }
    }
}
