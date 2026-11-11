//! Nexus sync engine (stage 2): change capture and the field-level merge, exercised with two
//! real databases: a school and its copy at the hub. Runs when `MADRASA_TEST_DATABASE_URL`
//! is set; skipped otherwise.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_core::sync::{policy, ApplyReport, Change, Mode, Origin, TABLES};
use madrasa_db::models::{School, Student};
use madrasa_db::testing::TestDb;
use madrasa_db::{academics, schools, students, sync, test_db, users, Actor, PgPool};
use serde_json::{json, Value};
use sqlx::Row;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// The two copies of one school, each with its own clock.
struct Pair {
    school: TestDb,
    hub: TestDb,
    sch: School,
    school_hlc: Hlc,
    hub_hlc: Hlc,
}

impl Pair {
    fn actor(&self) -> Actor {
        Actor::system_for(self.sch.id)
    }

    /// Send everything the school has waiting to the hub copy.
    async fn push(&self) -> ApplyReport {
        exchange(&self.school.pool, &self.hub.pool, &self.hub_hlc).await
    }

    /// Bring everything head office has waiting down to the school.
    async fn pull(&self) -> ApplyReport {
        exchange(&self.hub.pool, &self.school.pool, &self.school_hlc).await
    }

    /// One full round, the order a school uses: push, then pull.
    async fn round(&self) -> (ApplyReport, ApplyReport) {
        (self.push().await, self.pull().await)
    }

    async fn finish(self) {
        self.school.cleanup().await;
        self.hub.cleanup().await;
    }
}

