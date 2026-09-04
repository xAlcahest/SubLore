//! A module whose two numbers disagree: the handshake claims a revision above the host's and the
//! table it fills claims the host's own.
//!
//! It exists because the loader checks the minor twice, once at the handshake and once on the
//! table, and defence in depth means a mutation of either one alone is masked by the other. This
//! is the only shape the first check can refuse and the second cannot, so it is what holds that
//! check up. Found needed by mutation on 2026-09-04, when flipping the handshake comparison left
//! every check green.
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
    let mut table = SubloreModule::empty();
    table.size = SUBLORE_MODULE_SIZE;
    // The lie: the handshake said one revision above, and this says the host's own.
    table.minor = SUBLORE_ABI_MINOR;
    unsafe { out.write(table) };
    SUBLORE_OK
}
