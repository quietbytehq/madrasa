//! Shared fixture for the Nexus sync tests: one school with a section, students, a fee head,
//! staff and a user, joined to its copy at the hub and fully uploaded.
#![allow(dead_code)]

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_core::sync::{policy, ApplyReport, Origin};
use madrasa_db::models::{AcademicYear, School, Section};
use madrasa_db::testing::TestDb;
use madrasa_db::{academics, fees, schools, staff, students, sync, users, Actor, PgPool};
use serde_json::{json, Value};
use uuid::Uuid;

pub fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// The two copies of one school, each with its own clock.
pub struct Net {
    pub school: TestDb,
    pub hub: TestDb,
    pub school_hlc: Hlc,
    pub hub_hlc: Hlc,
    pub sch: School,
    pub year: AcademicYear,
    pub section: Section,
    /// A user of the school (receipts and attendance need one).
    pub admin: Uuid,
    pub students: Vec<Uuid>,
    pub staff: Vec<Uuid>,
    pub fee_head: Uuid,
}

/// Move every pending change from one copy to the other, as the transport does.
pub async fn exchange(from: &PgPool, to: &PgPool, to_hlc: &Hlc) -> ApplyReport {
    let mut total = ApplyReport::default();
    let mut first = true;
    loop {
        let mut src = from.begin().await.unwrap();
        let batch = sync::collect(&mut src, 200).await.unwrap();
        src.commit().await.unwrap();
        // Even an empty batch is applied once: that is when parked records are retried.
        if batch.is_empty() && !first {
            break;
        }
        first = false;
        let mut dst = to.begin().await.unwrap();
        let report = sync::apply(&mut dst, to_hlc, &batch).await.unwrap();
        dst.commit().await.unwrap();
        let mut src = from.begin().await.unwrap();
        sync::acknowledge(&mut src, &report.handled).await.unwrap();
        let stored = sync::store_remote_overwrites(&mut src, &report.overwrites)
            .await
            .unwrap();
        src.commit().await.unwrap();
        let mut dst = to.begin().await.unwrap();
        sync::mark_overwrites_reported(&mut dst, &stored)
            .await
            .unwrap();
        dst.commit().await.unwrap();
        total.applied += report.applied;
        total.unchanged += report.unchanged;
        total.overwrites.extend(report.overwrites);
        total.parked = report.parked;
    }
    total
}

impl Net {
    pub fn actor(&self) -> Actor {
        Actor::user(self.admin, self.sch.id)
    }

    pub async fn push(&self) -> ApplyReport {
        exchange(&self.school.pool, &self.hub.pool, &self.hub_hlc).await
    }

    pub async fn pull(&self) -> ApplyReport {
        exchange(&self.hub.pool, &self.school.pool, &self.school_hlc).await
    }

    /// Sync until both sides are quiet. `push_first` is the order a school uses; the tests
    /// also run the other order to show the result does not depend on it.
    pub async fn settle(&self, push_first: bool) {
        for _ in 0..8 {
            let (a, b) = if push_first {
                (self.push().await, self.pull().await)
            } else {
                (self.pull().await, self.push().await)
            };
            assert!(
                a.parked.is_empty() && b.parked.is_empty(),
                "parked: {:?} {:?}",
                a.parked,
                b.parked
            );
            let quiet = a.applied == 0
                && b.applied == 0
                && pending(&self.school.pool).await == 0
                && pending(&self.hub.pool).await == 0;
            if quiet {
                return;
            }
        }
        panic!("the two copies did not settle in eight rounds");
    }

    /// The same records, value for value, on both sides (derived columns included).
    pub async fn assert_same(&self, tables: &[&str]) {
        for table in tables {
            assert_eq!(
                rows(&self.school.pool, table).await,
                rows(&self.hub.pool, table).await,
                "{table} differs between the school and the hub"
            );
        }
    }

    pub async fn finish(self) {
        self.school.cleanup().await;
        self.hub.cleanup().await;
    }
}

pub async fn pending(pool: &PgPool) -> i64 {
    let mut c = pool.acquire().await.unwrap();
    sync::status(&mut c).await.unwrap().pending
}

/// Every record of a table as canonical JSON, without the columns that are never sent.
pub async fn rows(pool: &PgPool, table: &str) -> Vec<Value> {
    let p = policy(table).unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let rows: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT sync_row(to_jsonb(t)) FROM {table} t ORDER BY id"
    )))
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|mut r| {
            r.as_object_mut().unwrap().retain(|k, _| !p.is_excluded(k));
            r
        })
        .collect()
}

/// The overwrite log of one side as (table, field, reason, kept, discarded), oldest first.
pub async fn log(pool: &PgPool) -> Vec<(String, String, String, Value, Value)> {
    let mut c = pool.acquire().await.unwrap();
    let mut entries = sync::list_overwrites(&mut c, 100, 0).await.unwrap();
    entries.reverse();
    entries
        .into_iter()
        .map(|o| (o.table, o.field, o.reason, o.kept, o.discarded))
        .collect()
}

