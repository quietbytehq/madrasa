//! Fees: setup (heads, structure, policy), concessions, vouchers, payments, ledger, reports.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Datelike, NaiveDate};
use madrasa_core::fees::{FeePolicy, LedgerTotals, PaymentMethod};
use madrasa_core::permissions::Permission;
use madrasa_db::models::{
    CollectionByMethod, Defaulter, FeeConcession, FeeHead, FeeReceipt, FeeStructure, FeeVoucher,
    FeeVoucherLine, FeeVoucherRow, School,
};
use madrasa_db::{academics, fees, guardians, schools, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::clock::today;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/fees/policy", get(get_policy).put(put_policy))
        .route("/fees/heads", get(list_heads).post(create_head))
        .route("/fees/heads/{id}", axum::routing::patch(update_head))
        .route("/fees/structure", get(get_structure).put(put_structure))
        .route(
            "/fees/students/{id}/concessions",
            get(list_concessions).post(create_concession),
        )
        .route(
            "/fees/concessions/{id}",
            axum::routing::delete(end_concession),
        )
        .route("/fees/vouchers", get(list_vouchers))
        .route("/fees/vouchers/generate", post(generate))
        .route("/fees/vouchers/{id}", get(get_voucher))
        .route("/fees/vouchers/{id}/payments", post(pay))
        .route("/fees/vouchers/{id}/waive-late-fee", post(waive))
        .route("/fees/vouchers/{id}/cancel", post(cancel))
        .route("/fees/late-fees/run", post(run_late_fees))
        .route("/fees/receipts/{id}", get(get_receipt))
        .route("/fees/receipts/{id}/reverse", post(reverse))
        .route("/fees/students/{id}/ledger", get(ledger))
        .route("/fees/defaulters", get(defaulters))
        .route("/fees/collections", get(collections))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

fn first_of_month(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d)
}

// ---- policy & setup -----------------------------------------------------------------------

async fn get_policy(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<FeePolicy>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(fees::policy(&mut c, user.user.school_id).await?))
}

async fn put_policy(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<FeePolicy>,
) -> ApiResult<Json<FeePolicy>> {
    user.require(Permission::FeesApprove)?;
    let Json(p) = body?;
    p.validate()
        .map_err(|e| ApiError::validation(format!("{e}.")))?;
    let mut c = state.pool.acquire().await?;
    fees::set_policy(&mut c, &state.hlc, user.actor(), user.user.school_id, &p).await?;
    Ok(Json(p))
}

#[derive(Debug, Deserialize)]
pub struct HeadBody {
    pub name: Option<String>,
    pub code: Option<String>,
    pub is_recurring: Option<bool>,
    pub position: Option<i32>,
}

async fn list_heads(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<FeeHead>>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(fees::list_heads(&mut c, user.user.school_id).await?))
}

async fn create_head(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<HeadBody>,
) -> ApiResult<Json<FeeHead>> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    let name = b
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Fee head name is required."))?;
    let code = b
        .code
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Fee head code is required."))?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        fees::create_head(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            name,
            code,
            b.is_recurring.unwrap_or(true),
            b.position.unwrap_or(0),
        )
        .await?,
    ))
}