/// Move every pending change from one copy to the other, as the transport does: collect,
/// apply, acknowledge, and pass the overwrite log entries back.
async fn exchange(from: &PgPool, to: &PgPool, to_hlc: &Hlc) -> ApplyReport {
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

async fn make_school(pool: &PgPool, hlc: &Hlc) -> School {
    let mut c = pool.acquire().await.unwrap();
    schools::create(
        &mut c,
        hlc,
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
    .unwrap()
}

fn new_student(school_id: uuid::Uuid, name: &str) -> students::NewStudent {
    students::NewStudent {
        school_id,
        student_number: None,
        legacy_id: None,
        full_name: name.into(),
        father_name: "Father".into(),
        date_of_birth: d(2015, 5, 1),
        gender: "male".into(),
        cnic: None,
        address: "".into(),
        admission_date: d(2026, 4, 10),
        board_roll_no: None,
        board_registration_no: None,
        custom: json!({ "house": "Jinnah", "points": 12.5 }),
    }
}

async fn add_student(pool: &PgPool, hlc: &Hlc, school: &School, name: &str) -> Student {
    let mut c = pool.acquire().await.unwrap();
    students::create(
        &mut c,
        hlc,
        Actor::system_for(school.id),
        new_student(school.id, name),
    )
    .await
    .unwrap()
}

async fn rename(
    pool: &PgPool,
    hlc: &Hlc,
    school: &School,
    id: uuid::Uuid,
    p: students::StudentUpdate,
) {
    let mut c = pool.acquire().await.unwrap();
    students::update(&mut c, hlc, Actor::system_for(school.id), id, p)
        .await
        .unwrap();
}

async fn student(pool: &PgPool, id: uuid::Uuid) -> Student {
    let mut c = pool.acquire().await.unwrap();
    students::get(&mut c, id).await.unwrap()
}

/// A school with a year, a class, a user and two students, joined to a hub copy and fully
/// uploaded.
async fn joined_pair() -> Option<Pair> {
    let school = TestDb::new().await?;
    let hub = TestDb::new().await?;
    let school_hlc = Hlc::new(0x5c);
    let hub_hlc = Hlc::new(0x4b);
    let sch = make_school(&school.pool, &school_hlc).await;
    {
        let mut c = school.pool.acquire().await.unwrap();
        let (name, start, end) = academics::default_year_bounds(2026, 4).unwrap();
        academics::create_year(
            &mut c,
            &school_hlc,
            Actor::system_for(sch.id),
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
        academics::create_class(
            &mut c,
            &school_hlc,
            Actor::system_for(sch.id),
            sch.id,
            "5",
            5,
        )
        .await
        .unwrap();
        users::create(
            &mut c,
            &school_hlc,
            Actor::system_for(sch.id),
            users::NewUser {
                school_id: sch.id,
                username: "ayesha".into(),
                full_name: "Ayesha Khan".into(),
                email: None,
                password_hash: hash_password("school-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
    }
    add_student(&school.pool, &school_hlc, &sch, "Ali Khan").await;
    add_student(&school.pool, &school_hlc, &sch, "Sara Noor").await;

    // Join: the school switches capture on and queues everything; the hub copy starts from
    // the school's own record.
    let mut c = school.pool.begin().await.unwrap();
    sync::enable(&mut c, sch.id, Origin::School).await.unwrap();
    sync::queue_snapshot(&mut c).await.unwrap();
    let first = sync::collect(&mut c, 1).await.unwrap();
    c.commit().await.unwrap();
    assert_eq!(
        first[0].table, "schools",
        "the school's own record is sent first"
    );
    let mut h = hub.pool.begin().await.unwrap();
    sync::adopt_school(&mut h, &first[0], Origin::HeadOffice)
        .await
        .unwrap();
    h.commit().await.unwrap();

    let pair = Pair {
        school,
        hub,
        sch,
        school_hlc,
        hub_hlc,
    };
    let report = pair.push().await;
    assert!(report.parked.is_empty(), "{:?}", report.parked);
    Some(pair)
}

macro_rules! pair {
    () => {
        match joined_pair().await {
            Some(p) => p,
            None => return,
        }
    };
}

/// Every record of a table as canonical JSON, without the columns that are never sent.
async fn table_rows(pool: &PgPool, table: &str) -> Vec<Value> {
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
            let o = r.as_object_mut().unwrap();
            o.retain(|k, _| !p.is_excluded(k));
            r
        })
        .collect()
}

async fn status(pool: &PgPool) -> sync::SyncStatus {
    let mut c = pool.acquire().await.unwrap();
    sync::status(&mut c).await.unwrap()
}

#[tokio::test]
async fn the_policy_covers_every_table_of_the_schema() {
    let db: TestDb = test_db!();
    let mut c = db.pool.acquire().await.unwrap();
    assert_eq!(
        sync::unclassified_tables(&mut c).await.unwrap(),
        Vec::<String>::new(),
        "classify new tables in madrasa_core::sync::policy"
    );
    // And the policy names no table that does not exist.
    for p in TABLES {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(p.table)
            .fetch_one(&mut *c)
            .await
            .unwrap();
        assert!(exists, "{} is in the policy but not in the schema", p.table);
        // Excluded and derived columns are real columns.
        for column in p
            .exclude
            .iter()
            .map(|(c, _)| *c)
            .chain(p.derived.iter().copied())
        {
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                 WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2)",
            )
            .bind(p.table)
            .bind(column)
            .fetch_one(&mut *c)
            .await
            .unwrap();
            assert!(
                found,
                "{}.{column} is in the policy but not in the schema",
                p.table
            );
        }
    }
    assert!(sync::schema_version(&mut c).await.unwrap() >= 26);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn a_school_that_has_not_joined_has_no_triggers_and_notes_nothing() {
    let db: TestDb = test_db!();
    let hlc = Hlc::new(1);
    let sch = make_school(&db.pool, &hlc).await;
    let ali = add_student(&db.pool, &hlc, &sch, "Ali Khan").await;
    rename(
        &db.pool,
        &hlc,
        &sch,
        ali.id,
        students::StudentUpdate {
            full_name: Some("Ali Ahmed Khan".into()),
            ..Default::default()
        },
    )
    .await;
    let mut c = db.pool.acquire().await.unwrap();
    assert_eq!(sync::triggers_installed(&mut c).await.unwrap(), 0);
    assert!(sync::state(&mut c).await.unwrap().is_none());
    let st = sync::status(&mut c).await.unwrap();
    assert_eq!(
        (st.enabled, st.role, st.pending, st.parked, st.overwrites),
        (false, None, 0, 0, 0)
    );
    let stamps: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_field_stamps")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(stamps, 0);
    // Collecting or applying without having joined is refused, not silently ignored.
    assert!(sync::collect(&mut c, 10).await.unwrap().is_empty());
    assert!(sync::apply(&mut c, &hlc, &[]).await.is_err());
    assert!(sync::queue_snapshot(&mut c).await.is_err());
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn joining_uploads_everything_and_the_two_copies_match() {
    let pair = pair!();
    assert_eq!(
        status(&pair.school.pool).await.pending,
        0,
        "everything was acknowledged"
    );
    assert_eq!(
        status(&pair.hub.pool).await.pending,
        0,
        "received records are not sent back"
    );
    assert_eq!(status(&pair.hub.pool).await.role, Some(Origin::HeadOffice));

    let mut compared = 0;
    for p in TABLES.iter().filter(|p| p.is_synced()) {
        let here = table_rows(&pair.school.pool, p.table).await;
        let there = table_rows(&pair.hub.pool, p.table).await;
        assert_eq!(
            here, there,
            "{} differs between the school and the hub",
            p.table
        );
        compared += here.len();
    }
    assert!(
        compared >= 10,
        "school, year, class, user, students, roles, audit rows: {compared}"
    );
    // Money-like and decimal values arrive exactly; nested JSON survives.
    let ali = table_rows(&pair.hub.pool, "students").await;
    assert_eq!(
        ali[0]["custom"],
        json!({ "house": "Jinnah", "points": 12.5 })
    );

    // The school's password hash never left: the hub holds a placeholder nobody can use.
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users")
        .fetch_one(&pair.hub.pool)
        .await
        .unwrap();
    assert_eq!(hash, "!");
    let real: String = sqlx::query_scalar("SELECT password_hash FROM users")
        .fetch_one(&pair.school.pool)
        .await
        .unwrap();
    assert!(real.starts_with("$argon2id$"));

    // Tables that stay at home did not travel.
    for table in ["settings", "school_sequences", "sessions"] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&pair.hub.pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table} must not be synced");
    }
    pair.finish().await;
}

#[tokio::test]
async fn what_is_sent_never_contains_a_password_hash() {
    let school: TestDb = test_db!();
    let hlc = Hlc::new(3);
    let sch = make_school(&school.pool, &hlc).await;
    let mut c = school.pool.begin().await.unwrap();
    let user = users::create(
        &mut c,
        &hlc,
        Actor::system_for(sch.id),
        users::NewUser {
            school_id: sch.id,
            username: "ayesha".into(),
            full_name: "Ayesha Khan".into(),
            email: None,
            password_hash: hash_password("school-password-1").unwrap(),
        },
    )
    .await
    .unwrap();
    sync::enable(&mut c, sch.id, Origin::School).await.unwrap();
    sync::queue_snapshot(&mut c).await.unwrap();
    let all = sync::collect(&mut c, 1000).await.unwrap();
    sync::acknowledge(&mut c, &all.iter().map(|c| c.seq).collect::<Vec<_>>())
        .await
        .unwrap();
    let text = serde_json::to_string(&all).unwrap();
    assert!(
        !text.contains("argon2"),
        "a password hash was about to be sent"
    );
    let sent_user = all
        .iter()
        .find(|c| c.table == "users")
        .expect("the user is sent");
    for column in [
        "password_hash",
        "failed_login_count",
        "locked_until",
        "last_login_at",
    ] {
        assert!(
            !sent_user.row.contains_key(column),
            "{column} must stay at the school"
        );
    }
    assert_eq!(sent_user.row["username"], "ayesha");

    // A failed login changes only lockout columns: nothing to send. A rename is sent, still
    // without the hash.
    users::record_failed_login(&mut c, &hlc, user.id, 1, None)
        .await
        .unwrap();
    let after_failure = sync::collect(&mut c, 10).await.unwrap();
    assert!(
        after_failure.iter().all(|c| c.table != "users"),
        "lockout counters are not a change worth sending"
    );
    sync::acknowledge(
        &mut c,
        &after_failure.iter().map(|c| c.seq).collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET full_name = 'Ayesha K.', hlc = $2 WHERE id = $1")
        .bind(user.id)
        .bind(hlc.now().to_string())
        .execute(&mut *c)
        .await
        .unwrap();
    let batch = sync::collect(&mut c, 10).await.unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(
        batch[0].fields.as_deref(),
        Some(&["full_name".to_string()][..])
    );
    assert_eq!(
        batch[0].stamps.len(),
        1,
        "the changed field carries its own stamp"
    );
    assert!(!serde_json::to_string(&batch).unwrap().contains("argon2"));
    c.commit().await.unwrap();
    school.cleanup().await;
}

#[tokio::test]
async fn edits_to_different_fields_of_one_record_both_survive() {
    let pair = pair!();
    let ali = table_rows(&pair.school.pool, "students").await[0]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // The school corrects the name; head office corrects the father's name, before a sync.
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali Ahmed Khan".into()),
            ..Default::default()
        },
    )
    .await;
    rename(
        &pair.hub.pool,
        &pair.hub_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            father_name: Some("Imran Ahmed Khan".into()),
            ..Default::default()
        },
    )
    .await;
    // Only the changed field is queued, with its own stamp and origin.
    let stamp = sqlx::query("SELECT field, origin FROM sync_field_stamps WHERE row_id = $1")
        .bind(ali)
        .fetch_all(&pair.hub.pool)
        .await
        .unwrap();
    assert_eq!(stamp.len(), 1);
    assert_eq!(
        (stamp[0].get::<String, _>(0), stamp[0].get::<String, _>(1)),
        ("father_name".to_string(), "head_office".to_string())
    );

    let (up, down) = pair.round().await;
    assert!(
        up.overwrites.is_empty() && down.overwrites.is_empty(),
        "no conflict: different fields"
    );
    for pool in [&pair.school.pool, &pair.hub.pool] {
        let s = student(pool, ali).await;
        assert_eq!(
            (s.full_name.as_str(), s.father_name.as_str()),
            ("Ali Ahmed Khan", "Imran Ahmed Khan")
        );
    }
    assert_eq!(
        table_rows(&pair.school.pool, "students").await,
        table_rows(&pair.hub.pool, "students").await
    );
    assert_eq!(status(&pair.school.pool).await.overwrites, 0);
    // A second round has nothing left to do.
    let (up, down) = pair.round().await;
    assert_eq!((up.applied, down.applied), (0, 0));
    pair.finish().await;
}

