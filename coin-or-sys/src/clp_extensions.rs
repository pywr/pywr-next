unsafe extern "C" {
    pub fn PywrClp_whatsChanged(model: *const Clp_Simplex) -> std::os::raw::c_int;

    pub fn PywrClp_setWhatsChanged(model: *mut Clp_Simplex, value: std::os::raw::c_int);

    pub fn PywrClp_setUnchangedFlags(model: *mut Clp_Simplex, value: std::os::raw::c_int);

    pub fn PywrClp_dualWithOptions(
        model: *mut Clp_Simplex,
        if_values_pass: std::os::raw::c_int,
        start_finish_options: std::os::raw::c_int,
    ) -> std::os::raw::c_int;
}
