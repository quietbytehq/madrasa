//! Pilot onboarding: the data health page (what in the school's records needs a person's
//! attention after an import) and the one-click export of everything.

use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::audit::{self, Action};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/data-health", get(data_health))
        .route("/export/everything", get(export_everything))
}

/// One record a check found. `link` is where in the app to fix it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Found {
    pub label: String,
    pub detail: String,
    pub link: Option<String>,
}

/// One check: how many records it found and the first few of them. Mirrored by
/// `HealthCheck` in the UI, which words each `code`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub code: &'static str,
    /// `problem`: something will go wrong (a child with no class gets no voucher).
    /// `warning`: worth a look.
    pub severity: &'static str,
    /// `students`, `guardians`, `fees` or `classes`: where the check belongs on the page.
    pub area: &'static str,
    pub count: i64,
    pub sample: Vec<Found>,
}

const SAMPLE: i64 = 25;

/// The checks: code, severity, area, the permission that may see it, and a query giving
/// `label`, `detail`, `link` and the full count as `total` (`$1` school, `$2` sample size).
/// Every query looks at records in use only: active students, the active year.
const CHECKS: &[(&str, &str, &str, Permission, &str)] = &[
    (
        "students_without_section",
        "problem",
        "students",
        Permission::StudentsRead,
        "SELECT s.full_name AS label, s.student_number AS detail, '/students/' || s.id AS link, count(*) OVER () AS total
         FROM students s
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
           AND NOT EXISTS (SELECT 1 FROM enrollments e JOIN academic_years y ON y.id = e.academic_year_id
                           WHERE e.student_id = s.id AND e.deleted_at IS NULL AND y.is_active AND y.deleted_at IS NULL)
         ORDER BY s.student_number LIMIT $2",
    ),
    (
        "students_in_two_sections",
        "problem",
        "students",
        Permission::StudentsRead,
        "SELECT s.full_name AS label, s.student_number || ': ' || string_agg(c.name || '-' || sec.name, ', ' ORDER BY c.level, sec.name) AS detail,
                '/students/' || s.id AS link, count(*) OVER () AS total
         FROM students s JOIN enrollments e ON e.student_id = s.id AND e.deleted_at IS NULL
         JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         JOIN sections sec ON sec.id = e.section_id JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
         GROUP BY s.id HAVING count(*) > 1 ORDER BY s.student_number LIMIT $2",
    ),
    (
        "students_without_guardian",
        "problem",
        "students",
        Permission::StudentsRead,
        "SELECT s.full_name AS label, s.student_number AS detail, '/students/' || s.id AS link, count(*) OVER () AS total
         FROM students s
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
           AND NOT EXISTS (SELECT 1 FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
                           WHERE sg.student_id = s.id AND sg.deleted_at IS NULL)
         ORDER BY s.student_number LIMIT $2",
    ),
    (
        "duplicate_student_names",
        "warning",
        "students",
        Permission::StudentsRead,
        "SELECT min(s.full_name) AS label,
                'Father ' || min(s.father_name) || ': ' || string_agg(s.student_number, ', ' ORDER BY s.student_number) AS detail,
                '/students?q=' || replace(min(s.full_name), ' ', '%20') AS link, count(*) OVER () AS total
         FROM students s
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
         GROUP BY lower(regexp_replace(trim(s.full_name), '\\s+', ' ', 'g')), lower(regexp_replace(trim(s.father_name), '\\s+', ' ', 'g'))
         HAVING count(*) > 1 ORDER BY 1 LIMIT $2",
    ),
    (
        "doubtful_dates_of_birth",
        "warning",
        "students",
        Permission::StudentsRead,
        "SELECT s.full_name AS label, s.student_number || ', born ' || s.date_of_birth AS detail, '/students/' || s.id AS link,
                count(*) OVER () AS total
         FROM students s
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
           AND (s.date_of_birth < DATE '1990-01-01' OR s.date_of_birth > current_date - INTERVAL '2 years')
         ORDER BY s.student_number LIMIT $2",
    ),
    (
        "duplicate_roll_numbers",
        "warning",
        "students",
        Permission::StudentsRead,
        "SELECT c.name || '-' || sec.name || ', roll no. ' || e.roll_no AS label,
                string_agg(s.full_name, ', ' ORDER BY s.full_name) AS detail, NULL::text AS link, count(*) OVER () AS total
         FROM enrollments e JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         JOIN sections sec ON sec.id = e.section_id JOIN classes c ON c.id = sec.class_id
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE e.school_id = $1 AND e.deleted_at IS NULL AND e.roll_no IS NOT NULL
         GROUP BY c.level, c.name, sec.name, sec.id, e.roll_no HAVING count(*) > 1 ORDER BY c.level, sec.name, e.roll_no LIMIT $2",
    ),
    (
        "guardians_without_phone",
        "problem",
        "guardians",
        Permission::StudentsRead,
        "SELECT g.full_name AS label, string_agg(s.full_name || ' (' || s.student_number || ')', ', ' ORDER BY s.student_number) AS detail,
                '/students/' || (array_agg(s.id ORDER BY s.student_number))[1] AS link, count(*) OVER () AS total
         FROM guardians g JOIN student_guardians sg ON sg.guardian_id = g.id AND sg.deleted_at IS NULL
         JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE g.school_id = $1 AND g.deleted_at IS NULL AND coalesce(g.phone, '') = ''
         GROUP BY g.id ORDER BY g.full_name LIMIT $2",
    ),
    (
        "guardian_phone_shared",
        "warning",
        "guardians",
        Permission::StudentsRead,
        "SELECT g.phone AS label, string_agg(DISTINCT g.full_name, ', ') AS detail, NULL::text AS link, count(*) OVER () AS total
         FROM guardians g
         WHERE g.school_id = $1 AND g.deleted_at IS NULL AND coalesce(g.phone, '') <> ''
           AND EXISTS (SELECT 1 FROM student_guardians sg WHERE sg.guardian_id = g.id AND sg.deleted_at IS NULL)
         GROUP BY g.phone HAVING count(DISTINCT lower(trim(g.full_name))) > 1 ORDER BY g.phone LIMIT $2",
    ),
    (
        "classes_without_fee_structure",
        "problem",
        "fees",
        Permission::FeesRead,
        "SELECT 'Class ' || c.name AS label, count(DISTINCT e.student_id) || ' students' AS detail, '/fees' AS link, count(*) OVER () AS total
         FROM classes c JOIN sections sec ON sec.class_id = c.id AND sec.deleted_at IS NULL
         JOIN academic_years y ON y.id = sec.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         JOIN enrollments e ON e.section_id = sec.id AND e.deleted_at IS NULL
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE c.school_id = $1 AND c.deleted_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM fee_structures fs WHERE fs.class_id = c.id AND fs.academic_year_id = y.id AND fs.deleted_at IS NULL)
         GROUP BY c.id ORDER BY c.level LIMIT $2",
    ),
    (
        "vouchers_without_fee_head",
        "problem",
        "fees",
        Permission::FeesRead,
        "SELECT v.voucher_no AS label, s.full_name || ' (' || s.student_number || '), ' || to_char(v.billing_month, 'Mon YYYY') AS detail,
                '/fees/vouchers/' || v.id || '/print' AS link, count(*) OVER () AS total
         FROM fee_vouchers v JOIN students s ON s.id = v.student_id
         WHERE v.school_id = $1 AND v.deleted_at IS NULL AND v.status <> 'cancelled'
           AND (NOT EXISTS (SELECT 1 FROM fee_voucher_lines l WHERE l.voucher_id = v.id AND l.deleted_at IS NULL)
             OR EXISTS (SELECT 1 FROM fee_voucher_lines l WHERE l.voucher_id = v.id AND l.deleted_at IS NULL AND l.fee_head_id IS NULL))
         ORDER BY v.billing_month DESC, v.voucher_no LIMIT $2",
    ),
    (
        "sections_without_class_teacher",
        "warning",
        "classes",
        Permission::StudentsRead,
        "SELECT c.name || '-' || sec.name AS label, count(e.id) || ' students' AS detail, '/settings' AS link, count(*) OVER () AS total
         FROM sections sec JOIN classes c ON c.id = sec.class_id
         JOIN academic_years y ON y.id = sec.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         JOIN enrollments e ON e.section_id = sec.id AND e.deleted_at IS NULL
         WHERE sec.school_id = $1 AND sec.deleted_at IS NULL AND sec.class_teacher_id IS NULL
         GROUP BY c.level, c.name, sec.id, sec.name ORDER BY c.level, sec.name LIMIT $2",
    ),
];

