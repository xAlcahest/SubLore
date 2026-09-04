//! A module whose table declares a size this build does not compile to. Section 3.3 calls that
//! version skew the minor number failed to describe, which is the mistake a human makes rather
//! than one a compiler makes, and it is the only thing the size field is there to catch.
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
    table.size = SUBLORE_MODULE_SIZE + 8;
    table.minor = SUBLORE_ABI_MINOR;
    unsafe { out.write(table) };
    SUBLORE_OK
}
