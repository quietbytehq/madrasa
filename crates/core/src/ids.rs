//! Identifier generation. Every primary key in Madrasa is a UUID v7 (time-ordered), generated
//! by the application so that offline clients and the future sync engine never collide.

use uuid::Uuid;

/// New time-ordered UUID v7.
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_v7_and_increase_over_time() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.get_version_num(), 7);
        assert_eq!(b.get_version_num(), 7);
        assert_ne!(a, b);
        // v7 ids generated later never sort before earlier ones (same millisecond included,
        // thanks to the counter bits in the uuid crate's implementation).
        assert!(b > a);
    }
}