#[tokio::test]
async fn the_same_field_on_both_sides_head_office_wins_and_both_logs_say_so() {
    let pair = pair!();
    let ali: uuid::Uuid = table_rows(&pair.school.pool, "students").await[0]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // Head office edits first; the school edits the same field later (a later stamp).
    rename(
        &pair.hub.pool,
        &pair.hub_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali Khan (head office)".into()),
            ..Default::default()
        },
    )
    .await;
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali Khan (school)".into()),
            address: Some("House 12, Block 5".into()),
            ..Default::default()
        },
    )
    .await;

    let (up, down) = pair.round().await;
    // The hub refused the school's name; the school's other change went through.
    assert_eq!(up.overwrites.len(), 1, "{:?}", up.overwrites);
    assert!(
        down.overwrites.is_empty(),
        "the conflict is logged once, not twice"
    );
    for pool in [&pair.school.pool, &pair.hub.pool] {
        let s = student(pool, ali).await;
        assert_eq!(s.full_name, "Ali Khan (head office)", "head office wins");
        assert_eq!(
            s.address, "House 12, Block 5",
            "the non-conflicting field is kept"
        );
    }
    assert_eq!(
        table_rows(&pair.school.pool, "students").await,
        table_rows(&pair.hub.pool, "students").await
    );

    // Both sides hold the same log entry: what was kept, what was discarded, and why.
    let mut logs = Vec::new();
    for pool in [&pair.school.pool, &pair.hub.pool] {
        let mut c = pool.acquire().await.unwrap();
        let log = sync::list_overwrites(&mut c, 10, 0).await.unwrap();
        assert_eq!(log.len(), 1);
        let o = &log[0];
        assert_eq!(
            (o.table.as_str(), o.id, o.field.as_str()),
            ("students", ali, "full_name")
        );
        assert_eq!(o.kept, json!("Ali Khan (head office)"));
        assert_eq!(o.discarded, json!("Ali Khan (school)"));
        assert_eq!(o.kept_origin, Origin::HeadOffice);
        assert_eq!(o.reason, "head_office_wins");
        assert!(
            o.discarded_hlc > o.kept_hlc,
            "the school's edit was the later one and still lost"
        );
        assert!(sync::unreported_overwrites(&mut c, 10)
            .await
            .unwrap()
            .is_empty());
        logs.push(o.clone());
    }
    assert_eq!(logs[0], logs[1]);

    // The other way round: the school edits, syncs its batch, then edits again before the
    // pull brings a head office change of the same field. Head office wins at the school.
    rename(
        &pair.hub.pool,
        &pair.hub_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            father_name: Some("Imran (head office)".into()),
            ..Default::default()
        },
    )
    .await;
    pair.push().await;
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            father_name: Some("Imran (school)".into()),
            ..Default::default()
        },
    )
    .await;
    let down = pair.pull().await;
    assert_eq!(down.overwrites.len(), 1);
    assert_eq!(down.overwrites[0].discarded, json!("Imran (school)"));
    pair.round().await;
    for pool in [&pair.school.pool, &pair.hub.pool] {
        assert_eq!(student(pool, ali).await.father_name, "Imran (head office)");
        assert_eq!(status(pool).await.overwrites, 2);
        assert_eq!(status(pool).await.pending, 0);
    }
    pair.finish().await;
}

