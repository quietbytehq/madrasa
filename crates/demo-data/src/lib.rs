//! Demo data: a fake school with ~1,200 students, ~120 staff and classes K–12, generated from
//! a fixed seed so output is reproducible. Everything is visibly marked DEMO. Nothing here ever
//! runs on its own: loading requires an explicit confirmation and refuses when the database
//! already holds a real (non-demo) school.

pub mod generate;
pub mod load;

pub use generate::{generate, DemoOptions, DemoSchool};
pub use load::{load, write_json, LoadError, LoadOptions, LoadReport};

/// Prefix on every generated school name and username so demo data is never mistaken for real.
pub const DEMO_MARK: &str = "DEMO";
/// Password shared by every demo account. Documented on purpose; demo installs are not secure.
pub const DEMO_PASSWORD: &str = "demo-password-1";
