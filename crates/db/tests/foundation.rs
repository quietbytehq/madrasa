//! Integration tests for the foundation schema and repositories. Need PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`; skipped otherwise.

use madrasa_core::edition::Edition;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_core::permissions::{Permission, RoleCode};
use madrasa_core::session_token;
use madrasa_db::audit::{self, Action};
use madrasa_db::models::{School, User};
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, schools, sessions, settings, test_db, users, Actor, DbError, PgPool};
use serde_json::json;
use sqlx::Row;

fn hlc() -> Hlc {
    Hlc::new(0xabc)
}

async fn make_school(pool: &PgPool) -> School {
    let mut conn = pool.acquire().await.unwrap();
    schools::create(
        &mut conn,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Test School".into(),
            code: "tst01".into(),
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

async fn make_user(pool: &PgPool, school: &School, username: &str) -> User {
    let mut conn = pool.acquire().await.unwrap();
    users::create(
        &mut conn,
        &hlc(),
        Actor::system_for(school.id),
        users::NewUser {
            school_id: school.id,
            username: username.into(),
            full_name: "Test User".into(),
            email: None,
            password_hash: hash_password("a-long-password").unwrap(),
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn migrations_apply_and_tables_follow_the_rules() {
    let db: TestDb = test_db!();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name <> '_sqlx_migrations'
         ORDER BY table_name",
    )
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        tables,
        [
            "academic_years",
            "assessment_max_marks",
            "assessment_types",
            "attendance_entries",
            "attendance_sheets",
            "audit_log",
            "backup_runs",
            "class_subjects",
            "classes",
            "custom_field_definitions",
            "enrollments",
            "fee_concessions",
            "fee_heads",
            "fee_receipts",
            "fee_structures",
            "fee_voucher_lines",
            "fee_vouchers",
            "guardians",
            "holidays",
            "import_batch_rows",
            "import_batches",
            "mark_sheets",
            "marks",
            "permissions",
            "roles",
            "school_sequences",
            "schools",
            "section_teachers",
            "sections",
            "sessions",
            "settings",
            "staff",
            "staff_attendance",
            "student_guardians",
            "students",
            "subjects",
            "user_roles",
            "users",
        ]
    );

    for table in &tables {
        let cols: Vec<(String, String, String)> = sqlx::query(
            "SELECT column_name, data_type, is_nullable FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = $1",
        )
        .bind(table)
        .fetch_all(&db.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect();
        let col = |name: &str| cols.iter().find(|c| c.0 == name);

        // Rule 1: UUID primary key named id.
        let pk: String = sqlx::query_scalar(
            "SELECT a.attname FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
             WHERE i.indrelid = $1::regclass AND i.indisprimary",
        )
        .bind(table)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(pk, "id", "{table} primary key");
        assert_eq!(col("id").unwrap().1, "uuid", "{table}.id type");

        // Rule 1b: school_id everywhere except on schools itself.
        if table != "schools" {
            let c = col("school_id").unwrap_or_else(|| panic!("{table} lacks school_id"));
            assert_eq!(c.1, "uuid");
            if !matches!(table.as_str(), "settings" | "audit_log") {
                assert_eq!(c.2, "NO", "{table}.school_id must be NOT NULL");
            }
        }

        // Rule 2: sync columns on every mutable table; hlc everywhere.
        assert_eq!(col("hlc").unwrap().1, "text", "{table}.hlc");
        if table != "audit_log" && table != "fee_receipts" {
            for name in ["created_at", "updated_at", "deleted_at"] {
                assert_eq!(
                    col(name)
                        .unwrap_or_else(|| panic!("{table} lacks {name}"))
                        .1,
                    "timestamp with time zone"
                );
            }
            assert_eq!(col("updated_by").unwrap().1, "uuid", "{table}.updated_by");
        }
    }

    // Rule 5: core entities carry a JSONB custom column.
    for table in ["schools", "users", "students", "guardians", "staff"] {
        let t: String = sqlx::query_scalar(
            "SELECT data_type FROM information_schema.columns WHERE table_name = $1 AND column_name = 'custom'",
        )
        .bind(table)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(t, "jsonb");
    }
    db.cleanup().await;
}

#[tokio::test]
async fn audit_log_is_append_only() {
    let db: TestDb = test_db!();
    let school = make_school(&db.pool).await;
    let mut conn = db.pool.acquire().await.unwrap();
    let entries = audit::for_entity(&mut conn, "school", school.id)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    let id = entries[0].id;

    let upd = sqlx::query("UPDATE audit_log SET action = 'delete' WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await;
    assert!(upd.unwrap_err().to_string().contains("append-only"));
    let del = sqlx::query("DELETE FROM audit_log WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await;
    assert!(del.unwrap_err().to_string().contains("append-only"));
    let trunc = sqlx::query("TRUNCATE audit_log").execute(&mut *conn).await;
    assert!(trunc.unwrap_err().to_string().contains("append-only"));
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn every_create_update_delete_is_audited_with_old_and_new_values() {
    let db: TestDb = test_db!();
    let school = make_school(&db.pool).await;
    let admin = make_user(&db.pool, &school, "admin").await;
    let actor = Actor::user(admin.id, school.id);
    let mut conn = db.pool.acquire().await.unwrap();

    // create
    let created = audit::for_entity(&mut conn, "user", admin.id)
        .await
        .unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].action, "create");
    assert!(created[0].old_value.is_none());
    let new_value = created[0].new_value.clone().unwrap();
    assert_eq!(new_value["username"], "admin");
    assert!(
        new_value.get("password_hash").is_none(),
        "secrets must not be audited"
    );
    assert_eq!(created[0].school_id, Some(school.id));

    // update
    let updated = users::update(
        &mut conn,
        &hlc(),
        actor,
        admin.id,
        users::UserUpdate {
            full_name: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.full_name, "Renamed");
    assert_eq!(updated.updated_by, Some(admin.id));
    assert!(updated.updated_at > admin.updated_at);
    assert_ne!(updated.hlc, admin.hlc);

    // delete
    users::soft_delete(&mut conn, &hlc(), actor, admin.id)
        .await
        .unwrap();
    assert!(matches!(
        users::get(&mut conn, admin.id).await,
        Err(DbError::NotFound(_))
    ));

    let trail = audit::for_entity(&mut conn, "user", admin.id)
        .await
        .unwrap();
    let actions: Vec<_> = trail.iter().map(|e| e.action.as_str()).collect();
    assert_eq!(actions, ["create", "update", "delete"]);
    assert_eq!(
        trail[1].old_value.as_ref().unwrap()["full_name"],
        "Test User"
    );
    assert_eq!(trail[1].new_value.as_ref().unwrap()["full_name"], "Renamed");
    assert_eq!(trail[1].actor_user_id, Some(admin.id));
    assert_eq!(trail[2].old_value.as_ref().unwrap()["full_name"], "Renamed");
    assert!(trail[2].new_value.is_none());

    // Stamps are ordered.
    assert!(trail[0].hlc < trail[1].hlc && trail[1].hlc < trail[2].hlc);

    let recent = audit::list_recent(&mut conn, Some(school.id), 10)
        .await
        .unwrap();
    assert_eq!(recent[0].action, "delete");
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn school_codes_are_unique_case_insensitively() {
    let db: TestDb = test_db!();
    make_school(&db.pool).await;
    let mut conn = db.pool.acquire().await.unwrap();
    let dup = schools::create(
        &mut conn,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Other".into(),
            code: "TST01".into(),
            city: "".into(),
            is_demo: false,
            academic_year_start_month: 8,
            currency: "PKR".into(),
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
        },
    )
    .await;
    assert!(matches!(dup, Err(DbError::Conflict(_))));
    assert_eq!(schools::count(&mut conn, true).await.unwrap(), 1);
    assert_eq!(
        schools::current(&mut conn).await.unwrap().unwrap().code,
        "TST01"
    );
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn settings_upsert_audit_and_edition_default() {
    let db: TestDb = test_db!();
    let mut conn = db.pool.acquire().await.unwrap();
    assert_eq!(settings::edition(&mut conn).await.unwrap(), Edition::Lite);
    assert!(!settings::setup_completed(&mut conn).await.unwrap());

    settings::set_edition(&mut conn, &hlc(), Actor::SYSTEM, Edition::Nexus)
        .await
        .unwrap();
    assert_eq!(settings::edition(&mut conn).await.unwrap(), Edition::Nexus);
    settings::set_edition(&mut conn, &hlc(), Actor::SYSTEM, Edition::Lite)
        .await
        .unwrap();
    assert_eq!(settings::edition(&mut conn).await.unwrap(), Edition::Lite);

    let node_a = settings::node_id(&mut conn).await.unwrap();
    let node_b = settings::node_id(&mut conn).await.unwrap();
    assert_eq!(node_a, node_b);
    assert!(node_a <= 0xffff_ffff_ffff);

    // Secret settings are redacted in the audit log.
    let s = settings::set(
        &mut conn,
        &hlc(),
        Actor::SYSTEM,
        None,
        settings::KEY_BACKUP_KEYS,
        json!({"k": "secret"}),
    )
    .await
    .unwrap();
    let trail = audit::for_entity(&mut conn, "setting", s.id).await.unwrap();
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].new_value.as_ref().unwrap()["value"], "<redacted>");

    // Non-secret settings show old and new.
    let e = settings::set(
        &mut conn,
        &hlc(),
        Actor::SYSTEM,
        None,
        "greeting",
        json!("hi"),
    )
    .await
    .unwrap();
    settings::set(
        &mut conn,
        &hlc(),
        Actor::SYSTEM,
        None,
        "greeting",
        json!("hello"),
    )
    .await
    .unwrap();
    let trail = audit::for_entity(&mut conn, "setting", e.id).await.unwrap();
    assert_eq!(trail.len(), 2);
    assert_eq!(trail[1].old_value.as_ref().unwrap()["value"], "hi");
    assert_eq!(trail[1].new_value.as_ref().unwrap()["value"], "hello");
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn default_roles_grant_expected_permissions() {
    let db: TestDb = test_db!();
    let school = make_school(&db.pool).await;
    let mut conn = db.pool.acquire().await.unwrap();
    let created = roles::create_defaults(
        &mut conn,
        &hlc(),
        Actor::system_for(school.id),
        school.id,
        Edition::Lite,
    )
    .await
    .unwrap();
    assert_eq!(
        created.iter().map(|r| r.code.as_str()).collect::<Vec<_>>(),
        ["principal", "admin", "accountant", "teacher"]
    );
    assert!(
        roles::find_by_code(&mut conn, school.id, "parent")
            .await
            .is_err(),
        "parent is Nexus-only"
    );

    let teacher = make_user(&db.pool, &school, "teacher1").await;
    let role = roles::find_by_code(&mut conn, school.id, RoleCode::Teacher.code())
        .await
        .unwrap();
    roles::assign(
        &mut conn,
        &hlc(),
        Actor::system_for(school.id),
        teacher.id,
        &role,
    )
    .await
    .unwrap();
    let codes = roles::permission_codes_for_user(&mut conn, teacher.id)
        .await
        .unwrap();
    assert!(codes.contains(&Permission::AttendanceManage.code().to_string()));
    assert!(!codes.contains(&Permission::UsersManage.code().to_string()));
    assert_eq!(
        roles::codes_for_user(&mut conn, teacher.id).await.unwrap(),
        ["teacher"]
    );

    // Assigning twice is idempotent.
    roles::assign(
        &mut conn,
        &hlc(),
        Actor::system_for(school.id),
        teacher.id,
        &role,
    )
    .await
    .unwrap();
    assert_eq!(
        roles::codes_for_user(&mut conn, teacher.id).await.unwrap(),
        ["teacher"]
    );
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn sessions_round_trip_and_revoke() {
    let db: TestDb = test_db!();
    let school = make_school(&db.pool).await;
    let user = make_user(&db.pool, &school, "u").await;
    let mut conn = db.pool.acquire().await.unwrap();
    let now = chrono::Utc::now();
    let tok = session_token::generate();
    let session = sessions::create(
        &mut conn,
        &hlc(),
        &user,
        &tok.hash,
        now + chrono::Duration::hours(1),
    )
    .await
    .unwrap();

    let found = sessions::find_live(&mut conn, &session_token::hash(&tok.token), now)
        .await
        .unwrap();
    assert_eq!(
        found.as_ref().map(|(s, u)| (s.id, u.id)),
        Some((session.id, user.id))
    );
    assert!(sessions::find_live(&mut conn, "nope", now)
        .await
        .unwrap()
        .is_none());
    assert!(
        sessions::find_live(&mut conn, &tok.hash, now + chrono::Duration::hours(2))
            .await
            .unwrap()
            .is_none()
    );

    sessions::revoke(&mut conn, &hlc(), &session).await.unwrap();
    assert!(sessions::find_live(&mut conn, &tok.hash, now)
        .await
        .unwrap()
        .is_none());
    let trail = audit::for_entity(&mut conn, "session", session.id)
        .await
        .unwrap();
    assert_eq!(trail[0].action, Action::Logout.as_str());
    drop(conn);
    db.cleanup().await;
}

#[tokio::test]
async fn failed_logins_lock_and_successful_login_resets() {
    let db: TestDb = test_db!();
    let school = make_school(&db.pool).await;
    let user = make_user(&db.pool, &school, "u").await;
    let mut conn = db.pool.acquire().await.unwrap();
    let until = chrono::Utc::now() + chrono::Duration::minutes(15);
    users::record_failed_login(&mut conn, &hlc(), user.id, 5, Some(until))
        .await
        .unwrap();
    let locked = users::get(&mut conn, user.id).await.unwrap();
    assert_eq!(locked.failed_login_count, 5);
    assert!(locked.locked_until.is_some());
    let actions: Vec<_> = audit::for_entity(&mut conn, "user", user.id)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert_eq!(actions, ["create", "login_failed", "lockout"]);

    users::record_successful_login(&mut conn, &hlc(), user.id)
        .await
        .unwrap();
    let ok = users::get(&mut conn, user.id).await.unwrap();
    assert_eq!(ok.failed_login_count, 0);
    assert!(ok.locked_until.is_none());
    assert!(ok.last_login_at.is_some());
    drop(conn);
    db.cleanup().await;
}

#[test]
fn replace_database_keeps_host_and_params() {
    use madrasa_db::testing::replace_database;
    assert_eq!(
        replace_database("postgres://u:p@h:5/postgres?sslmode=disable", "x"),
        "postgres://u:p@h:5/x?sslmode=disable"
    );
    assert_eq!(
        replace_database("postgres://u@h/db", "x"),
        "postgres://u@h/x"
    );
    assert_eq!(replace_database("postgres://u@h", "x"), "postgres://u@h/x");
}