#[tokio::test]
async fn a_replayed_batch_changes_nothing() {
    let pair = pair!();
    let ali: uuid::Uuid = table_rows(&pair.school.pool, "students").await[0]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali A. Khan".into()),
            ..Default::default()
        },
    )
    .await;
    // The batch reaches the hub, but the acknowledgement is lost.
    let mut c = pair.school.pool.begin().await.unwrap();
    let batch = sync::collect(&mut c, 100).await.unwrap();
    c.commit().await.unwrap();
    let mut h = pair.hub.pool.begin().await.unwrap();
    let first = sync::apply(&mut h, &pair.hub_hlc, &batch).await.unwrap();
    h.commit().await.unwrap();
    assert_eq!(
        first.applied, 2,
        "the student and the audit row of the edit"
    );
    // Head office then changes the same field, having seen the school's value.
    rename(
        &pair.hub.pool,
        &pair.hub_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali Ahmed Khan".into()),
            ..Default::default()
        },
    )
    .await;
    // The school sends the same batch again.
    let mut h = pair.hub.pool.begin().await.unwrap();
    let again = sync::apply(&mut h, &pair.hub_hlc, &batch).await.unwrap();
    h.commit().await.unwrap();
    assert_eq!((again.applied, again.unchanged), (0, 2));
    assert!(again.overwrites.is_empty(), "a replay is not a conflict");
    assert_eq!(
        student(&pair.hub.pool, ali).await.full_name,
        "Ali Ahmed Khan"
    );
    let mut c = pair.school.pool.begin().await.unwrap();
    sync::acknowledge(&mut c, &again.handled).await.unwrap();
    c.commit().await.unwrap();

    // The same on the way down: the school applies head office's change, the acknowledgement
    // is lost, the school edits the field again, and the old batch arrives a second time.
    let mut h = pair.hub.pool.begin().await.unwrap();
    let down = sync::collect(&mut h, 100).await.unwrap();
    h.commit().await.unwrap();
    let mut c = pair.school.pool.begin().await.unwrap();
    sync::apply(&mut c, &pair.school_hlc, &down).await.unwrap();
    c.commit().await.unwrap();
    assert_eq!(
        student(&pair.school.pool, ali).await.full_name,
        "Ali Ahmed Khan"
    );
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("Ali Ahmed Khan Jr".into()),
            ..Default::default()
        },
    )
    .await;
    let mut c = pair.school.pool.begin().await.unwrap();
    let again = sync::apply(&mut c, &pair.school_hlc, &down).await.unwrap();
    c.commit().await.unwrap();
    assert!(again.overwrites.is_empty());
    assert_eq!(
        student(&pair.school.pool, ali).await.full_name,
        "Ali Ahmed Khan Jr",
        "a replay of an old head office value must not undo a newer school edit"
    );
    let mut h = pair.hub.pool.begin().await.unwrap();
    sync::acknowledge(&mut h, &again.handled).await.unwrap();
    h.commit().await.unwrap();
    pair.round().await;
    for pool in [&pair.school.pool, &pair.hub.pool] {
        assert_eq!(student(pool, ali).await.full_name, "Ali Ahmed Khan Jr");
        assert_eq!(status(pool).await.overwrites, 0);
    }
    pair.finish().await;
}

