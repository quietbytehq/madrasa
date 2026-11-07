//! Subject allocation per student (M2 stage 6): which of a class's subjects a student takes.
//!
//! A class subject is compulsory unless it is marked elective. A student with no allocation
//! takes every subject of the class, exactly as before allocation existed. A student with an
//! allocation takes the class's compulsory subjects plus the allocated electives; allocated
//! subjects that are no longer taught in the class are ignored.

use uuid::Uuid;

/// A subject of a class and whether it is an elective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassSubject {
    pub subject_id: Uuid,
    pub is_elective: bool,
}

/// The subjects a student takes, in the class's subject order.
pub fn effective_subjects(class_subjects: &[ClassSubject], allocated: &[Uuid]) -> Vec<Uuid> {
    if allocated.is_empty() {
        return class_subjects.iter().map(|c| c.subject_id).collect();
    }
    class_subjects
        .iter()
        .filter(|c| !c.is_elective || allocated.contains(&c.subject_id))
        .map(|c| c.subject_id)
        .collect()
}

/// Split a cell such as "Physics, Chemistry; Biology" into trimmed, non-empty names.
pub fn split_subject_list(cell: &str) -> Vec<String> {
    cell.split([',', ';', '|', '/', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn class() -> Vec<ClassSubject> {
        vec![
            ClassSubject {
                subject_id: id(1),
                is_elective: false,
            },
            ClassSubject {
                subject_id: id(2),
                is_elective: true,
            },
            ClassSubject {
                subject_id: id(3),
                is_elective: true,
            },
            ClassSubject {
                subject_id: id(4),
                is_elective: false,
            },
        ]
    }

    #[test]
    fn no_allocation_means_every_class_subject() {
        assert_eq!(
            effective_subjects(&class(), &[]),
            vec![id(1), id(2), id(3), id(4)]
        );
    }

    #[test]
    fn allocation_keeps_compulsory_and_chosen_electives_in_class_order() {
        assert_eq!(
            effective_subjects(&class(), &[id(3)]),
            vec![id(1), id(3), id(4)]
        );
        // A chosen subject the class no longer teaches is ignored; a compulsory one is always in.
        assert_eq!(
            effective_subjects(&class(), &[id(9), id(1)]),
            vec![id(1), id(4)]
        );
    }

    #[test]
    fn subject_lists_split_on_common_separators() {
        assert_eq!(
            split_subject_list(" Physics ,Chemistry; Biology / Computer |"),
            vec!["Physics", "Chemistry", "Biology", "Computer"]
        );
        assert!(split_subject_list("  ").is_empty());
    }
}
