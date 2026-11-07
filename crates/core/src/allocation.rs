//! Allocation screens (M2 stage 7): the section-balancing proposal.
//!
//! Given the sections of a class with their students in roll order, propose moves so that
//! the sizes differ by at most one. Moves are taken from the end of the over-full sections'
//! rolls (the last-admitted students), never touching excluded students, and the proposal
//! is deterministic for the same input. Capacity is not consulted: the office sees the
//! sizes in the preview and decides.

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionRoster {
    pub section_id: Uuid,
    /// Students in roll order.
    pub students: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub student_id: Uuid,
    pub from_section_id: Uuid,
    pub to_section_id: Uuid,
}

/// Target sizes: every section gets `total / n`, and the largest sections (ties by input
/// order) take the remainder, so an already balanced class needs no moves.
fn targets(sizes: &[usize]) -> Vec<usize> {
    let n = sizes.len();
    if n == 0 {
        return Vec::new();
    }
    let total: usize = sizes.iter().sum();
    let base = total / n;
    let extra = total % n;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| sizes[*b].cmp(&sizes[*a]).then(a.cmp(b)));
    let mut out = vec![base; n];
    for i in order.into_iter().take(extra) {
        out[i] = base + 1;
    }
    out
}

/// Propose the moves that balance the sections (sizes differ by at most one afterwards).
pub fn balance(sections: &[SectionRoster], excluded: &[Uuid]) -> Vec<Move> {
    let mut rosters: Vec<Vec<Uuid>> = sections.iter().map(|s| s.students.clone()).collect();
    let sizes: Vec<usize> = rosters.iter().map(|r| r.len()).collect();
    let targets = targets(&sizes);
    let mut moves = Vec::new();
    loop {
        // Most over-full section first (ties by input order), then the most under-full.
        let over = (0..rosters.len())
            .filter(|i| rosters[*i].len() > targets[*i])
            .max_by(|a, b| {
                (rosters[*a].len() - targets[*a])
                    .cmp(&(rosters[*b].len() - targets[*b]))
                    .then(b.cmp(a))
            });
        let under = (0..rosters.len())
            .filter(|i| rosters[*i].len() < targets[*i])
            .max_by(|a, b| {
                (targets[*a] - rosters[*a].len())
                    .cmp(&(targets[*b] - rosters[*b].len()))
                    .then(b.cmp(a))
            });
        let (Some(from), Some(to)) = (over, under) else {
            break;
        };
        let Some(pos) = rosters[from].iter().rposition(|s| !excluded.contains(s)) else {
            // Nothing movable in the fullest section: give up rather than loop.
            break;
        };
        let student = rosters[from].remove(pos);
        rosters[to].push(student);
        moves.push(Move {
            student_id: student,
            from_section_id: sections[from].section_id,
            to_section_id: sections[to].section_id,
        });
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn roster(section: u128, students: std::ops::Range<u128>) -> SectionRoster {
        SectionRoster {
            section_id: id(section),
            students: students.map(id).collect(),
        }
    }

    fn sizes_after(sections: &[SectionRoster], moves: &[Move]) -> Vec<usize> {
        sections
            .iter()
            .map(|s| {
                s.students.len()
                    - moves
                        .iter()
                        .filter(|m| m.from_section_id == s.section_id)
                        .count()
                    + moves
                        .iter()
                        .filter(|m| m.to_section_id == s.section_id)
                        .count()
            })
            .collect()
    }

    #[test]
    fn balances_to_within_one_from_the_end_of_the_roll() {
        let sections = vec![
            roster(1, 100..140),
            roster(2, 200..220),
            roster(3, 300..330),
        ];
        let moves = balance(&sections, &[]);
        assert_eq!(sizes_after(&sections, &moves), vec![30, 30, 30]);
        assert_eq!(moves.len(), 10);
        // The last-admitted students of section 1 move, and all go to section 2.
        assert_eq!(moves[0].student_id, id(139));
        assert!(moves
            .iter()
            .all(|m| m.from_section_id == id(1) && m.to_section_id == id(2)));
    }

    #[test]
    fn remainder_stays_with_the_largest_and_balanced_classes_need_no_moves() {
        let sections = vec![roster(1, 0..31), roster(2, 100..130), roster(3, 200..230)];
        assert!(balance(&sections, &[]).is_empty());
        let sections = vec![roster(1, 0..30), roster(2, 100..131), roster(3, 200..230)];
        assert!(balance(&sections, &[]).is_empty());
        // 10 + 11 + 12 = 33 → 11, 11, 11.
        let sections = vec![roster(1, 0..10), roster(2, 100..111), roster(3, 200..212)];
        let moves = balance(&sections, &[]);
        assert_eq!(moves.len(), 1);
        assert_eq!(sizes_after(&sections, &moves), vec![11, 11, 11]);
    }

    #[test]
    fn excluded_students_stay_and_the_proposal_is_deterministic() {
        let sections = vec![roster(1, 0..5), roster(2, 100..101)];
        let moves = balance(&sections, &[id(4), id(3)]);
        assert_eq!(moves.len(), 2);
        assert_eq!(
            moves.iter().map(|m| m.student_id).collect::<Vec<_>>(),
            vec![id(2), id(1)]
        );
        assert_eq!(balance(&sections, &[id(4), id(3)]), moves);
        // Everything excluded: no moves, no loop.
        let all: Vec<Uuid> = (0..5).map(id).collect();
        assert!(balance(&sections, &all).is_empty());
        assert!(balance(&[], &[]).is_empty());
    }
}