#[tokio::test]
async fn records_that_cannot_be_stored_yet_are_parked_and_retried() {
    let pair = pair!();
    let actor = pair.actor();
    // A new class and a section in it; the section's change is delivered before the class.
    let (class, section) = {
        let mut c = pair.school.pool.acquire().await.unwrap();
        let year: uuid::Uuid = sqlx::query_scalar("SELECT id FROM academic_years")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        let class = academics::create_class(&mut c, &pair.school_hlc, actor, pair.sch.id, "6", 6)
            .await
            .unwrap();
        let section = academics::create_section(
            &mut c,
            &pair.school_hlc,
            actor,
            academics::NewSection {
                school_id: pair.sch.id,
                class_id: class.id,
                academic_year_id: year,
                name: "A".into(),
                class_teacher_id: None,
                capacity: None,
            },
        )
        .await
        .unwrap();
        (class, section)
    };
    let mut c = pair.school.pool.begin().await.unwrap();
    let batch = sync::collect(&mut c, 100).await.unwrap();
    c.commit().await.unwrap();
    let pick = |table: &str, id: uuid::Uuid| -> Change {
        batch
            .iter()
            .find(|c| c.table == table && c.id == id)
            .unwrap()
            .clone()
    };
    let (class_change, section_change) = (pick("classes", class.id), pick("sections", section.id));

    let mut h = pair.hub.pool.begin().await.unwrap();
    let report = sync::apply(&mut h, &pair.hub_hlc, std::slice::from_ref(&section_change))
        .await
        .unwrap();
    h.commit().await.unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(
        report.handled,
        [section_change.seq],
        "a parked record is the receiver's to keep"
    );
    assert_eq!(report.parked.len(), 1);
    assert_eq!(
        (report.parked[0].table.as_str(), report.parked[0].id),
        ("sections", section.id)
    );
    assert!(
        report.parked[0]
            .reason
            .starts_with("It refers to a record that has not arrived here yet"),
        "{}",
        report.parked[0].reason
    );
    assert_eq!(status(&pair.hub.pool).await.parked, 1);

    // A change to the parked record's field while it waits is folded into it.
    let mut c = pair.school.pool.begin().await.unwrap();
    sync::acknowledge(&mut c, &report.handled).await.unwrap();
    sqlx::query("UPDATE sections SET name = 'B', hlc = $2 WHERE id = $1")
        .bind(section.id)
        .bind(pair.school_hlc.now().to_string())
        .execute(&mut *c)
        .await
        .unwrap();
    let rename_change = sync::collect(&mut c, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.table == "sections")
        .unwrap();
    c.commit().await.unwrap();
    assert!(rename_change.fields.is_some());
    let mut h = pair.hub.pool.begin().await.unwrap();
    let report = sync::apply(&mut h, &pair.hub_hlc, &[rename_change])
        .await
        .unwrap();
    h.commit().await.unwrap();
    assert_eq!(report.parked.len(), 1, "still waiting for the class");

    // The class arrives: the section goes in with it, already renamed.
    let mut h = pair.hub.pool.begin().await.unwrap();
    let report = sync::apply(&mut h, &pair.hub_hlc, &[class_change])
        .await
        .unwrap();
    h.commit().await.unwrap();
    assert_eq!(report.applied, 2);
    assert!(report.parked.is_empty());
    assert_eq!(status(&pair.hub.pool).await.parked, 0);
    let name: String = sqlx::query_scalar("SELECT name FROM sections WHERE id = $1")
        .bind(section.id)
        .fetch_one(&pair.hub.pool)
        .await
        .unwrap();
    assert_eq!(name, "B");
    let mut c = pair.school.pool.begin().await.unwrap();
    let rest = sync::collect(&mut c, 100).await.unwrap();
    sync::acknowledge(&mut c, &rest.iter().map(|c| c.seq).collect::<Vec<_>>())
        .await
        .unwrap();
    c.commit().await.unwrap();

    // Two different students with the same number: the second cannot be stored. It waits,
    // with a reason a person can act on, and goes in once the number is changed.
    let twin = {
        let mut c = pair.hub.pool.acquire().await.unwrap();
        let mut input = new_student(pair.sch.id, "Head Office Student");
        input.student_number = Some("HO-1".into());
        students::create(&mut c, &pair.hub_hlc, actor, input)
            .await
            .unwrap()
    };
    let local = add_student(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        "School Student",
    )
    .await;
    sqlx::query("UPDATE students SET student_number = $2, hlc = $3 WHERE id = $1")
        .bind(local.id)
        .bind(&twin.student_number)
        .bind(pair.school_hlc.now().to_string())
        .execute(&pair.school.pool)
        .await
        .unwrap();
    let (up, down) = pair.round().await;
    let parked_up: Vec<&str> = up.parked.iter().map(|p| p.reason.as_str()).collect();
    assert_eq!(parked_up.len(), 1, "{parked_up:?}");
    assert!(
        parked_up[0].starts_with("Another record here already uses the same value"),
        "{}",
        parked_up[0]
    );
    assert!(
        parked_up[0].contains("students_number_uq"),
        "{}",
        parked_up[0]
    );
    assert_eq!(
        down.parked.len(),
        1,
        "and the hub's student waits at the school for the same reason"
    );
    // Nothing else was held up.
    assert_eq!(status(&pair.school.pool).await.pending, 0);
    sqlx::query("UPDATE students SET student_number = 'S-RENUMBERED', hlc = $2 WHERE id = $1")
        .bind(local.id)
        .bind(pair.school_hlc.now().to_string())
        .execute(&pair.school.pool)
        .await
        .unwrap();
    pair.round().await;
    let (up, down) = pair.round().await;
    assert!(up.parked.is_empty() && down.parked.is_empty());
    for pool in [&pair.school.pool, &pair.hub.pool] {
        assert_eq!(student(pool, local.id).await.student_number, "S-RENUMBERED");
        assert_eq!(
            student(pool, twin.id).await.full_name,
            "Head Office Student"
        );
        assert_eq!(status(pool).await.parked, 0);
    }
    assert_eq!(
        table_rows(&pair.school.pool, "students").await,
        table_rows(&pair.hub.pool, "students").await
    );
    pair.finish().await;
}

