//! A module that is honest at the handshake and lies in the table: the two numbers disagree the
//! other way round from `sublore_module_lying_minor`.
//!
//! It holds up the loader's second minor check, which the handshake would otherwise reach first in
//! every case and leave undefended. A module is code this process did not compile, so what it says
//! about itself is checked at both points it says it.
use sublore_module_api::{
    SubloreHost, SubloreModule, SUBLORE_ABI_MINOR, SUBLORE_ABI_VERSION, SUBLORE_MODULE_SIZE,
    SUBLORE_OK,
};

#[no_mangle]
pub extern "C" fn sublore_module_abi() -> u64 {
    SUBLORE_ABI_VERSION
}

/// # Safety
/// The host passes two tables it owns and keeps alive for the call, per section 1.
#[no_mangle]
pub unsafe extern "C" fn sublore_module_load(
    _host: *const SubloreHost,
    out: *mut SubloreModule,
) -> i32 {
    if out.is_null() {
        return sublore_module_api::SUBLORE_ERR_BAD_STRING;
    }
    let mut table = SubloreModule::empty();
    table.size = SUBLORE_MODULE_SIZE;
    table.minor = SUBLORE_ABI_MINOR + 1;
    unsafe { out.write(table) };
    SUBLORE_OK
}
