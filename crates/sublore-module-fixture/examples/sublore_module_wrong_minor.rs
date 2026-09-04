//! A module built against slots this host does not have. Section 3.2's rule is asymmetric: a host
//! ahead of a module loads it, a module ahead of its host is refused, because calling a slot the
//! table does not carry is a jump through whatever follows it.
//!
//! It exports the load symbol as well as the handshake, and that is the point rather than an
//! accident. Without it, flipping the comparison in the loader would refuse this file anyway, as
//! not a module, and the check would pass while the rule it names was inverted. A mutation showed
//! exactly that on 2026-09-04.
use sublore_module_api::{
    SubloreHost, SubloreModule, SUBLORE_ABI_MAJOR, SUBLORE_ABI_MINOR, SUBLORE_MODULE_SIZE,
    SUBLORE_OK,
};

#[no_mangle]
pub extern "C" fn sublore_module_abi() -> u64 {
    ((SUBLORE_ABI_MAJOR as u64) << 32) | (SUBLORE_ABI_MINOR + 1) as u64
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
    // Claims the same minor its handshake did. A loader that let the handshake through must still
    // stop here, which is the second half of the same rule.
    let mut table = SubloreModule::empty();
    table.size = SUBLORE_MODULE_SIZE;
    table.minor = SUBLORE_ABI_MINOR + 1;
    unsafe { out.write(table) };
    SUBLORE_OK
}