#[tokio::test]
async fn a_record_of_another_school_is_refused() {
    let pair = pair!();
    let other = uuid::Uuid::now_v7();
    let forged = Change {
        table: "students".into(),
        id: uuid::Uuid::now_v7(),
        fields: None,
        row: json!({ "id": uuid::Uuid::now_v7(), "school_id": other, "full_name": "Forged" })
            .as_object()
            .unwrap()
            .clone(),
        stamps: Default::default(),
        hlc: pair.school_hlc.now().to_string(),
        seq: 1,
    };
    let unknown = Change {
        table: "pg_catalog.pg_authid; DROP TABLE students".into(),
        ..forged.clone()
    };
    let skipped = Change {
        table: "settings".into(),
        ..forged.clone()
    };
    let before = table_rows(&pair.hub.pool, "students").await.len();
    let mut h = pair.hub.pool.begin().await.unwrap();
    let report = sync::apply(&mut h, &pair.hub_hlc, &[forged, unknown, skipped])
        .await
        .unwrap();
    h.commit().await.unwrap();
    assert_eq!((report.applied, report.unchanged), (0, 3));
    assert!(report.parked.is_empty());
    assert_eq!(table_rows(&pair.hub.pool, "students").await.len(), before);
    let settings: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pair.hub.pool)
        .await
        .unwrap();
    assert_eq!(settings, 0);
    pair.finish().await;
}

