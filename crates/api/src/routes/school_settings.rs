//! School-level settings editable by administrators.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::fees::payment_reference_default;
use madrasa_core::permissions::Permission;
use madrasa_core::students::normalise_prefix;
use madrasa_db::{schools, settings, students};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

/// Settings key: `{ "label": "…", "note": "…" }`, absent until the school edits it.
pub const KEY_PAYMENT_REFERENCE: &str = "fees.payment_reference";

#[derive(Debug, Serialize, Deserialize)]
pub struct SchoolSettings {
    pub student_number_prefix: String,
    /// Heading of the payment-reference block on vouchers (v0.2.0-alpha B5). Defaults to
    /// the country's usual wording: "Bank / 1Bill / Kuickpay reference" in Pakistan,
    /// "Payment reference" elsewhere.
    #[serde(default)]
    pub payment_reference_label: String,
    /// One or two sentences under the reference telling the parent how to pay.
    #[serde(default)]
    pub payment_reference_note: String,
}

/// Stored payment-reference wording, or the country's default.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentReferenceText {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub note: String,
}

pub async fn payment_reference_text(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
    country: &str,
) -> ApiResult<PaymentReferenceText> {
    let stored = settings::get(conn, Some(school_id), KEY_PAYMENT_REFERENCE)
        .await?
        .and_then(|v| serde_json::from_value::<PaymentReferenceText>(v).ok())
        .filter(|t| !t.label.trim().is_empty());
    Ok(match stored {
        Some(t) => t,
        None => {
            let (label, note) = payment_reference_default(country);
            PaymentReferenceText {
                label: label.into(),
                note: note.into(),
            }
        }
    })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/settings/school", get(get_settings).put(put_settings))
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<SchoolSettings>> {
    user.require(Permission::SettingsManage)?;
    let mut c = state.pool.acquire().await?;
    let school = schools::get(&mut c, user.user.school_id).await?;
    let reference = payment_reference_text(&mut c, school.id, &school.country).await?;
    Ok(Json(SchoolSettings {
        student_number_prefix: students::number_prefix(&mut c, user.user.school_id).await?,
        payment_reference_label: reference.label,
        payment_reference_note: reference.note,
    }))
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    body: Result<Json<SchoolSettings>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<SchoolSettings>> {
    user.require(Permission::SettingsManage)?;
    let Json(b) = body?;
    let prefix = normalise_prefix(&b.student_number_prefix);
    if prefix.is_empty() || prefix.len() > 12 {
        return Err(ApiError::validation(
            "Student number prefix must be 1–12 letters, digits or hyphens.",
        ));
    }
    let label = b.payment_reference_label.trim().to_string();
    let note = b.payment_reference_note.trim().to_string();
    if label.chars().count() > 80 {
        return Err(ApiError::validation(
            "The payment reference heading must be 80 characters or fewer.",
        ));
    }
    if note.chars().count() > 300 {
        return Err(ApiError::validation(
            "The payment note must be 300 characters or fewer.",
        ));
    }
    let mut c = state.pool.acquire().await?;
    let school = schools::get(&mut c, user.user.school_id).await?;
    settings::set(
        &mut c,
        &state.hlc,
        user.actor(),
        Some(user.user.school_id),
        students::KEY_STUDENT_NUMBER_PREFIX,
        serde_json::json!(prefix),
    )
    .await?;
    // An empty heading means "back to the country's default".
    let reference = if label.is_empty() {
        let (l, n) = payment_reference_default(&school.country);
        PaymentReferenceText {
            label: l.into(),
            note: n.into(),
        }
    } else {
        PaymentReferenceText { label, note }
    };
    settings::set(
        &mut c,
        &state.hlc,
        user.actor(),
        Some(user.user.school_id),
        KEY_PAYMENT_REFERENCE,
        serde_json::to_value(&reference).unwrap_or_default(),
    )
    .await?;
    Ok(Json(SchoolSettings {
        student_number_prefix: prefix,
        payment_reference_label: reference.label,
        payment_reference_note: reference.note,
    }))
}
