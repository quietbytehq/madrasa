//! Madrasa domain logic. Pure: no I/O, no database, no network.
//!
//! Everything here is shared by the API, the desktop shell and the demo generator, and is
//! exhaustively unit tested. See `docs/spec.md` for the rules these modules implement.

pub mod admissions;
pub mod allocation;
pub mod api_keys;
pub mod attendance;
pub mod backup;
pub mod boards;
pub mod consents;
pub mod directory;
pub mod edition;
pub mod exams;
pub mod fees;
pub mod hlc;
pub mod hostel;
pub mod ids;
pub mod imports;
pub mod inventory;
pub mod leave;
pub mod library;
pub mod locale;
pub mod password;
pub mod periods;
pub mod permissions;
pub mod secrets;
pub mod session_token;
pub mod students;
pub mod subjects;
pub mod sync;
pub mod transport;
#[rustfmt::skip]
pub mod timetable;

pub use edition::{Edition, Feature};
pub use permissions::{Permission, RoleCode};
