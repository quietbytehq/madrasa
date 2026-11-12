//! A demo network for trying the hub out: `madrasa-nexus demo-network --yes` fills an empty
//! hub with three made-up schools, each in a schema of its own, as if they had joined and
//! synced. The people, fees and marks are generated (crates/demo-data); nothing is real.
//!
//! The demo schools have no sync key: no installation can sync as one of them. Remove and
//! purge them from the console when you are done.
//!
//! To try the parent app against the demo network, [`parent_codes`] gives one access code
//! for a made-up guardian of each demo school (`madrasa-nexus demo-parent-codes`).

use chrono::{DateTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::parent_app::{generate_access_code, normalize_access_code};
use madrasa_core::session_token;
use madrasa_core::sync::Origin;
use madrasa_db::sync;
use madrasa_demo_data::{generate, load, DemoOptions, LoadOptions};
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use crate::tenants::{self, HubSchool};
use crate::{parents, store, VERSION};

/// One made-up school of the demo network.
#[derive(Debug, Clone, Copy)]
pub struct DemoSchool {
    pub name: &'static str,
    pub code: &'static str,
    pub city: &'static str,
    pub currency: &'static str,
    pub seed: u64,
    pub students: usize,
    pub staff: usize,
    pub sections_per_grade: usize,
}

/// Three schools of different sizes; one bills in another currency, so the fees report
/// shows that money is never added across currencies.
pub const DEMO_NETWORK: [DemoSchool; 3] = [
    DemoSchool {
        name: "DEMO North Campus",
        code: "DEMO-N",
        city: "Karachi",
        currency: "PKR",
        seed: 2026,
        students: 320,
        staff: 40,
        sections_per_grade: 2,
    },
    DemoSchool {
        name: "DEMO South Campus",
        code: "DEMO-S",
        city: "Hyderabad",
        currency: "PKR",
        seed: 2027,
        students: 220,
        staff: 30,
        sections_per_grade: 1,
    },
    DemoSchool {
        name: "DEMO International Campus",
        code: "DEMO-I",
        city: "Dubai",
        currency: "AED",
        seed: 2028,
        students: 140,
        staff: 24,
        sections_per_grade: 1,
    },
];

#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    #[error("the demo network is for an empty hub: this hub already has schools")]
    HubNotEmpty,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Fill an empty hub with the demo network. Returns the schools created.
pub async fn seed_network(pool: &PgPool, hlc: &Hlc) -> Result<Vec<HubSchool>, DemoError> {
    let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM hub_schools")
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
    if existing > 0 {
        return Err(DemoError::HubNotEmpty);
    }
    let mut created = Vec::with_capacity(DEMO_NETWORK.len());
    for spec in DEMO_NETWORK {
        created.push(seed_school(pool, hlc, &spec).await?);
    }
    Ok(created)
}

async fn seed_school(pool: &PgPool, hlc: &Hlc, spec: &DemoSchool) -> anyhow::Result<HubSchool> {
    let id = new_id();
    let schema = tenants::schema_for(id);
    let built = fill_schema(pool, hlc, &schema, spec).await;
    let school_id = match built {
        Ok(school_id) => school_id,
        Err(e) => {
            // Leave nothing half-made.
            let _ = sqlx::query(AssertSqlSafe(format!(
                "DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"
            )))
            .execute(pool)
            .await;
            return Err(e);
        }
    };
    let mut tx = pool.begin().await?;
    // No sync key: nothing can sync as a demo school.
    sqlx::query(
        "INSERT INTO hub_schools (id, school_id, name, code, schema_name, status, school_version,
                                  school_schema, edition, joined_at, last_sync_at, hlc)
         VALUES ($1, $2, $3, $4, $5, 'active', $6, $7, 'lite', now(), now(), $8)",
    )
    .bind(id)
    .bind(school_id)
    .bind(spec.name)
    .bind(spec.code)
    .bind(&schema)
    .bind(VERSION)
    .bind(tenants::hub_schema_version())
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    let school = tenants::get(&mut tx, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the demo school vanished"))?;
    store::audit(
        &mut tx,
        hlc,
        None,
        "demo_school",
        "hub_school",
        Some(id),
        None,
        Some(serde_json::json!(school)),
    )
    .await?;
    tx.commit().await?;
    Ok(school)
}

/// Create the schema, load a generated school into it and switch change capture on as the
/// head office side, exactly as for a school that joined. Returns the school's own id.
async fn fill_schema(
    pool: &PgPool,
    hlc: &Hlc,
    schema: &str,
    spec: &DemoSchool,
) -> anyhow::Result<uuid::Uuid> {
    tenants::migrate_schema(pool, schema, true).await?;
    let scoped = tenants::scoped_pool(pool, schema).await?;
    let demo = generate(&DemoOptions {
        seed: spec.seed,
        students: spec.students,
        staff: spec.staff,
        sections_per_grade: spec.sections_per_grade,
        today: None,
    });
    let loaded = load(
        &scoped,
        hlc,
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: false,
        },
    )
    .await;
    let result: anyhow::Result<uuid::Uuid> = async {
        let report = loaded?;
        sqlx::query(
            "UPDATE schools SET name = $2, code = $3, city = $4, currency = $5, hlc = $6 WHERE id = $1",
        )
        .bind(report.school_id)
        .bind(spec.name)
        .bind(spec.code)
        .bind(spec.city)
        .bind(spec.currency)
        .bind(hlc.now().to_string())
        .execute(&scoped)
        .await?;
        // A hub copy never holds password hashes (they are not synced); nor does this one.
        sqlx::query("UPDATE users SET password_hash = '!'")
            .execute(&scoped)
            .await?;
        let mut conn = scoped.acquire().await?;
        sync::enable(&mut conn, report.school_id, Origin::HeadOffice).await?;
        Ok(report.school_id)
    }
    .await;
    scoped.close().await;
    result
}

