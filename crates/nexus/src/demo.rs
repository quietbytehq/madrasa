//! A demo network for trying the hub out: `madrasa-nexus demo-network --yes` fills an empty
//! hub with three made-up schools, each in a schema of its own, as if they had joined and
//! synced. The people, fees and marks are generated (crates/demo-data); nothing is real.
//!
//! The demo schools have no sync key: no installation can sync as one of them. Remove and
//! purge them from the console when you are done.

use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::sync::Origin;
use madrasa_db::sync;
use madrasa_demo_data::{generate, load, DemoOptions, LoadOptions};
use sqlx::{AssertSqlSafe, PgPool};

use crate::tenants::{self, HubSchool};
use crate::{store, VERSION};

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
