//! A library that is not a Sublore module at all. It exports one unrelated symbol, so that it is a
//! real loadable object rather than an empty file, and the loader has to refuse it for the reason
//! it claims to: the handshake symbol is absent.
#[no_mangle]
pub extern "C" fn something_else_entirely() -> u64 {
    0
}
