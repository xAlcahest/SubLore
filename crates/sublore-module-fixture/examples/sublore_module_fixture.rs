//! The good fixture's exported symbols. The logic is in the crate beside this file; only the two
//! `#[no_mangle]` exports live here, because an example is the target shape the workspace gate
//! rebuilds and a `cdylib` lib is not.
use sublore_module_api::{SubloreHost, SubloreModule};

#[no_mangle]
pub extern "C" fn sublore_module_abi() -> u64 {
    sublore_module_fixture::abi()
}

/// # Safety
/// The host passes two tables it owns and keeps alive for the call, per section 1.
#[no_mangle]
pub unsafe extern "C" fn sublore_module_load(
    host: *const SubloreHost,
    out: *mut SubloreModule,
) -> i32 {
    unsafe { sublore_module_fixture::load(host, out) }
}
