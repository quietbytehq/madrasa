//! Madrasa domain logic. Pure: no I/O, no database, no network.
//!
//! Everything here is shared by the API, the desktop shell and the demo generator, and is
//! exhaustively unit tested. See `docs/spec.md` for the rules these modules implement.

pub mod attendance;
pub mod backup;
pub mod edition;
pub mod exams;
pub mod fees;
pub mod hlc;
pub mod ids;
pub mod imports;
pub mod password;
pub mod permissions;
pub mod session_token;
pub mod students;

pub use edition::{Edition, Feature};
pub use permissions::{Permission, RoleCode};