#[derive(sqlx::FromRow)]
struct FoundRow {
    label: Option<String>,
    detail: Option<String>,
    link: Option<String>,
    total: i64,
}

/// Run the checks this user may see.
pub async fn checks(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
    may: impl Fn(Permission) -> bool,
) -> Result<Vec<HealthCheck>, ApiError> {
    let mut out = Vec::new();
    for (code, severity, area, permission, sql) in CHECKS {
        if !may(*permission) {
            continue;
        }
        let rows: Vec<FoundRow> = sqlx::query_as(*sql)
            .bind(school_id)
            .bind(SAMPLE)
            .fetch_all(&mut *conn)
            .await?;
        out.push(HealthCheck {
            code,
            severity,
            area,
            count: rows.first().map(|r| r.total).unwrap_or(0),
            sample: rows
                .into_iter()
                .map(|r| Found {
                    label: r.label.unwrap_or_default(),
                    detail: r.detail.unwrap_or_default(),
                    link: r.link,
                })
                .collect(),
        });
    }
    Ok(out)
}

async fn data_health(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<HealthCheck>>> {
    if !user.can(Permission::StudentsManage) && !user.can(Permission::SettingsManage) {
        return Err(ApiError::forbidden());
    }
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        checks(&mut c, user.user.school_id, |p| user.can(p)).await?,
    ))
}

/// The whole school as one workbook. For whoever may take a backup, and written to the
/// audit log: it is every record the school has.
async fn export_everything(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Response> {
    user.require(Permission::BackupManage)?;
    let exported = crate::everything::export(&state.pool, user.user.school_id).await?;
    let mut c = state.pool.acquire().await?;
    audit::record(
        &mut c,
        &state.hlc,
        user.actor(),
        Action::Report,
        "export",
        None,
        None,
        Some(serde_json::json!({
            "what": "everything",
            "sheets": exported.sheets.len(),
            "records": exported.sheets.iter().map(|(_, n)| n).sum::<u64>(),
        })),
    )
    .await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let name = format!(
        "madrasa-{}-everything-{}.xlsx",
        school.code.to_lowercase(),
        crate::clock::today()
    );
    Ok(crate::export::file_response(&name, exported.bytes))
}
