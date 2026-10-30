//! Load a generated demo school into the database. Requires explicit confirmation and refuses
//! to touch a database that already holds a real (non-demo) school.

use crate::generate::DemoSchool;
use crate::DEMO_PASSWORD;
use chrono::Utc;
use madrasa_core::edition::Edition;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::{roles, schools, settings, users, Actor, PgPool};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Must be `true`; the loader never runs implicitly.
    pub confirmed: bool,
    /// Where to write `students.json` and `classes.json` until those tables exist (M1).
    pub out_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadReport {
    pub school_id: uuid::Uuid,
    pub staff_accounts: usize,
    pub classes_generated: usize,
    pub students_generated: usize,
    pub files_written: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("refusing to load demo data without explicit confirmation")]
    NotConfirmed,
    #[error("refusing to load demo data: this database already contains a real school")]
    RealSchoolPresent,
    #[error("demo data is already loaded")]
    AlreadyLoaded,
    #[error(transparent)]
    Db(#[from] madrasa_db::DbError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Insert the demo school, roles and staff accounts in one transaction.
pub async fn load(
    pool: &PgPool,
    hlc: &Hlc,
    demo: &DemoSchool,
    options: &LoadOptions,
) -> Result<LoadReport, LoadError> {
    if !options.confirmed {
        return Err(LoadError::NotConfirmed);
    }
    let mut tx = pool.begin().await.map_err(madrasa_db::DbError::from)?;
    if schools::count(&mut tx, false).await? > 0 {
        return Err(LoadError::RealSchoolPresent);
    }
    if schools::count(&mut tx, true).await? > 0 {
        return Err(LoadError::AlreadyLoaded);
    }

    let school = schools::create(
        &mut tx,
        hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: demo.school.name.clone(),
            code: demo.school.code.clone(),
            city: demo.school.city.clone(),
            is_demo: true,
            academic_year_start_month: demo.school.academic_year_start_month,
            currency: demo.school.currency.clone(),
        },
    )
    .await?;
    let actor = Actor::system_for(school.id);
    let created_roles =
        roles::create_defaults(&mut tx, hlc, actor, school.id, Edition::Lite).await?;

    // One hash for all demo accounts (hashing 120 passwords would take a minute).
    let password_hash = hash_password(DEMO_PASSWORD).map_err(|e| anyhow::anyhow!(e))?;
    for staff in &demo.staff {
        let user = users::create(
            &mut tx,
            hlc,
            actor,
            users::NewUser {
                school_id: school.id,
                username: staff.username.clone(),
                full_name: staff.full_name.clone(),
                email: None,
                password_hash: password_hash.clone(),
            },
        )
        .await?;
        if let Some(role) = created_roles.iter().find(|r| r.code == staff.role) {
            roles::assign(&mut tx, hlc, actor, user.id, role).await?;
        }
    }

    settings::set_edition(&mut tx, hlc, Actor::SYSTEM, Edition::Lite).await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        Some(school.id),
        settings::KEY_ACADEMIC_YEAR_START_MONTH,
        json!(4),
    )
    .await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        Some(school.id),
        settings::KEY_CURRENCY,
        json!("PKR"),
    )
    .await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        None,
        settings::KEY_SETUP_COMPLETED_AT,
        json!(Utc::now()),
    )
    .await?;
    tx.commit().await.map_err(madrasa_db::DbError::from)?;

    let mut files_written = Vec::new();
    if let Some(dir) = &options.out_dir {
        files_written = write_json(dir, demo)?;
    }

    Ok(LoadReport {
        school_id: school.id,
        staff_accounts: demo.staff.len(),
        classes_generated: demo.classes.len(),
        students_generated: demo.students.len(),
        files_written,
    })
}

/// Write the generated classes and students as JSON (no tables for them yet).
pub fn write_json(dir: &Path, demo: &DemoSchool) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for (name, value) in [
        ("school.json", serde_json::to_value(&demo.school)?),
        ("staff.json", serde_json::to_value(&demo.staff)?),
        ("classes.json", serde_json::to_value(&demo.classes)?),
        ("students.json", serde_json::to_value(&demo.students)?),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, serde_json::to_string_pretty(&value)?)?;
        written.push(path);
    }
    Ok(written)
}