async fn update_head(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<HeadBody>,
) -> ApiResult<Json<FeeHead>> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    let mut c = state.pool.acquire().await?;
    if fees::get_head(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("fee head"));
    }
    Ok(Json(
        fees::update_head(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            b.name.as_deref(),
            b.is_recurring,
            b.position,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct StructureQuery {
    pub academic_year_id: Uuid,
    pub class_id: Option<Uuid>,
}

async fn get_structure(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StructureQuery>,
) -> ApiResult<Json<Vec<FeeStructure>>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    if academics::get_year(&mut c, q.academic_year_id)
        .await?
        .school_id
        != user.user.school_id
    {
        return Err(ApiError::not_found("academic year"));
    }
    Ok(Json(
        fees::list_structure(&mut c, q.academic_year_id, q.class_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct StructureLine {
    pub fee_head_id: Uuid,
    pub amount: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct StructureBody {
    pub academic_year_id: Uuid,
    pub class_id: Uuid,
    pub lines: Vec<StructureLine>,
}

async fn put_structure(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<StructureBody>,
) -> ApiResult<Json<Vec<FeeStructure>>> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    let sid = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    if academics::get_year(&mut tx, b.academic_year_id)
        .await?
        .school_id
        != sid
        || academics::get_class(&mut tx, b.class_id).await?.school_id != sid
    {
        return Err(ApiError::not_found("academic year or class"));
    }
    for l in &b.lines {
        if l.amount < Decimal::ZERO {
            return Err(ApiError::validation("Amounts cannot be negative."));
        }
        if fees::get_head(&mut tx, l.fee_head_id).await?.school_id != sid {
            return Err(ApiError::not_found("fee head"));
        }
        fees::set_structure(
            &mut tx,
            &state.hlc,
            user.actor(),
            sid,
            b.academic_year_id,
            b.class_id,
            l.fee_head_id,
            l.amount,
        )
        .await?;
    }
    let rows = fees::list_structure(&mut tx, b.academic_year_id, Some(b.class_id)).await?;
    tx.commit().await?;
    Ok(Json(rows))
}

// ---- concessions --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConcessionBody {
    pub kind: String,
    pub percent: Option<Decimal>,
    pub amount: Option<Decimal>,
    pub reason: String,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
}

async fn check_student(c: &mut sqlx::PgConnection, school_id: Uuid, id: Uuid) -> ApiResult<()> {
    if students::get(c, id).await?.school_id != school_id {
        return Err(ApiError::not_found("student"));
    }
    Ok(())
}

async fn list_concessions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FeeConcession>>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    check_student(&mut c, user.user.school_id, id).await?;
    Ok(Json(fees::concessions_for(&mut c, id).await?))
}

async fn create_concession(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ConcessionBody>,
) -> ApiResult<(StatusCode, Json<FeeConcession>)> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    if !matches!(b.kind.as_str(), "staff_child" | "scholarship" | "custom") {
        return Err(ApiError::validation(
            "Concession kind must be staff_child, scholarship or custom.",
        ));
    }
    if b.reason.trim().is_empty() {
        return Err(ApiError::validation(
            "A reason is required for a concession.",
        ));
    }
    if b.percent.is_none() && b.amount.is_none() {
        return Err(ApiError::validation("Give a percentage or a fixed amount."));
    }
    if b.percent
        .is_some_and(|p| p < Decimal::ZERO || p > Decimal::ONE_HUNDRED)
        || b.amount.is_some_and(|a| a < Decimal::ZERO)
    {
        return Err(ApiError::validation(
            "Percentage must be 0–100 and amounts cannot be negative.",
        ));
    }
    let mut c = state.pool.acquire().await?;
    check_student(&mut c, user.user.school_id, id).await?;
    let row = fees::create_concession(
        &mut c,
        &state.hlc,
        user.actor(),
        fees::NewConcession {
            school_id: user.user.school_id,
            student_id: id,
            kind: b.kind,
            percent: b.percent,
            amount: b.amount,
            reason: b.reason,
            approved_by: user.user.id,
            valid_from: b.valid_from.unwrap_or_else(|| first_of_month(today())),
            valid_to: b.valid_to,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn end_concession(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::FeesApprove)?;
    let mut c = state.pool.acquire().await?;
    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT school_id FROM fee_concessions WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *c)
    .await?;
    if owner != Some(user.user.school_id) {
        return Err(ApiError::not_found("concession"));
    }
    fees::end_concession(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- vouchers -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VoucherQuery {
    /// Any date in the billing month (YYYY-MM-01 preferred).
    pub month: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    pub student_id: Option<Uuid>,
    pub q: Option<String>,
    pub limit: Option<i64>,
}

async fn list_vouchers(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<VoucherQuery>,
) -> ApiResult<Json<Vec<FeeVoucherRow>>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    let f = fees::VoucherFilter {
        billing_month: q.month.map(first_of_month),
        class_id: q.class_id,
        section_id: q.section_id,
        status: q.status,
        student_id: q.student_id,
        q: q.q,
    };
    Ok(Json(
        fees::list_vouchers(
            &mut c,
            user.user.school_id,
            &f,
            q.limit.unwrap_or(500).clamp(1, 5000),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct GenerateBody {
    pub month: NaiveDate,
    pub academic_year_id: Option<Uuid>,
    /// One class, or omit for the whole school.
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub issue_date: Option<NaiveDate>,
}

async fn generate(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<GenerateBody>,
) -> ApiResult<Json<fees::GenerationReport>> {
    user.require(Permission::FeesManage)?;
    let Json(b) = body?;
    let sid = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let year = match b.academic_year_id {
        Some(id) => academics::get_year(&mut tx, id).await?,
        None => academics::active_year(&mut tx, sid).await?.ok_or_else(|| {
            ApiError::validation("No active academic year. Set one in Settings first.")
        })?,
    };
    if year.school_id != sid {
        return Err(ApiError::not_found("academic year"));
    }
    let month = first_of_month(b.month);
    if month < first_of_month(year.start_date) || month > year.end_date {
        return Err(ApiError::validation(
            "The billing month is outside the academic year.",
        ));
    }
    let sections: Option<Vec<Uuid>> = match (b.section_id, b.class_id) {
        (Some(s), _) => {
            if academics::get_section(&mut tx, s).await?.school_id != sid {
                return Err(ApiError::not_found("section"));
            }
            Some(vec![s])
        }
        (None, Some(cid)) => Some(fees::class_section_ids(&mut tx, sid, year.id, cid).await?),
        (None, None) => None,
    };
    let report = fees::generate_vouchers(
        &mut tx,
        &state.hlc,
        user.actor(),
        sid,
        year.id,
        month,
        sections.as_deref(),
        b.issue_date.unwrap_or_else(today),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(report))
}

#[derive(Debug, Deserialize, Default)]
pub struct LateFeeRunBody {
    /// Date to check the vouchers against; today when omitted. Never in the future.
    pub as_of: Option<NaiveDate>,
}

/// Run the automatic late fee pass now (the daily job does this on its own; here for an
/// operator who wants the figures current at once, and for tests).
async fn run_late_fees(
    State(state): State<AppState>,
    user: CurrentUser,
    body: Option<Json<LateFeeRunBody>>,
) -> ApiResult<Json<fees::LateFeeReport>> {
    user.require(Permission::FeesManage)?;
    let as_of = body.and_then(|Json(b)| b.as_of).unwrap_or_else(today);
    if as_of > today() {
        return Err(ApiError::validation(
            "Late fees cannot be applied for a date in the future.",
        ));
    }
    let report = crate::late_fees::run(&state, user.actor(), user.user.school_id, as_of).await?;
    Ok(Json(report))
}

/// Voucher with everything needed to print it.
#[derive(Debug, Serialize)]
pub struct VoucherDetail {
    #[serde(flatten)]
    pub voucher: FeeVoucher,
    pub balance: Decimal,
    pub lines: Vec<FeeVoucherLine>,
    pub receipts: Vec<FeeReceipt>,
    pub student_name: String,
    pub student_number: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub guardian_phone: Option<String>,
    pub school: SchoolHeader,
    pub policy: FeePolicy,
}

#[derive(Debug, Serialize)]
pub struct SchoolHeader {
    pub name: String,
    pub code: String,
    pub city: String,
    /// ISO 4217 code printed next to amounts.
    pub currency: String,
}

impl From<School> for SchoolHeader {
    fn from(s: School) -> Self {
        Self {
            name: s.name,
            code: s.code,
            city: s.city,
            currency: s.currency,
        }
    }
}

async fn voucher_detail(c: &mut sqlx::PgConnection, v: FeeVoucher) -> ApiResult<VoucherDetail> {
    let student = students::get(c, v.student_id).await?;
    let (class_name, section_name) = match v.section_id {
        Some(sid) => {
            let section = academics::get_section(c, sid).await?;
            let class = academics::get_class(c, section.class_id).await?;
            (Some(class.name), Some(section.name))
        }
        None => (None, None),
    };
    let guardian_phone = guardians::for_student(c, student.id)
        .await?
        .into_iter()
        .find_map(|g| g.phone);
    let school = schools::get(c, v.school_id).await?;
    let policy = fees::policy(c, v.school_id).await?;
    Ok(VoucherDetail {
        balance: v.balance(),
        lines: fees::voucher_lines(c, v.id).await?,
        receipts: fees::receipts_for_voucher(c, v.id).await?,
        student_name: student.full_name,
        student_number: student.student_number,
        father_name: student.father_name,
        class_name,
        section_name,
        guardian_phone,
        school: school.into(),
        policy,
        voucher: v,
    })
}

async fn load_voucher(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<FeeVoucher> {
    let v = fees::get_voucher(c, id).await?;
    if v.school_id != user.user.school_id {
        return Err(ApiError::not_found("voucher"));
    }
    Ok(v)
}

async fn get_voucher(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<VoucherDetail>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    let v = load_voucher(&mut c, &user, id).await?;
    Ok(Json(voucher_detail(&mut c, v).await?))
}

#[derive(Debug, Deserialize)]
pub struct PaymentBody {
    pub amount: Decimal,
    pub method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub paid_on: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    pub receipt: FeeReceipt,
    pub voucher: VoucherDetail,
}

async fn pay(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<PaymentBody>,
) -> ApiResult<(StatusCode, Json<PaymentResponse>)> {
    user.require(Permission::FeesManage)?;
    let Json(b) = body?;
    let method: PaymentMethod = b.method.parse().map_err(|_| {
        ApiError::validation("Payment method must be cash, bank, online, one_bill or kuickpay.")
    })?;
    if method.needs_reference()
        && b.reference
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(ApiError::validation(
            "A reference number is required for this payment method.",
        ));
    }
    if b.amount <= Decimal::ZERO {
        return Err(ApiError::validation("Amount must be greater than zero."));
    }
    let paid_on = b.paid_on.unwrap_or_else(today);
    if paid_on > today() {
        return Err(ApiError::validation(
            "Payment date cannot be in the future.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    load_voucher(&mut tx, &user, id).await?;
    let policy = fees::policy(&mut tx, user.user.school_id).await?;
    let (receipt, voucher) = fees::record_payment(
        &mut tx,
        &state.hlc,
        user.actor(),
        &policy,
        fees::NewPayment {
            voucher_id: id,
            amount: b.amount,
            method,
            reference: b.reference,
            note: b.note,
            paid_on,
            received_by: user.user.id,
        },
    )
    .await?;
    let detail = voucher_detail(&mut tx, voucher).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(PaymentResponse {
            receipt,
            voucher: detail,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ReasonBody {
    pub reason: String,
}

async fn waive(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ReasonBody>,
) -> ApiResult<Json<VoucherDetail>> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    if b.reason.trim().is_empty() {
        return Err(ApiError::validation(
            "A reason is required to waive a late fee.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let v = load_voucher(&mut tx, &user, id).await?;
    if v.status == "cancelled" || v.status == "carried" {
        return Err(ApiError::conflict(
            "Only open vouchers can have their late fee waived.",
        ));
    }
    let v = fees::waive_late_fee(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        &b.reason,
        user.user.id,
    )
    .await?;
    let detail = voucher_detail(&mut tx, v).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

async fn cancel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ReasonBody>,
) -> ApiResult<Json<VoucherDetail>> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    if b.reason.trim().is_empty() {
        return Err(ApiError::validation(
            "A reason is required to cancel a voucher.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    load_voucher(&mut tx, &user, id).await?;
    let v = fees::cancel_voucher(&mut tx, &state.hlc, user.actor(), id, &b.reason).await?;
    let detail = voucher_detail(&mut tx, v).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

// ---- receipts -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ReceiptDetail {
    #[serde(flatten)]
    pub receipt: FeeReceipt,
    pub voucher_no: String,
    pub billing_month: NaiveDate,
    pub student_name: String,
    pub student_number: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub received_by_name: String,
    pub reversed_by: Option<FeeReceipt>,
    pub voucher_balance_after: Decimal,
    pub school: SchoolHeader,
}

async fn receipt_detail(c: &mut sqlx::PgConnection, r: FeeReceipt) -> ApiResult<ReceiptDetail> {
    let v = fees::get_voucher(c, r.voucher_id).await?;
    let student = students::get(c, r.student_id).await?;
    let (class_name, section_name) = match v.section_id {
        Some(sid) => {
            let section = academics::get_section(c, sid).await?;
            (
                Some(academics::get_class(c, section.class_id).await?.name),
                Some(section.name),
            )
        }
        None => (None, None),
    };
    let received_by_name = madrasa_db::users::get(c, r.received_by)
        .await
        .map(|u| u.full_name)
        .unwrap_or_default();
    let reversed_by =
        sqlx::query_as::<_, FeeReceipt>("SELECT * FROM fee_receipts WHERE reverses_id = $1")
            .bind(r.id)
            .fetch_optional(&mut *c)
            .await?;
    let school = schools::get(c, r.school_id).await?;
    Ok(ReceiptDetail {
        voucher_no: v.voucher_no.clone(),
        billing_month: v.billing_month,
        student_name: student.full_name,
        student_number: student.student_number,
        father_name: student.father_name,
        class_name,
        section_name,
        received_by_name,
        reversed_by,
        voucher_balance_after: v.balance(),
        school: school.into(),
        receipt: r,
    })
}

async fn get_receipt(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReceiptDetail>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    let r = fees::get_receipt(&mut c, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("receipt"));
    }
    Ok(Json(receipt_detail(&mut c, r).await?))
}

async fn reverse(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ReasonBody>,
) -> ApiResult<(StatusCode, Json<ReceiptDetail>)> {
    user.require(Permission::FeesApprove)?;
    let Json(b) = body?;
    if b.reason.trim().is_empty() {
        return Err(ApiError::validation(
            "A reason is required to reverse a receipt.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let r = fees::get_receipt(&mut tx, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("receipt"));
    }
    let (reversal, _) = fees::reverse_receipt(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        &b.reason,
        user.user.id,
        today(),
    )
    .await?;
    let detail = receipt_detail(&mut tx, reversal).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

// ---- ledger & reports ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LedgerVoucher {
    #[serde(flatten)]
    pub voucher: FeeVoucher,
    pub balance: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Ledger {
    pub student_id: Uuid,
    pub totals: LedgerTotals,
    pub vouchers: Vec<LedgerVoucher>,
    pub receipts: Vec<FeeReceipt>,
    pub concessions: Vec<FeeConcession>,
    /// Open vouchers the student can pay (latest first).
    pub payable: Vec<Uuid>,
}

async fn ledger(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Ledger>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    check_student(&mut c, user.user.school_id, id).await?;
    let vouchers = fees::vouchers_for_student(&mut c, id).await?;
    let payable = vouchers
        .iter()
        .filter(|v| v.status == "unpaid" || v.status == "partial")
        .map(|v| v.id)
        .collect();
    Ok(Json(Ledger {
        student_id: id,
        totals: fees::ledger_totals(&mut c, id).await?,
        vouchers: vouchers
            .into_iter()
            .map(|v| LedgerVoucher {
                balance: v.balance(),
                voucher: v,
            })
            .collect(),
        receipts: fees::receipts_for_student(&mut c, id).await?,
        concessions: fees::concessions_for(&mut c, id).await?,
        payable,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DefaultersQuery {
    pub as_of: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
    pub min_days: Option<i32>,
}

async fn defaulters(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DefaultersQuery>,
) -> ApiResult<Json<Vec<Defaulter>>> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        fees::defaulters(
            &mut c,
            user.user.school_id,
            q.as_of.unwrap_or_else(today),
            q.class_id,
            q.min_days.unwrap_or(0).max(0),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct CollectionsQuery {
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct Collections {
    pub date: NaiveDate,
    pub total: Decimal,
    pub by_method: Vec<CollectionByMethod>,
    pub receipts: Vec<ReceiptDetail>,
}

async fn collections(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<CollectionsQuery>,
) -> ApiResult<Json<Collections>> {
    user.require(Permission::FeesRead)?;
    let date = q.date.unwrap_or_else(today);
    let mut c = state.pool.acquire().await?;
    let by_method = fees::collection_summary(&mut c, user.user.school_id, date).await?;
    let total = by_method.iter().map(|m| m.amount).sum();
    let mut receipts = Vec::new();
    for r in fees::receipts_on(&mut c, user.user.school_id, date).await? {
        receipts.push(receipt_detail(&mut c, r).await?);
    }
    Ok(Json(Collections {
        date,
        total,
        by_method,
        receipts,
    }))
}