/// A parent app access code for a made-up guardian of a demo school.
#[derive(Debug, Clone)]
pub struct DemoParentCode {
    pub school_name: String,
    pub guardian_name: String,
    pub children: Vec<String>,
    /// Shown once, like every access code: only its hash is kept.
    pub code: String,
    pub expires_at: DateTime<Utc>,
}

/// One fresh access code for each school of the demo network, for trying the parent app:
/// the guardian of the demo conversation where there is one. Each code works once and for
/// fourteen days, and replaces an unused earlier code of that guardian.
///
/// Only for schools the demo network made (marked demo in their own records, and without a
/// sync key). A real school's guardians get their codes from the school, never from here.
pub async fn parent_codes(pool: &PgPool, hlc: &Hlc) -> anyhow::Result<Vec<DemoParentCode>> {
    let schools: Vec<HubSchool> = {
        let mut conn = pool.acquire().await?;
        let demo_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM hub_schools
             WHERE status = 'active' AND token_hash IS NULL AND deleted_at IS NULL
             ORDER BY name, created_at",
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut found = Vec::with_capacity(demo_ids.len());
        for id in demo_ids {
            if let Some(school) = tenants::get(&mut conn, id).await? {
                found.push(school);
            }
        }
        found
    };
    let mut out = Vec::new();
    for school in schools {
        let mut tx = tenants::begin(pool, &school).await?;
        let is_demo: bool = sqlx::query_scalar("SELECT is_demo FROM schools WHERE id = $1")
            .bind(school.school_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(false);
        if !is_demo {
            continue;
        }
        let guardian: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT g.id, g.full_name FROM guardians g
             WHERE g.deleted_at IS NULL AND EXISTS (
                 SELECT 1 FROM student_guardians sg
                 JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
                 WHERE sg.guardian_id = g.id AND sg.deleted_at IS NULL)
             ORDER BY EXISTS (SELECT 1 FROM messages m WHERE m.guardian_id = g.id) DESC,
                      g.full_name, g.id
             LIMIT 1",
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some((guardian_id, guardian_name)) = guardian else {
            continue;
        };
        let children: Vec<String> = sqlx::query_scalar(
            "SELECT s.full_name FROM student_guardians sg
             JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
             WHERE sg.guardian_id = $1 AND sg.deleted_at IS NULL ORDER BY s.full_name, s.id",
        )
        .bind(guardian_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let code = generate_access_code();
        let hash = session_token::hash(
            &normalize_access_code(&code)
                .ok_or_else(|| anyhow::anyhow!("a generated access code did not normalize"))?,
        );
        let mut tx = pool.begin().await?;
        let expires_at = parents::issue_code(
            &mut tx,
            hlc,
            school.id,
            guardian_id,
            &guardian_name,
            &hash,
            Utc::now(),
        )
        .await?;
        // As for any code: the log says one was issued and for whom, never the code.
        store::audit(
            &mut tx,
            hlc,
            None,
            "parent_code",
            "hub_school",
            Some(school.id),
            None,
            Some(serde_json::json!({ "guardian_id": guardian_id, "demo": true })),
        )
        .await?;
        tx.commit().await?;
        out.push(DemoParentCode {
            school_name: school.name.clone(),
            guardian_name,
            children,
            code,
            expires_at,
        });
    }
    Ok(out)
}