/// A joined school with one section of `students` pupils, two staff, a fee head and a
/// user, already uploaded to its hub copy. `None` without a test database.
pub async fn net(students_wanted: usize) -> Option<Net> {
    let school = TestDb::new().await?;
    let hub = TestDb::new().await?;
    let school_hlc = Hlc::new(0x5c);
    let hub_hlc = Hlc::new(0x4b);
    let mut c = school.pool.acquire().await.unwrap();
    let sch = schools::create(
        &mut c,
        &school_hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Gulshan High".into(),
            code: "KHI-SY".into(),
            city: "Karachi".into(),
            is_demo: false,
            academic_year_start_month: 4,
            currency: "PKR".into(),
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
        },
    )
    .await
    .unwrap();
    let actor = Actor::system_for(sch.id);
    let (name, start, end) = academics::default_year_bounds(2026, 4).unwrap();
    let year = academics::create_year(
        &mut c,
        &school_hlc,
        actor,
        academics::NewAcademicYear {
            school_id: sch.id,
            name,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await
    .unwrap();
    let year = academics::activate_year(&mut c, &school_hlc, actor, year.id)
        .await
        .unwrap();
    let class = academics::create_class(&mut c, &school_hlc, actor, sch.id, "5", 5)
        .await
        .unwrap();
    let section = academics::create_section(
        &mut c,
        &school_hlc,
        actor,
        academics::NewSection {
            school_id: sch.id,
            class_id: class.id,
            academic_year_id: year.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    let admin = users::create(
        &mut c,
        &school_hlc,
        actor,
        users::NewUser {
            school_id: sch.id,
            username: "ayesha".into(),
            full_name: "Ayesha Khan".into(),
            email: None,
            password_hash: hash_password("school-password-1").unwrap(),
        },
    )
    .await
    .unwrap()
    .id;
    let mut pupils = Vec::new();
    for i in 0..students_wanted {
        let s = students::create(
            &mut c,
            &school_hlc,
            actor,
            students::NewStudent {
                school_id: sch.id,
                student_number: None,
                legacy_id: None,
                full_name: format!("Student {}", (b'A' + i as u8) as char),
                father_name: "Father".into(),
                date_of_birth: d(2015, 1, 1),
                gender: "male".into(),
                cnic: None,
                address: "".into(),
                admission_date: d(2026, 4, 1),
                board_roll_no: None,
                board_registration_no: None,
                custom: json!({}),
            },
        )
        .await
        .unwrap();
        students::enrol(&mut c, &school_hlc, actor, s.id, section.id, None)
            .await
            .unwrap();
        pupils.push(s.id);
    }
    let mut people = Vec::new();
    for (n, name) in ["Bilal Raza", "Sana Malik"].iter().enumerate() {
        let member = staff::create(
            &mut c,
            &school_hlc,
            actor,
            staff::NewStaff {
                school_id: sch.id,
                user_id: None,
                staff_number: format!("T-{}", n + 1),
                full_name: (*name).into(),
                father_name: "Father".into(),
                gender: "male".into(),
                phone: None,
                email: None,
                cnic: None,
                designation: "Teacher".into(),
                joined_on: Some(d(2026, 4, 1)),
                custom: json!({}),
            },
        )
        .await
        .unwrap();
        people.push(member.id);
    }
    let fee_head = fees::create_head(
        &mut c,
        &school_hlc,
        actor,
        sch.id,
        "Tuition",
        "TUI",
        true,
        1,
    )
    .await
    .unwrap()
    .id;
    drop(c);

    // Join and upload.
    let mut c = school.pool.begin().await.unwrap();
    sync::enable(&mut c, sch.id, Origin::School).await.unwrap();
    sync::queue_snapshot(&mut c).await.unwrap();
    let first = sync::collect(&mut c, 1).await.unwrap();
    c.commit().await.unwrap();
    let mut h = hub.pool.begin().await.unwrap();
    sync::adopt_school(&mut h, &first[0], Origin::HeadOffice)
        .await
        .unwrap();
    h.commit().await.unwrap();
    let net = Net {
        school,
        hub,
        school_hlc,
        hub_hlc,
        sch,
        year,
        section,
        admin,
        students: pupils,
        staff: people,
        fee_head,
    };
    let up = net.push().await;
    assert!(up.parked.is_empty(), "{:?}", up.parked);
    Some(net)
}

/// Skip the current test unless a test database is available.
#[macro_export]
macro_rules! net {
    ($students:expr) => {
        match $crate::sync_support::net($students).await {
            Some(n) => n,
            None => return,
        }
    };
}
