//! Licensed under the GNU GPL v3 or later, with the section 7 additional permission for modules
//! loaded through `sublore-module-api`. See LICENSE at the root of the repository.

//! Editing a lossless subtitle document: every change is a byte splice that is re-parsed and
//! verified before it is kept. See BACKLOG.md M2.1-M2.3.

pub mod diff;
pub mod error;
pub mod history;
pub mod plan;
pub mod session;
pub mod splice;
pub mod verify;
