//! A module built for the next major. The loader must refuse it and say both numbers.
//!
//! It exports the handshake and nothing else on purpose: `sublore_module_load` is resolved only
//! after the handshake agrees, so a loader that reached for it here would be reaching too early.
use sublore_module_api::{SUBLORE_ABI_MAJOR, SUBLORE_ABI_MINOR};

#[no_mangle]
pub extern "C" fn sublore_module_abi() -> u64 {
    (((SUBLORE_ABI_MAJOR + 1) as u64) << 32) | SUBLORE_ABI_MINOR as u64
}
