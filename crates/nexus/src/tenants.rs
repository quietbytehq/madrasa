//! Each joined school's data lives in a PostgreSQL schema of its own (ADR 0005). This
//! module creates those schemas, keeps the register (`hub_schools`) and opens transactions
//! that can see exactly one school.

use chrono::{DateTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::sync::{Change, Origin};
use madrasa_db::sync;
use serde::Serialize;
use sqlx::{AssertSqlSafe, PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

/// A row of the school register.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HubSchool {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub code: String,
    #[serde(skip_serializing)]
    pub schema_name: String,
    pub status: String,
    pub school_version: Option<String>,
    pub school_schema: Option<i64>,
    pub edition: Option<String>,
    pub joined_at: Option<DateTime<Utc>>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub left_at: Option<DateTime<Utc>>,
    pub leave_reason: Option<String>,
    pub purged_at: Option<DateTime<Utc>>,
}

const COLS: &str =
    "id, school_id, name, code, schema_name, status, school_version, school_schema, \
    edition, joined_at, last_sync_at, left_at, leave_reason, purged_at";

fn select(tail: &str) -> AssertSqlSafe<String> {
    AssertSqlSafe(format!("SELECT {COLS} FROM nexus.hub_schools {tail}"))
}

/// The schema of a school: `t_` and the 32 hex digits of its register id.
pub fn schema_for(id: Uuid) -> String {
    format!("t_{}", id.simple())
}

/// Only names this module made are ever put into SQL text.
pub fn is_schema_name(name: &str) -> bool {
    named(name, "t_")
}

/// An archive of a school's copy that head office set aside: `a_` and 32 hex digits.
pub fn is_archive_name(name: &str) -> bool {
    named(name, "a_")
}

fn named(name: &str, prefix: &str) -> bool {
    name.len() == 34
        && name.starts_with(prefix)
        && name[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The schema version the hub's copies are on: the latest school migration this build knows.
pub fn hub_schema_version() -> i64 {
    madrasa_db::MIGRATOR
        .iter()
        .map(|m| m.version)
        .max()
        .unwrap_or(0)
}

pub async fn list(conn: &mut PgConnection) -> Result<Vec<HubSchool>, sqlx::Error> {
    sqlx::query_as::<_, HubSchool>(select(
        "WHERE deleted_at IS NULL ORDER BY (status = 'active') DESC, name, created_at",
    ))
    .fetch_all(conn)
    .await
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<HubSchool>, sqlx::Error> {
    sqlx::query_as::<_, HubSchool>(select("WHERE id = $1 AND deleted_at IS NULL"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn find_by_token(
    conn: &mut PgConnection,
    token_hash: &str,
) -> Result<Option<HubSchool>, sqlx::Error> {
    sqlx::query_as::<_, HubSchool>(select("WHERE token_hash = $1 AND deleted_at IS NULL"))
        .bind(token_hash)
        .fetch_optional(conn)
        .await
}

/// Active schools: the ones reports and sync look at.
pub async fn active(conn: &mut PgConnection) -> Result<Vec<HubSchool>, sqlx::Error> {
    sqlx::query_as::<_, HubSchool>(select(
        "WHERE status = 'active' AND deleted_at IS NULL ORDER BY name, created_at",
    ))
    .fetch_all(conn)
    .await
}

/// Open a transaction that sees one school's schema and nothing else: not the hub's own
/// tables, not another school's. The path is set for this transaction only, so the pooled
/// connection goes back to the hub schema when it ends.
pub async fn begin<'a>(
    pool: &'a PgPool,
    school: &HubSchool,
) -> Result<Transaction<'a, Postgres>, sqlx::Error> {
    if !is_schema_name(&school.schema_name) {
        return Err(sqlx::Error::Protocol(
            "this school has no data schema".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    sqlx::query(AssertSqlSafe(format!(
        "SET LOCAL search_path TO \"{}\"",
        school.schema_name
    )))
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}

/// A one-connection pool whose connection starts on a school's schema. For code that takes
/// a pool and must work inside one school (the migrator, the demo loader); the hub's own
/// pool is not touched. Close it when done.
pub(crate) async fn scoped_pool(pool: &PgPool, schema: &str) -> anyhow::Result<PgPool> {
    anyhow::ensure!(is_schema_name(schema), "not a school schema name");
    let set_path = format!("SET search_path TO \"{schema}\"");
    Ok(sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            let set_path = set_path.clone();
            Box::pin(async move {
                sqlx::query(AssertSqlSafe(set_path))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone())
        .await?)
}

/// Run the school migrations in a schema (creating it when new). The migrator needs the
/// path set for a whole session, so it gets a pool of its own on that schema.
pub(crate) async fn migrate_schema(
    pool: &PgPool,
    schema: &str,
    create: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(is_schema_name(schema), "not a school schema name");
    if create {
        sqlx::query(AssertSqlSafe(format!("CREATE SCHEMA \"{schema}\"")))
            .execute(pool)
            .await?;
    }
    let scoped = scoped_pool(pool, schema).await?;
    let result = madrasa_db::MIGRATOR.run(&scoped).await;
    scoped.close().await;
    result?;
    Ok(())
}

/// What a school sends to join.
pub struct NewTenant<'a> {
    pub invitation_id: Uuid,
    pub school: &'a Change,
    pub name: String,
    pub code: String,
    pub school_version: String,
    pub school_schema: i64,
    pub edition: String,
    pub token_hash: String,
}

/// Create a school's schema, migrate it, store the school's own record and switch change
/// capture on as the head office side. The invitation is used up in the same step that
/// reserves the register row, so a join code works once even if two requests race.
pub async fn create(
    pool: &PgPool,
    hlc: &Hlc,
    new: NewTenant<'_>,
) -> anyhow::Result<Option<HubSchool>> {
    let id = new_id();
    let schema = schema_for(id);
    let mut tx = pool.begin().await?;
    let free: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM hub_invitations
         WHERE id = $1 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(new.invitation_id)
    .fetch_optional(&mut *tx)
    .await?;
    if free.is_none() {
        return Ok(None);
    }
    sqlx::query(
        "INSERT INTO hub_schools (id, school_id, name, code, schema_name, status, school_version,
                                  school_schema, edition, hlc)
         VALUES ($1, $2, $3, $4, $5, 'joining', $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(new.school.id)
    .bind(&new.name)
    .bind(&new.code)
    .bind(&schema)
    .bind(&new.school_version)
    .bind(new.school_schema)
    .bind(&new.edition)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE hub_invitations SET used_at = now(), used_by = $2, updated_at = now(), hlc = $3 WHERE id = $1")
        .bind(new.invitation_id)
        .bind(id)
        .bind(hlc.now().to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let built: anyhow::Result<()> = async {
        migrate_schema(pool, &schema, true).await?;
        let school = HubSchool {
            id,
            school_id: new.school.id,
            name: new.name.clone(),
            code: new.code.clone(),
            schema_name: schema.clone(),
            status: "joining".into(),
            school_version: None,
            school_schema: None,
            edition: None,
            joined_at: None,
            last_sync_at: None,
            left_at: None,
            leave_reason: None,
            purged_at: None,
        };
        let mut t = begin(pool, &school).await?;
        sync::adopt_school(&mut t, new.school, Origin::HeadOffice).await?;
        t.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = built {
        // Leave nothing half-made: drop the schema, free the invitation, forget the row.
        tracing::error!(error = %e, "creating a school schema failed");
        let _ = sqlx::query(AssertSqlSafe(format!(
            "DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"
        )))
        .execute(pool)
        .await;
        let _ =
            sqlx::query("UPDATE hub_invitations SET used_at = NULL, used_by = NULL WHERE id = $1")
                .bind(new.invitation_id)
                .execute(pool)
                .await;
        let _ = sqlx::query("DELETE FROM hub_schools WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await;
        return Err(e);
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE hub_schools SET status = 'active', token_hash = $2, joined_at = now(), updated_at = now(), hlc = $3
         WHERE id = $1",
    )
    .bind(id)
    .bind(&new.token_hash)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    let school = get(&mut tx, id).await?;
    crate::store::audit(
        &mut tx,
        hlc,
        None,
        "join",
        "hub_school",
        Some(id),
        None,
        Some(serde_json::json!(school)),
    )
    .await?;
    tx.commit().await?;
    Ok(school)
}

/// How a school stops being active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The school left by itself: its token is forgotten.
    Left,
    /// Head office removed it: the token is kept so the school can be told at its next sync.
    Removed,
}

/// End a school's membership. Its schema stays as an archive, with change capture off, and
/// is left out of every report until it is purged.
pub async fn end(
    pool: &PgPool,
    hlc: &Hlc,
    actor: Option<Uuid>,
    school: &HubSchool,
    ending: Ending,
    reason: &str,
) -> anyhow::Result<HubSchool> {
    let mut t = begin(pool, school).await?;
    sync::disable(&mut t).await?;
    t.commit().await?;
    let (status, keep_token) = match ending {
        Ending::Left => ("left", false),
        Ending::Removed => ("removed", true),
    };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE hub_schools SET status = $2, left_at = now(), leave_reason = $3,
             token_hash = CASE WHEN $4 THEN token_hash ELSE NULL END,
             updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1",
    )
    .bind(school.id)
    .bind(status)
    .bind(reason.trim())
    .bind(keep_token)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    // A school that is no longer joined could not be told about a release: its sealed
    // backup key is erased now, not kept with the archive.
    crate::escrow::end_deposit(&mut tx, hlc, actor, school.id, status).await?;
    // Its parents can no longer see anything of it.
    crate::parents::end_for_school(&mut tx, hlc, school.id, status).await?;
    let after = get(&mut tx, school.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("school vanished"))?;
    crate::store::audit(
        &mut tx,
        hlc,
        actor,
        status,
        "hub_school",
        Some(school.id),
        Some(serde_json::json!(school)),
        Some(serde_json::json!(after)),
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// What setting a school's copy aside did.
#[derive(Debug, Clone)]
pub struct Rebaselined {
    /// The archive schema now holding the old copy.
    pub archive: String,
    pub at: chrono::DateTime<chrono::Utc>,
}

/// A school restored a backup while joined (ADR 0007). Its records are now older
/// than head office's copy, so that copy is set aside as an archive and a fresh one is
/// started from the school record, exactly as when the school joined; the school then sends
/// every record again into it. Nothing in the fresh copy can clash with what the school
/// holds, because it is made from nothing else.
///
/// The old copy is renamed, not changed: what head office held, including corrections and
/// parents' messages the school had not received yet, stays readable there until the school
/// is purged.
pub async fn rebaseline(
    pool: &PgPool,
    hlc: &Hlc,
    school: &HubSchool,
    record: &Change,
    backup_created_at: Option<chrono::DateTime<chrono::Utc>>,
    restored_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Rebaselined> {
    anyhow::ensure!(
        is_schema_name(&school.schema_name),
        "this school has no data schema"
    );
    anyhow::ensure!(
        record.table == "schools" && record.fields.is_none() && record.id == school.school_id,
        "the school record does not belong to this school"
    );
    let archive_id = new_id();
    let archive = format!("a_{}", archive_id.simple());

    // 1. Set the old copy aside: capture off, then renamed.
    let mut t = begin(pool, school).await?;
    sync::disable(&mut t).await?;
    t.commit().await?;
    let mut tx = pool.begin().await?;
    sqlx::query(AssertSqlSafe(format!(
        "ALTER SCHEMA \"{}\" RENAME TO \"{archive}\"",
        school.schema_name
    )))
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO hub_school_archives (id, hub_school_id, schema_name, reason, backup_created_at,
                                          restored_at, hlc)
         VALUES ($1, $2, $3, 'restored', $4, $5, $6)",
    )
    .bind(archive_id)
    .bind(school.id)
    .bind(&archive)
    .bind(backup_created_at)
    .bind(restored_at)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // 2. A fresh copy under the school's own schema name, from its school record.
    let built: anyhow::Result<()> = async {
        migrate_schema(pool, &school.schema_name, true).await?;
        let mut t = begin(pool, school).await?;
        sync::adopt_school(&mut t, record, Origin::HeadOffice).await?;
        t.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = built {
        // Put the old copy back as it was, so the school can try again.
        tracing::error!(error = %e, "starting a fresh copy after a restore failed");
        let undo: anyhow::Result<()> = async {
            let mut tx = pool.begin().await?;
            sqlx::query(AssertSqlSafe(format!(
                "DROP SCHEMA IF EXISTS \"{}\" CASCADE",
                school.schema_name
            )))
            .execute(&mut *tx)
            .await?;
            sqlx::query(AssertSqlSafe(format!(
                "ALTER SCHEMA \"{archive}\" RENAME TO \"{}\"",
                school.schema_name
            )))
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM hub_school_archives WHERE id = $1")
                .bind(archive_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            let mut t = begin(pool, school).await?;
            sync::enable(&mut t, school.school_id, Origin::HeadOffice).await?;
            t.commit().await?;
            Ok(())
        }
        .await;
        if let Err(undo) = undo {
            tracing::error!(error = %undo, "putting the old copy back failed");
        }
        return Err(e);
    }

    // 3. Logged at head office; the school logs it on its side when it hears back.
    let at = chrono::Utc::now();
    let mut tx = pool.begin().await?;
    crate::store::audit(
        &mut tx,
        hlc,
        None,
        "school_restored",
        "hub_school",
        Some(school.id),
        None,
        Some(serde_json::json!({
            "archive": archive, "backup_created_at": backup_created_at, "restored_at": restored_at,
            "note": "the school restored a backup; head office's copy was set aside and started afresh",
        })),
    )
    .await?;
    touch(&mut tx, hlc, school.id, None, None).await?;
    tx.commit().await?;
    Ok(Rebaselined { archive, at })
}

/// Archives of a school's earlier copies that still exist.
pub async fn archives(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT schema_name FROM hub_school_archives
         WHERE hub_school_id = $1 AND dropped_at IS NULL AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(hub_school_id)
    .fetch_all(conn)
    .await
}

/// Delete a school's data for good: drop its schema. Nothing of another school is touched.
pub async fn purge(
    pool: &PgPool,
    hlc: &Hlc,
    actor: Option<Uuid>,
    school: &HubSchool,
) -> anyhow::Result<HubSchool> {
    anyhow::ensure!(is_schema_name(&school.schema_name), "nothing to purge");
    let mut tx = pool.begin().await?;
    sqlx::query(AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS \"{}\" CASCADE",
        school.schema_name
    )))
    .execute(&mut *tx)
    .await?;
    // Copies set aside after a restore go too.
    for archive in archives(&mut tx, school.id).await? {
        if is_archive_name(&archive) {
            sqlx::query(AssertSqlSafe(format!(
                "DROP SCHEMA IF EXISTS \"{archive}\" CASCADE"
            )))
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "UPDATE hub_school_archives SET dropped_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE hub_school_id = $1 AND dropped_at IS NULL",
    )
    .bind(school.id)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE hub_schools SET status = 'purged', schema_name = '', token_hash = NULL, purged_at = now(),
             updated_at = now(), updated_by = $2, hlc = $3
         WHERE id = $1",
    )
    .bind(school.id)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *tx)
    .await?;
    crate::escrow::end_deposit(&mut tx, hlc, actor, school.id, "purged").await?;
    crate::parents::end_for_school(&mut tx, hlc, school.id, "purged").await?;
    let after = get(&mut tx, school.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("school vanished"))?;
    crate::store::audit(
        &mut tx,
        hlc,
        actor,
        "purge",
        "hub_school",
        Some(school.id),
        Some(serde_json::json!(school)),
        Some(serde_json::json!(after)),
    )
    .await?;
    tx.commit().await?;
    Ok(after)
}

/// Note that a school synced just now, and what it runs.
pub async fn touch(
    conn: &mut PgConnection,
    hlc: &Hlc,
    id: Uuid,
    school_version: Option<&str>,
    school_schema: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE nexus.hub_schools SET last_sync_at = now(),
             school_version = COALESCE($2, school_version), school_schema = COALESCE($3, school_schema),
             updated_at = now(), hlc = $4
         WHERE id = $1",
    )
    .bind(id)
    .bind(school_version)
    .bind(school_schema)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// After a hub upgrade: bring every school's schema to the hub's version and make sure
/// change capture covers any new table. Called at start.
pub async fn migrate_all(pool: &PgPool) -> anyhow::Result<usize> {
    let mut conn = pool.acquire().await?;
    let schools: Vec<HubSchool> = sqlx::query_as::<_, HubSchool>(select(
        "WHERE status IN ('active', 'left', 'removed') AND deleted_at IS NULL",
    ))
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);
    for school in &schools {
        migrate_schema(pool, &school.schema_name, false).await?;
        if school.status == "active" {
            let mut t = begin(pool, school).await?;
            sync::enable(&mut t, school.school_id, Origin::HeadOffice).await?;
            t.commit().await?;
        }
    }
    Ok(schools.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_names_are_strict() {
        let id = Uuid::now_v7();
        let name = schema_for(id);
        assert!(is_schema_name(&name), "{name}");
        for bad in [
            "",
            "public",
            "nexus",
            "t_",
            "t_0123456789abcdef0123456789abcdeg",
            "t_0123456789ABCDEF0123456789ABCDEF",
            "t_0123456789abcdef0123456789abcdef\"; DROP SCHEMA nexus; --",
            "x_0123456789abcdef0123456789abcdef",
        ] {
            assert!(!is_schema_name(bad), "{bad}");
        }
    }

    #[test]
    fn the_hub_knows_its_schema_version() {
        assert!(hub_schema_version() >= 27);
    }
}