#[tokio::test]
async fn leaving_removes_the_triggers_and_keeps_the_school_data() {
    let pair = pair!();
    let before = table_rows(&pair.school.pool, "students").await;
    let ali: uuid::Uuid = before[0]["id"].as_str().unwrap().parse().unwrap();
    let mut c = pair.school.pool.begin().await.unwrap();
    let installed = sync::triggers_installed(&mut c).await.unwrap();
    let expected = TABLES.iter().filter(|p| p.mode != Mode::Skip).count() as i64;
    assert_eq!(
        installed, expected,
        "one trigger per synced table while joined"
    );
    // Something still waiting when the school leaves is simply forgotten.
    sqlx::query("UPDATE students SET address = 'New address', hlc = $2 WHERE id = $1")
        .bind(ali)
        .bind(pair.school_hlc.now().to_string())
        .execute(&mut *c)
        .await
        .unwrap();
    assert_eq!(sync::status(&mut c).await.unwrap().pending, 1);
    sync::disable(&mut c).await.unwrap();
    c.commit().await.unwrap();

    let mut c = pair.school.pool.acquire().await.unwrap();
    assert_eq!(sync::triggers_installed(&mut c).await.unwrap(), 0);
    let st = sync::status(&mut c).await.unwrap();
    assert_eq!((st.enabled, st.pending, st.parked), (false, 0, 0));
    drop(c);
    // The school's records are all still there, and editing them no longer queues anything.
    assert_eq!(student(&pair.school.pool, ali).await.address, "New address");
    assert_eq!(
        table_rows(&pair.school.pool, "students").await.len(),
        before.len()
    );
    rename(
        &pair.school.pool,
        &pair.school_hlc,
        &pair.sch,
        ali,
        students::StudentUpdate {
            full_name: Some("After Leaving".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(status(&pair.school.pool).await.pending, 0);
    pair.finish().await;
}
