//! Deterministic generator. No I/O.

use chrono::NaiveDate;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::DEMO_MARK;

#[derive(Debug, Clone)]
pub struct DemoOptions {
    pub seed: u64,
    pub students: usize,
    pub staff: usize,
    pub sections_per_grade: usize,
}

impl Default for DemoOptions {
    fn default() -> Self {
        Self {
            seed: 2026,
            students: 1_200,
            staff: 120,
            sections_per_grade: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DemoSchool {
    pub school: SchoolInfo,
    /// Calendar year the active academic year starts in.
    pub academic_year_start: i32,
    pub subjects: Vec<Subject>,
    pub classes: Vec<Class>,
    pub staff: Vec<Staff>,
    pub students: Vec<Student>,
    pub guardians: Vec<Guardian>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subject {
    pub name: String,
    pub code: String,
    /// Grades (0 = KG … 12) that teach this subject.
    pub grades: Vec<u8>,
}

/// A guardian; `students` lists the admission numbers of the children sharing this record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Guardian {
    pub full_name: String,
    pub relationship: String,
    pub phone: String,
    pub cnic: String,
    pub occupation: String,
    pub students: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchoolInfo {
    pub name: String,
    pub code: String,
    pub city: String,
    pub is_demo: bool,
    pub academic_year_start_month: i16,
    pub currency: String,
}

/// Grades K (0) to 12, each with sections A, B, C.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Class {
    /// 0 = KG, 1..=12.
    pub grade: u8,
    pub section: char,
    /// Display name, e.g. "KG-A", "7-B".
    pub name: String,
    pub class_teacher: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Staff {
    pub username: String,
    pub full_name: String,
    /// Built-in role code: principal, admin, accountant, teacher.
    pub role: String,
    pub subject: Option<String>,
    pub phone: String,
    pub cnic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Student {
    pub admission_no: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: char,
    pub date_of_birth: NaiveDate,
    pub class: String,
    pub roll_no: u16,
    /// B-Form number, `#####-#######-#`.
    pub b_form: String,
    pub address: String,
    pub admission_date: NaiveDate,
}

const MALE_FIRST: &[&str] = &[
    "Ahmed", "Ali", "Bilal", "Danish", "Farhan", "Hamza", "Hassan", "Ibrahim", "Imran", "Junaid",
    "Kamran", "Mohsin", "Mustafa", "Noman", "Omar", "Rizwan", "Saad", "Shahzaib", "Talha", "Umar",
    "Usman", "Waqas", "Yasir", "Zain", "Zubair", "Abdullah", "Faisal", "Haris", "Taimoor",
    "Arsalan",
];
const FEMALE_FIRST: &[&str] = &[
    "Aisha", "Amna", "Anum", "Ayesha", "Bushra", "Fatima", "Hira", "Iqra", "Khadija", "Laiba",
    "Mahnoor", "Maryam", "Mehwish", "Noor", "Rabia", "Sana", "Sadia", "Saba", "Sehrish", "Zainab",
    "Zara", "Hania", "Kinza", "Momina", "Nimra", "Rimsha", "Sumaira", "Tooba", "Urooj", "Warda",
];
const LAST: &[&str] = &[
    "Khan", "Ahmed", "Malik", "Qureshi", "Siddiqui", "Sheikh", "Raza", "Baig", "Chaudhry", "Butt",
    "Hussain", "Iqbal", "Javed", "Mirza", "Nawaz", "Rana", "Shah", "Syed", "Tariq", "Zaidi",
];
const SUBJECTS: &[&str] = &[
    "English",
    "Urdu",
    "Mathematics",
    "Science",
    "Islamiat",
    "Pakistan Studies",
    "Computer",
    "Physics",
    "Chemistry",
    "Biology",
    "Art",
    "Physical Education",
];
const OCCUPATIONS: &[&str] = &[
    "Trader",
    "Teacher",
    "Driver",
    "Engineer",
    "Shopkeeper",
    "Clerk",
    "Doctor",
    "Farmer",
    "Tailor",
];
const AREAS: &[&str] = &[
    "Gulshan-e-Iqbal",
    "North Nazimabad",
    "Korangi",
    "Malir",
    "Clifton",
    "Saddar",
];

fn pick<'a>(rng: &mut StdRng, items: &'a [&'a str]) -> &'a str {
    items.choose(rng).copied().unwrap_or("Demo")
}

pub fn grade_name(grade: u8) -> String {
    if grade == 0 {
        "KG".to_string()
    } else {
        grade.to_string()
    }
}

fn fake_cnic(rng: &mut StdRng) -> String {
    // 42101 is the Karachi district prefix; the rest is random and not a real number.
    format!(
        "42101-{:07}-{}",
        rng.gen_range(0..10_000_000u32),
        rng.gen_range(0..10)
    )
}

fn fake_phone(rng: &mut StdRng) -> String {
    format!(
        "03{:02}{:07}",
        rng.gen_range(0..50),
        rng.gen_range(0..10_000_000u32)
    )
}

/// Subject catalogue with the grades that teach each subject.
pub fn subject_catalogue() -> Vec<Subject> {
    let code = |n: &str| {
        n.split_whitespace()
            .map(|w| w.chars().take(4).collect::<String>())
            .collect::<Vec<_>>()
            .join("")
            .to_uppercase()
    };
    SUBJECTS
        .iter()
        .map(|name| {
            let grades: Vec<u8> = match *name {
                "Physics" | "Chemistry" | "Biology" => (9..=12).collect(),
                "Science" => (1..=8).collect(),
                "Computer" => (4..=12).collect(),
                "Pakistan Studies" => (6..=12).collect(),
                "Art" => (0..=5).collect(),
                _ => (0..=12).collect(),
            };
            Subject {
                name: name.to_string(),
                code: code(name),
                grades,
            }
        })
        .collect()
}

/// Generate a reproducible demo school.
pub fn generate(options: &DemoOptions) -> DemoSchool {
    let mut rng = StdRng::seed_from_u64(options.seed);
    let year0 = 2026;

    let school = SchoolInfo {
        name: format!("{DEMO_MARK} Gulshan Model School"),
        code: "DEMO01".into(),
        city: "Karachi".into(),
        is_demo: true,
        academic_year_start_month: 4,
        currency: "PKR".into(),
    };

    // Staff: 1 principal, 2 admins, 2 accountants, the rest teachers.
    let mut staff = Vec::with_capacity(options.staff);
    let mut used_usernames = std::collections::HashSet::new();
    for i in 0..options.staff {
        let role = match i {
            0 => "principal",
            1 | 2 => "admin",
            3 | 4 => "accountant",
            _ => "teacher",
        };
        let female = rng.gen_bool(0.55);
        let first = pick(&mut rng, if female { FEMALE_FIRST } else { MALE_FIRST });
        let last = pick(&mut rng, LAST);
        let mut username = format!("demo.{}.{}", first.to_lowercase(), last.to_lowercase());
        let mut n = 1;
        while !used_usernames.insert(username.clone()) {
            n += 1;
            username = format!("demo.{}.{}{}", first.to_lowercase(), last.to_lowercase(), n);
        }
        let subject = (role == "teacher").then(|| pick(&mut rng, SUBJECTS).to_string());
        let phone = fake_phone(&mut rng);
        let cnic = fake_cnic(&mut rng);
        staff.push(Staff {
            username,
            full_name: format!("{first} {last}"),
            role: role.to_string(),
            subject,
            phone,
            cnic,
        });
    }

    // Classes K..12 × sections, each with a class teacher.
    let mut classes = Vec::new();
    let mut teachers: Vec<&Staff> = staff.iter().filter(|s| s.role == "teacher").collect();
    teachers.shuffle(&mut rng);
    let mut teacher_iter = teachers.iter().cycle();
    for grade in 0..=12u8 {
        for s in 0..options.sections_per_grade {
            let section = (b'A' + s as u8) as char;
            classes.push(Class {
                grade,
                section,
                name: format!("{}-{}", grade_name(grade), section),
                class_teacher: teacher_iter.next().map(|t| t.username.clone()),
            });
        }
    }

    // Students spread evenly across classes, roll numbers per class. Roughly a quarter of the
    // students are a younger sibling of an earlier student and share that student's father.
    let mut students: Vec<Student> = Vec::with_capacity(options.students);
    let mut guardians: Vec<Guardian> = Vec::new();
    let mut roll: std::collections::HashMap<String, u16> = std::collections::HashMap::new();
    for i in 0..options.students {
        let class = &classes[i % classes.len()];
        let female = rng.gen_bool(0.5);
        let first = pick(&mut rng, if female { FEMALE_FIRST } else { MALE_FIRST });
        let admission_no = format!("{DEMO_MARK}-{:05}", i + 1);
        let sibling_of = if i > 0 && rng.gen_bool(0.25) {
            Some(rng.gen_range(0..i))
        } else {
            None
        };
        let (last, father, address) = match sibling_of {
            Some(j) => {
                let s = &students[j];
                let last = s.full_name.rsplit(' ').next().unwrap_or("Khan").to_string();
                (last, s.father_name.clone(), s.address.clone())
            }
            None => {
                let last = pick(&mut rng, LAST).to_string();
                let father = format!("{} {}", pick(&mut rng, MALE_FIRST), last);
                let address = format!(
                    "House {}, {}, Karachi",
                    rng.gen_range(1..400),
                    pick(&mut rng, AREAS)
                );
                (last, father, address)
            }
        };
        match guardians
            .iter_mut()
            .find(|g| g.full_name == father && sibling_of.is_some())
        {
            Some(g) => g.students.push(admission_no.clone()),
            None => guardians.push(Guardian {
                full_name: father.clone(),
                relationship: "father".into(),
                phone: fake_phone(&mut rng),
                cnic: fake_cnic(&mut rng),
                occupation: pick(&mut rng, OCCUPATIONS).to_string(),
                students: vec![admission_no.clone()],
            }),
        }
        // Age ≈ grade + 5 at the start of the academic year.
        let birth_year = year0 - 5 - class.grade as i32 - rng.gen_range(0..2);
        let dob = NaiveDate::from_ymd_opt(birth_year, rng.gen_range(1..=12), rng.gen_range(1..=28))
            .expect("valid date");
        let r = roll.entry(class.name.clone()).or_insert(0);
        *r += 1;
        let years_in_school = rng.gen_range(0..=class.grade as i32);
        let admission_date =
            NaiveDate::from_ymd_opt(year0 - years_in_school, 4, rng.gen_range(1..=20))
                .expect("valid date");
        students.push(Student {
            admission_no,
            full_name: format!("{first} {last}"),
            father_name: father,
            gender: if female { 'F' } else { 'M' },
            date_of_birth: dob,
            class: class.name.clone(),
            roll_no: *r,
            b_form: fake_cnic(&mut rng),
            address,
            admission_date,
        });
    }

    DemoSchool {
        school,
        academic_year_start: year0,
        subjects: subject_catalogue(),
        classes,
        staff,
        students,
        guardians,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn is_deterministic() {
        let a = generate(&DemoOptions::default());
        let b = generate(&DemoOptions::default());
        assert_eq!(a, b);
        let c = generate(&DemoOptions {
            seed: 7,
            ..DemoOptions::default()
        });
        assert_ne!(a.students[0].full_name, c.students[0].full_name);
    }

    #[test]
    fn has_expected_sizes_and_classes() {
        let d = generate(&DemoOptions::default());
        assert_eq!(d.students.len(), 1_200);
        assert_eq!(d.staff.len(), 120);
        assert_eq!(d.classes.len(), 13 * 3);
        assert_eq!(d.classes[0].name, "KG-A");
        assert_eq!(d.classes.last().unwrap().name, "12-C");
        for class in &d.classes {
            let n = d.students.iter().filter(|s| s.class == class.name).count();
            assert!((30..=31).contains(&n), "{}: {n}", class.name);
            assert!(class.class_teacher.is_some());
        }
        assert_eq!(d.staff.iter().filter(|s| s.role == "principal").count(), 1);
        assert_eq!(d.staff.iter().filter(|s| s.role == "admin").count(), 2);
        assert_eq!(d.staff.iter().filter(|s| s.role == "accountant").count(), 2);
        assert_eq!(d.staff.iter().filter(|s| s.role == "teacher").count(), 115);
    }

    #[test]
    fn everything_is_marked_demo_and_unique() {
        let d = generate(&DemoOptions::default());
        assert!(d.school.is_demo);
        assert!(d.school.name.starts_with(DEMO_MARK));
        assert!(d.school.code.starts_with(DEMO_MARK));
        let usernames: HashSet<_> = d.staff.iter().map(|s| s.username.as_str()).collect();
        assert_eq!(usernames.len(), d.staff.len());
        assert!(usernames.iter().all(|u| u.starts_with("demo.")));
        let admissions: HashSet<_> = d.students.iter().map(|s| s.admission_no.as_str()).collect();
        assert_eq!(admissions.len(), d.students.len());
        assert!(admissions.iter().all(|a| a.starts_with(DEMO_MARK)));
        // Roll numbers are unique within a class and ages are plausible.
        for class in &d.classes {
            let rolls: HashSet<_> = d
                .students
                .iter()
                .filter(|s| s.class == class.name)
                .map(|s| s.roll_no)
                .collect();
            assert_eq!(
                rolls.len(),
                d.students.iter().filter(|s| s.class == class.name).count()
            );
        }
        for s in &d.students {
            let year = s
                .date_of_birth
                .format("%Y")
                .to_string()
                .parse::<i32>()
                .unwrap();
            assert!((2008..=2021).contains(&year), "{}", s.date_of_birth);
        }
    }

    #[test]
    fn guardians_are_shared_by_siblings_and_subjects_cover_all_grades() {
        let d = generate(&DemoOptions::default());
        let linked: usize = d.guardians.iter().map(|g| g.students.len()).sum();
        assert_eq!(
            linked,
            d.students.len(),
            "every student has exactly one guardian"
        );
        let families = d.guardians.iter().filter(|g| g.students.len() > 1).count();
        assert!(
            families > 100,
            "expected many sibling families, got {families}"
        );
        for g in &d.guardians {
            for a in &g.students {
                let s = d.students.iter().find(|s| &s.admission_no == a).unwrap();
                assert_eq!(s.father_name, g.full_name);
            }
            assert_eq!(g.cnic.len(), 15);
            assert_eq!(g.phone.len(), 11);
        }
        for grade in 0..=12u8 {
            let n = d
                .subjects
                .iter()
                .filter(|s| s.grades.contains(&grade))
                .count();
            assert!(n >= 5, "grade {grade} has {n} subjects");
        }
        let codes: HashSet<_> = d.subjects.iter().map(|s| s.code.as_str()).collect();
        assert_eq!(codes.len(), d.subjects.len());
        for s in &d.students {
            assert!(s.admission_date <= NaiveDate::from_ymd_opt(2026, 4, 30).unwrap());
            assert_eq!(s.b_form.len(), 15);
        }
    }
}
