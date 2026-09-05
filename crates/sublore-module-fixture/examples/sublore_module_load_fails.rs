//! The handshake agrees and the load refuses. The loader keeps the module's own code rather than
//! inventing one, because section 3.5 says the user is shown the module's detail.
use sublore_module_api::{SubloreHost, SubloreModule, SUBLORE_ABI_VERSION, SUBLORE_ERR_STORAGE};

#[no_mangle]
pub extern "C" fn sublore_module_abi() -> u64 {
    SUBLORE_ABI_VERSION
}

/// # Safety
/// The host passes two tables it owns and keeps alive for the call, per section 1.
#[no_mangle]
pub unsafe extern "C" fn sublore_module_load(
    _host: *const SubloreHost,
    _out: *mut SubloreModule,
) -> i32 {
    SUBLORE_ERR_STORAGE
}
