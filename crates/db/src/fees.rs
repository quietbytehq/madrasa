//! Fees: heads, structures, concessions, vouchers, receipts, ledger, defaulters, collections.
//! Every money change is audited; receipts are immutable (reversal rows only).

use crate::audit::{self, snapshot, Action};
use crate::models::{
    CollectionByMethod, Defaulter, FeeConcession, FeeHead, FeeReceipt, FeeStructure, FeeVoucher,
    FeeVoucherLine, FeeVoucherRow,
};
use crate::{academics, map_unique, settings, students, Actor, DbError, DbResult};
use chrono::{Datelike, NaiveDate};
use madrasa_core::fees::{
    self, ConcessionTerms, FeePolicy, PaymentMethod, VoucherInput, VoucherStatus,
};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

pub const KEY_POLICY: &str = "fees.policy";

// ---- policy -------------------------------------------------------------------------------

pub async fn policy(conn: &mut PgConnection, school_id: Uuid) -> DbResult<FeePolicy> {
    Ok(settings::get(conn, Some(school_id), KEY_POLICY)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub async fn set_policy(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    policy: &FeePolicy,
) -> DbResult<()> {
    let value =
        serde_json::to_value(policy).map_err(|_| DbError::Conflict("invalid policy".into()))?;
    settings::set(conn, hlc, actor, Some(school_id), KEY_POLICY, value).await?;
    Ok(())
}

/// A voucher that carries an opening balance brought over from the old register: one line
/// under the "Opening balance" head (created on first use), due on issue. Later months carry
/// it forward like any other unpaid balance.
#[allow(clippy::too_many_arguments)]
pub async fn create_opening_balance(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    student_id: Uuid,
    academic_year_id: Uuid,
    billing_month: NaiveDate,
    amount: Decimal,
    note: Option<&str>,
) -> DbResult<FeeVoucher> {
    if amount <= Decimal::ZERO || fees::money(amount) != amount {
        return Err(DbError::Conflict(
            "opening balance must be a positive amount with at most two decimals".into(),
        ));
    }
    let head = match list_heads(&mut *conn, school_id)
        .await?
        .into_iter()
        .find(|h| h.code.eq_ignore_ascii_case("OPB"))
    {
        Some(h) => h,
        None => {
            create_head(
                &mut *conn,
                hlc,
                actor,
                school_id,
                "Opening balance",
                "OPB",
                false,
                99,
            )
            .await?
        }
    };
    let section_id = students::section_in_year(&mut *conn, student_id, academic_year_id).await?;
    let seq = students::next_sequence(
        &mut *conn,
        hlc,
        school_id,
        &format!("voucher_no:{}", billing_month.year()),
    )
    .await?;
    let voucher_no = format!(
        "V{}{:02}-{:05}",
        billing_month.year(),
        billing_month.month(),
        seq
    );
    let id = new_id();
    let voucher = sqlx::query_as::<_, FeeVoucher>(
        "INSERT INTO fee_vouchers (id, school_id, student_id, academic_year_id, section_id, voucher_no, billing_month, issue_date, due_date,
            gross, sibling_discount, concession_discount, net_monthly, arrears, total_due, status, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7, $8, 0, 0, $8, 0, $8, 'unpaid', $9, $10) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(student_id)
    .bind(academic_year_id)
    .bind(section_id)
    .bind(&voucher_no)
    .bind(billing_month)
    .bind(amount)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO fee_voucher_lines (id, school_id, voucher_id, fee_head_id, description, amount, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8)",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(id)
    .bind(head.id)
    .bind(match note {
        Some(n) if !n.trim().is_empty() => format!("Opening balance: {}", n.trim()),
        _ => "Opening balance".to_string(),
    })
    .bind(amount)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "fee_voucher",
        Some(voucher.id),
        None,
        Some(serde_json::json!({ "opening_balance": amount, "note": note, "voucher_no": voucher_no })),
    )
    .await?;
    Ok(voucher)
}

// ---- heads --------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn create_head(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    name: &str,
    code: &str,
    is_recurring: bool,
    position: i32,
) -> DbResult<FeeHead> {
    let id = new_id();
    let row = sqlx::query_as::<_, FeeHead>(
        "INSERT INTO fee_heads (id, school_id, name, code, is_recurring, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(name.trim())
    .bind(code.trim().to_uppercase())
    .bind(is_recurring)
    .bind(position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a fee head with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "fee_head",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_head(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: Option<&str>,
    is_recurring: Option<bool>,
    position: Option<i32>,
) -> DbResult<FeeHead> {
    let before = get_head(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, FeeHead>(
        "UPDATE fee_heads SET name = $2, is_recurring = $3, position = $4, updated_at = now(), updated_by = $5, hlc = $6 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name.map(str::trim).unwrap_or(&before.name))
    .bind(is_recurring.unwrap_or(before.is_recurring))
    .bind(position.unwrap_or(before.position))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "fee_head",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn get_head(conn: &mut PgConnection, id: Uuid) -> DbResult<FeeHead> {
    sqlx::query_as::<_, FeeHead>("SELECT * FROM fee_heads WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("fee head".into()))
}

pub async fn list_heads(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<FeeHead>> {
    Ok(sqlx::query_as::<_, FeeHead>("SELECT * FROM fee_heads WHERE school_id = $1 AND deleted_at IS NULL ORDER BY position, name")
        .bind(school_id)
        .fetch_all(conn)
        .await?)
}

// ---- structures ---------------------------------------------------------------------------

pub async fn list_structure(
    conn: &mut PgConnection,
    academic_year_id: Uuid,
    class_id: Option<Uuid>,
) -> DbResult<Vec<FeeStructure>> {
    Ok(sqlx::query_as::<_, FeeStructure>(
        "SELECT id, school_id, academic_year_id, class_id, fee_head_id, amount, hlc FROM fee_structures
         WHERE academic_year_id = $1 AND ($2::uuid IS NULL OR class_id = $2) AND deleted_at IS NULL",
    )
    .bind(academic_year_id)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

/// Set the amount of one head for a class/year (upsert; zero removes the line). Audited.
#[allow(clippy::too_many_arguments)]
pub async fn set_structure(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    academic_year_id: Uuid,
    class_id: Uuid,
    fee_head_id: Uuid,
    amount: Decimal,
) -> DbResult<Option<FeeStructure>> {
    let amount = fees::money(amount);
    let before = sqlx::query_as::<_, FeeStructure>(
        "SELECT id, school_id, academic_year_id, class_id, fee_head_id, amount, hlc FROM fee_structures
         WHERE academic_year_id = $1 AND class_id = $2 AND fee_head_id = $3 AND deleted_at IS NULL",
    )
    .bind(academic_year_id)
    .bind(class_id)
    .bind(fee_head_id)
    .fetch_optional(&mut *conn)
    .await?;
    let stamp = hlc.now().to_string();
    let after = if amount.is_zero() {
        sqlx::query("UPDATE fee_structures SET deleted_at = now(), updated_at = now(), updated_by = $4, hlc = $5 WHERE academic_year_id = $1 AND class_id = $2 AND fee_head_id = $3 AND deleted_at IS NULL")
            .bind(academic_year_id)
            .bind(class_id)
            .bind(fee_head_id)
            .bind(actor.user_id)
            .bind(&stamp)
            .execute(&mut *conn)
            .await?;
        None
    } else {
        Some(
            sqlx::query_as::<_, FeeStructure>(
                "INSERT INTO fee_structures (id, school_id, academic_year_id, class_id, fee_head_id, amount, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (academic_year_id, class_id, fee_head_id) DO UPDATE
                   SET amount = EXCLUDED.amount, deleted_at = NULL, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
                 RETURNING id, school_id, academic_year_id, class_id, fee_head_id, amount, hlc",
            )
            .bind(new_id())
            .bind(school_id)
            .bind(academic_year_id)
            .bind(class_id)
            .bind(fee_head_id)
            .bind(amount)
            .bind(actor.user_id)
            .bind(&stamp)
            .fetch_one(&mut *conn)
            .await?,
        )
    };
    let entity_id = after
        .as_ref()
        .map(|a| a.id)
        .or(before.as_ref().map(|b| b.id));
    let action = match (&before, &after) {
        (None, Some(_)) => Action::Create,
        (Some(_), None) => Action::Delete,
        _ => Action::Update,
    };
    audit::record(
        conn,
        hlc,
        actor,
        action,
        "fee_structure",
        entity_id,
        before.as_ref().map(snapshot),
        after.as_ref().map(snapshot),
    )
    .await?;
    Ok(after)
}

// ---- concessions --------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewConcession {
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub kind: String,
    pub percent: Option<Decimal>,
    pub amount: Option<Decimal>,
    pub reason: String,
    pub approved_by: Uuid,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
}

pub async fn create_concession(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewConcession,
) -> DbResult<FeeConcession> {
    let id = new_id();
    let row = sqlx::query_as::<_, FeeConcession>(
        "INSERT INTO fee_concessions (id, school_id, student_id, kind, percent, amount, reason, approved_by, valid_from, valid_to, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.student_id)
    .bind(input.kind)
    .bind(input.percent)
    .bind(input.amount.map(fees::money))
    .bind(input.reason.trim())
    .bind(input.approved_by)
    .bind(input.valid_from)
    .bind(input.valid_to)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "fee_concession",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// End a concession (soft delete), audited.
pub async fn end_concession(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let before = sqlx::query_as::<_, FeeConcession>(
        "SELECT * FROM fee_concessions WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("concession".into()))?;
    sqlx::query("UPDATE fee_concessions SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "fee_concession",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

pub async fn concessions_for(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<Vec<FeeConcession>> {
    Ok(sqlx::query_as::<_, FeeConcession>("SELECT * FROM fee_concessions WHERE student_id = $1 AND deleted_at IS NULL ORDER BY valid_from DESC")
        .bind(student_id)
        .fetch_all(conn)
        .await?)
}

/// Concessions in force on `on`.
pub async fn active_concessions(
    conn: &mut PgConnection,
    student_id: Uuid,
    on: NaiveDate,
) -> DbResult<Vec<FeeConcession>> {
    Ok(sqlx::query_as::<_, FeeConcession>(
        "SELECT * FROM fee_concessions WHERE student_id = $1 AND deleted_at IS NULL AND valid_from <= $2 AND (valid_to IS NULL OR valid_to >= $2)",
    )
    .bind(student_id)
    .bind(on)
    .fetch_all(conn)
    .await?)
}

// ---- vouchers -----------------------------------------------------------------------------

pub async fn get_voucher(conn: &mut PgConnection, id: Uuid) -> DbResult<FeeVoucher> {
    sqlx::query_as::<_, FeeVoucher>(
        "SELECT * FROM fee_vouchers WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("voucher".into()))
}

pub async fn voucher_lines(
    conn: &mut PgConnection,
    voucher_id: Uuid,
) -> DbResult<Vec<FeeVoucherLine>> {
    Ok(sqlx::query_as::<_, FeeVoucherLine>(
        "SELECT id, voucher_id, fee_head_id, description, amount, position FROM fee_voucher_lines WHERE voucher_id = $1 AND deleted_at IS NULL ORDER BY position",
    )
    .bind(voucher_id)
    .fetch_all(conn)
    .await?)
}

/// Vouchers of a student, newest first.
pub async fn vouchers_for_student(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<Vec<FeeVoucher>> {
    Ok(sqlx::query_as::<_, FeeVoucher>(
        "SELECT * FROM fee_vouchers WHERE student_id = $1 AND deleted_at IS NULL ORDER BY billing_month DESC",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// Sibling rank of a student within their family: `(sibling_count, rank)` where rank 0 is the
/// earliest-admitted (then eldest) child. Siblings are active students sharing a guardian.
pub async fn sibling_rank(conn: &mut PgConnection, student_id: Uuid) -> DbResult<(usize, usize)> {
    let family: Vec<(Uuid, NaiveDate, NaiveDate)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.admission_date, s.date_of_birth FROM students s
         WHERE s.deleted_at IS NULL AND s.status = 'active' AND (s.id = $1 OR s.id IN (
            SELECT b.student_id FROM student_guardians a
            JOIN student_guardians b ON b.guardian_id = a.guardian_id AND b.deleted_at IS NULL
            WHERE a.student_id = $1 AND a.deleted_at IS NULL))
         ORDER BY s.admission_date, s.date_of_birth, s.id",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?;
    let rank = family
        .iter()
        .position(|(id, _, _)| *id == student_id)
        .unwrap_or(0);
    Ok((family.len().saturating_sub(1), rank))
}

/// Carry forward: assess late fees on the student's open vouchers before `billing_month`,
/// mark them `carried` (audited) and return the total balance to put on the new voucher.
/// Credits (overpayments) carry as negative arrears.
pub async fn outstanding_before(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    policy: &FeePolicy,
    student_id: Uuid,
    billing_month: NaiveDate,
    as_of: NaiveDate,
) -> DbResult<Decimal> {
    let earlier = sqlx::query_as::<_, FeeVoucher>(
        "SELECT * FROM fee_vouchers WHERE student_id = $1 AND billing_month < $2 AND status IN ('unpaid', 'partial') AND deleted_at IS NULL ORDER BY billing_month",
    )
    .bind(student_id)
    .bind(billing_month)
    .fetch_all(&mut *conn)
    .await?;
    let mut balances = Vec::with_capacity(earlier.len());
    for v in earlier {
        let v = assess_late_fee(&mut *conn, hlc, actor, policy, v, as_of).await?;
        let balance = v.balance();
        if balance.is_zero() {
            continue;
        }
        let after = sqlx::query_as::<_, FeeVoucher>(
            "UPDATE fee_vouchers SET status = 'carried', updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 RETURNING *",
        )
        .bind(v.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "fee_voucher",
            Some(v.id),
            Some(snapshot(&v)),
            Some(snapshot(&after)),
        )
        .await?;
        balances.push(balance);
    }
    Ok(fees::carry_forward(&balances))
}

/// Assess the late fee on a voucher that is unpaid after its due date (idempotent, audited).
pub async fn assess_late_fee(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    policy: &FeePolicy,
    v: FeeVoucher,
    as_of: NaiveDate,
) -> DbResult<FeeVoucher> {
    if v.status == "paid"
        || v.status == "cancelled"
        || v.late_fee_waived
        || !v.late_fee.is_zero()
        || v.balance() <= Decimal::ZERO
    {
        return Ok(v);
    }
    let fee = fees::late_fee(policy, v.net_monthly, v.due_date, as_of, false);
    if fee.is_zero() {
        return Ok(v);
    }
    let after = sqlx::query_as::<_, FeeVoucher>(
        "UPDATE fee_vouchers SET late_fee = $2, status = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1 RETURNING *",
    )
    .bind(v.id)
    .bind(fee)
    .bind(VoucherStatus::derive(fees::voucher_balance(v.total_due, fee, v.net_paid), v.net_paid).as_str())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "fee_voucher",
        Some(v.id),
        Some(snapshot(&v)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct GenerationReport {
    pub created: usize,
    pub skipped_existing: usize,
    pub skipped_no_structure: usize,
    pub voucher_ids: Vec<Uuid>,
}

/// Generate the month's vouchers for the active students of a section set (or the whole school
/// when `section_ids` is `None`). One voucher per student per month; existing ones are skipped.
#[allow(clippy::too_many_arguments)]
pub async fn generate_vouchers(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    academic_year_id: Uuid,
    billing_month: NaiveDate,
    section_ids: Option<&[Uuid]>,
    issue_date: NaiveDate,
) -> DbResult<GenerationReport> {
    let billing_month = NaiveDate::from_ymd_opt(billing_month.year(), billing_month.month(), 1)
        .unwrap_or(billing_month);
    let policy = policy(&mut *conn, school_id).await?;
    let heads = list_heads(&mut *conn, school_id).await?;
    let structure = list_structure(&mut *conn, academic_year_id, None).await?;
    let enrolled: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT e.student_id, e.section_id, sec.class_id FROM enrollments e
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         JOIN sections sec ON sec.id = e.section_id
         WHERE e.academic_year_id = $1 AND e.deleted_at IS NULL AND ($2::uuid[] IS NULL OR e.section_id = ANY($2))
         ORDER BY s.student_number",
    )
    .bind(academic_year_id)
    .bind(section_ids)
    .fetch_all(&mut *conn)
    .await?;
    let due_date = policy.due_date(billing_month);
    let mut report = GenerationReport::default();
    for (student_id, section_id, class_id) in enrolled {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM fee_vouchers WHERE student_id = $1 AND billing_month = $2 AND status <> 'cancelled' AND deleted_at IS NULL)")
            .bind(student_id)
            .bind(billing_month)
            .fetch_one(&mut *conn)
            .await?;
        if exists {
            report.skipped_existing += 1;
            continue;
        }
        let first_voucher: bool = !sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM fee_vouchers WHERE student_id = $1 AND deleted_at IS NULL)")
            .bind(student_id)
            .fetch_one(&mut *conn)
            .await?;
        let mut lines: Vec<(FeeHead, Decimal)> = Vec::new();
        for st in structure.iter().filter(|s| s.class_id == class_id) {
            if let Some(head) = heads.iter().find(|h| h.id == st.fee_head_id) {
                if head.is_recurring || first_voucher {
                    lines.push((head.clone(), st.amount));
                }
            }
        }
        if lines.is_empty() {
            report.skipped_no_structure += 1;
            continue;
        }
        lines.sort_by_key(|(h, _)| (h.position, h.name.clone()));
        let (sibling_count, rank) = sibling_rank(&mut *conn, student_id).await?;
        let concessions = active_concessions(&mut *conn, student_id, billing_month).await?;
        let arrears = outstanding_before(
            &mut *conn,
            hlc,
            actor,
            &policy,
            student_id,
            billing_month,
            issue_date,
        )
        .await?;
        let totals = fees::compute_voucher(
            &policy,
            &VoucherInput {
                lines: lines.iter().map(|(h, a)| (h.name.clone(), *a)).collect(),
                sibling_discount: policy.sibling_discount_applies(sibling_count, rank),
                concessions: concessions
                    .iter()
                    .map(|c| ConcessionTerms {
                        percent: c.percent,
                        amount: c.amount,
                    })
                    .collect(),
                arrears,
            },
        );
        let seq = students::next_sequence(
            &mut *conn,
            hlc,
            school_id,
            &format!("voucher_no:{}", billing_month.year()),
        )
        .await?;
        let voucher_no = format!(
            "V{}{:02}-{:05}",
            billing_month.year(),
            billing_month.month(),
            seq
        );
        let id = new_id();
        let status = VoucherStatus::derive(totals.total_due, Decimal::ZERO).as_str();
        let voucher = sqlx::query_as::<_, FeeVoucher>(
            "INSERT INTO fee_vouchers (id, school_id, student_id, academic_year_id, section_id, voucher_no, billing_month, issue_date, due_date,
                gross, sibling_discount, concession_discount, net_monthly, arrears, total_due, status, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) RETURNING *",
        )
        .bind(id)
        .bind(school_id)
        .bind(student_id)
        .bind(academic_year_id)
        .bind(section_id)
        .bind(&voucher_no)
        .bind(billing_month)
        .bind(issue_date)
        .bind(due_date)
        .bind(totals.gross)
        .bind(totals.sibling_discount)
        .bind(totals.concession_discount)
        .bind(totals.net_monthly)
        .bind(totals.arrears)
        .bind(totals.total_due)
        .bind(status)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        for (i, (head, amount)) in lines.iter().enumerate() {
            sqlx::query("INSERT INTO fee_voucher_lines (id, school_id, voucher_id, fee_head_id, description, amount, position, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)")
                .bind(new_id())
                .bind(school_id)
                .bind(id)
                .bind(head.id)
                .bind(&head.name)
                .bind(amount)
                .bind(i as i32)
                .bind(actor.user_id)
                .bind(hlc.now().to_string())
                .execute(&mut *conn)
                .await?;
        }
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Create,
            "fee_voucher",
            Some(id),
            None,
            Some(snapshot(&voucher)),
        )
        .await?;
        report.created += 1;
        report.voucher_ids.push(id);
    }
    Ok(report)
}

/// Waive the late fee on a voucher (requires approval upstream). Audited with the amount waived.
pub async fn waive_late_fee(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    reason: &str,
    approver: Uuid,
) -> DbResult<FeeVoucher> {
    let before = get_voucher(&mut *conn, id).await?;
    let balance = fees::voucher_balance(before.total_due, Decimal::ZERO, before.net_paid);
    let after = sqlx::query_as::<_, FeeVoucher>(
        "UPDATE fee_vouchers SET late_fee = 0, late_fee_waived = TRUE, waiver_reason = $2, waived_by = $3, waived_at = now(), status = $4,
            updated_at = now(), updated_by = $5, hlc = $6 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(reason.trim())
    .bind(approver)
    .bind(VoucherStatus::derive(balance, before.net_paid).as_str())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "fee_voucher",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Cancel an unpaid voucher (e.g. generated by mistake). Refused once anything was paid.
pub async fn cancel_voucher(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    reason: &str,
) -> DbResult<FeeVoucher> {
    let before = get_voucher(&mut *conn, id).await?;
    if !before.net_paid.is_zero() {
        return Err(DbError::Conflict(
            "a voucher with payments cannot be cancelled; reverse the receipts first".into(),
        ));
    }
    let after = sqlx::query_as::<_, FeeVoucher>(
        "UPDATE fee_vouchers SET status = 'cancelled', waiver_reason = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(reason.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "fee_voucher",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

#[derive(Debug, Clone, Default)]
pub struct VoucherFilter {
    pub billing_month: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    pub student_id: Option<Uuid>,
    pub q: Option<String>,
}

pub async fn list_vouchers(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &VoucherFilter,
    limit: i64,
) -> DbResult<Vec<FeeVoucherRow>> {
    Ok(sqlx::query_as::<_, FeeVoucherRow>(
        "SELECT v.id, v.voucher_no, v.billing_month, v.due_date, v.student_id, s.student_number, s.full_name AS student_name,
                c.name AS class_name, sec.name AS section_name, v.gross, v.sibling_discount, v.concession_discount, v.arrears,
                v.total_due, v.late_fee, v.late_fee_waived, v.net_paid, v.status
         FROM fee_vouchers v JOIN students s ON s.id = v.student_id
         LEFT JOIN sections sec ON sec.id = v.section_id LEFT JOIN classes c ON c.id = sec.class_id
         WHERE v.school_id = $1 AND v.deleted_at IS NULL
           AND ($2::date IS NULL OR v.billing_month = $2)
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND ($4::uuid IS NULL OR v.section_id = $4)
           AND ($5::text IS NULL OR v.status = $5)
           AND ($6::uuid IS NULL OR v.student_id = $6)
           AND ($7::text IS NULL OR s.full_name ILIKE '%' || $7 || '%' OR s.student_number ILIKE '%' || $7 || '%' OR v.voucher_no ILIKE '%' || $7 || '%')
         ORDER BY v.billing_month DESC, c.level, sec.name, s.full_name LIMIT $8",
    )
    .bind(school_id)
    .bind(f.billing_month)
    .bind(f.class_id)
    .bind(f.section_id)
    .bind(f.status.as_deref())
    .bind(f.student_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

// ---- receipts -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewPayment {
    pub voucher_id: Uuid,
    pub amount: Decimal,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub paid_on: NaiveDate,
    pub received_by: Uuid,
}

async fn apply_payment_to_voucher(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    voucher: &FeeVoucher,
    delta: Decimal,
) -> DbResult<FeeVoucher> {
    let net_paid = fees::money(voucher.net_paid + delta);
    let balance = fees::voucher_balance(voucher.total_due, voucher.late_fee, net_paid);
    let after = sqlx::query_as::<_, FeeVoucher>(
        "UPDATE fee_vouchers SET net_paid = $2, status = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1 RETURNING *",
    )
    .bind(voucher.id)
    .bind(net_paid)
    .bind(VoucherStatus::derive(balance, net_paid).as_str())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "fee_voucher",
        Some(voucher.id),
        Some(snapshot(voucher)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Record a (possibly partial) payment. Assesses the late fee first if the voucher is overdue.
/// Returns the receipt and the updated voucher.
pub async fn record_payment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    policy: &FeePolicy,
    p: NewPayment,
) -> DbResult<(FeeReceipt, FeeVoucher)> {
    let amount = fees::money(p.amount);
    if amount <= Decimal::ZERO {
        return Err(DbError::Conflict("payment amount must be positive".into()));
    }
    let voucher = get_voucher(&mut *conn, p.voucher_id).await?;
    if voucher.status == "cancelled" {
        return Err(DbError::Conflict("this voucher is cancelled".into()));
    }
    if voucher.status == "carried" {
        return Err(DbError::Conflict(
            "this voucher's balance was carried forward; pay the latest voucher instead".into(),
        ));
    }
    let voucher = assess_late_fee(&mut *conn, hlc, actor, policy, voucher, p.paid_on).await?;
    if amount > voucher.balance() {
        return Err(DbError::Conflict(format!(
            "payment exceeds the outstanding balance of {}",
            voucher.balance()
        )));
    }
    let seq = students::next_sequence(
        &mut *conn,
        hlc,
        voucher.school_id,
        &format!("receipt_no:{}", p.paid_on.year()),
    )
    .await?;
    let receipt_no = format!("R{}-{:06}", p.paid_on.year(), seq);
    let id = new_id();
    let receipt = sqlx::query_as::<_, FeeReceipt>(
        "INSERT INTO fee_receipts (id, school_id, voucher_id, student_id, receipt_no, paid_on, amount, method, reference, note, received_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING *",
    )
    .bind(id)
    .bind(voucher.school_id)
    .bind(voucher.id)
    .bind(voucher.student_id)
    .bind(&receipt_no)
    .bind(p.paid_on)
    .bind(amount)
    .bind(p.method.as_str())
    .bind(p.reference.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()))
    .bind(p.note.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()))
    .bind(p.received_by)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::Create,
        "fee_receipt",
        Some(id),
        None,
        Some(snapshot(&receipt)),
    )
    .await?;
    let voucher = apply_payment_to_voucher(conn, hlc, actor, &voucher, amount).await?;
    Ok((receipt, voucher))
}

/// Reverse a receipt: a new negative receipt that references it. The original is untouched.
pub async fn reverse_receipt(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    receipt_id: Uuid,
    reason: &str,
    received_by: Uuid,
    on: NaiveDate,
) -> DbResult<(FeeReceipt, FeeVoucher)> {
    let original = get_receipt(&mut *conn, receipt_id).await?;
    if original.reverses_id.is_some() {
        return Err(DbError::Conflict("a reversal cannot be reversed".into()));
    }
    let already: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM fee_receipts WHERE reverses_id = $1)")
            .bind(receipt_id)
            .fetch_one(&mut *conn)
            .await?;
    if already {
        return Err(DbError::Conflict(
            "this receipt has already been reversed".into(),
        ));
    }
    if reason.trim().is_empty() {
        return Err(DbError::Conflict(
            "a reason is required to reverse a receipt".into(),
        ));
    }
    let voucher = get_voucher(&mut *conn, original.voucher_id).await?;
    let seq = students::next_sequence(
        &mut *conn,
        hlc,
        original.school_id,
        &format!("receipt_no:{}", on.year()),
    )
    .await?;
    let receipt_no = format!("R{}-{:06}", on.year(), seq);
    let id = new_id();
    let reversal = sqlx::query_as::<_, FeeReceipt>(
        "INSERT INTO fee_receipts (id, school_id, voucher_id, student_id, receipt_no, paid_on, amount, method, reference, note, reverses_id, reversal_reason, received_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING *",
    )
    .bind(id)
    .bind(original.school_id)
    .bind(original.voucher_id)
    .bind(original.student_id)
    .bind(&receipt_no)
    .bind(on)
    .bind(-original.amount)
    .bind(&original.method)
    .bind(&original.reference)
    .bind(format!("Reversal of {}", original.receipt_no))
    .bind(receipt_id)
    .bind(reason.trim())
    .bind(received_by)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::Create,
        "fee_receipt",
        Some(id),
        Some(snapshot(&original)),
        Some(snapshot(&reversal)),
    )
    .await?;
    let voucher = apply_payment_to_voucher(conn, hlc, actor, &voucher, -original.amount).await?;
    Ok((reversal, voucher))
}

pub async fn get_receipt(conn: &mut PgConnection, id: Uuid) -> DbResult<FeeReceipt> {
    sqlx::query_as::<_, FeeReceipt>("SELECT * FROM fee_receipts WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("receipt".into()))
}

pub async fn receipts_for_voucher(
    conn: &mut PgConnection,
    voucher_id: Uuid,
) -> DbResult<Vec<FeeReceipt>> {
    Ok(sqlx::query_as::<_, FeeReceipt>(
        "SELECT * FROM fee_receipts WHERE voucher_id = $1 ORDER BY created_at",
    )
    .bind(voucher_id)
    .fetch_all(conn)
    .await?)
}

pub async fn receipts_for_student(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<Vec<FeeReceipt>> {
    Ok(sqlx::query_as::<_, FeeReceipt>(
        "SELECT * FROM fee_receipts WHERE student_id = $1 ORDER BY created_at DESC",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// Receipts of one day (collections), newest first.
/// Net amount received between two dates inclusive (reversals are negative receipts).
pub async fn collected_between(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Decimal> {
    let total: Option<Decimal> = sqlx::query_scalar(
        "SELECT sum(amount) FROM fee_receipts WHERE school_id = $1 AND paid_on BETWEEN $2 AND $3",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_one(conn)
    .await?;
    Ok(fees::money(total.unwrap_or_default()))
}

pub async fn receipts_on(
    conn: &mut PgConnection,
    school_id: Uuid,
    day: NaiveDate,
) -> DbResult<Vec<FeeReceipt>> {
    Ok(sqlx::query_as::<_, FeeReceipt>(
        "SELECT * FROM fee_receipts WHERE school_id = $1 AND paid_on = $2 ORDER BY created_at DESC",
    )
    .bind(school_id)
    .bind(day)
    .fetch_all(conn)
    .await?)
}

pub async fn collection_summary(
    conn: &mut PgConnection,
    school_id: Uuid,
    day: NaiveDate,
) -> DbResult<Vec<CollectionByMethod>> {
    Ok(sqlx::query_as::<_, CollectionByMethod>(
        "SELECT method, count(*) AS receipts, coalesce(sum(amount), 0)::numeric(14,2) AS amount FROM fee_receipts
         WHERE school_id = $1 AND paid_on = $2 GROUP BY method ORDER BY method",
    )
    .bind(school_id)
    .bind(day)
    .fetch_all(conn)
    .await?)
}

// ---- ledger and defaulters ----------------------------------------------------------------

/// Ledger totals for a student from their non-cancelled vouchers.
pub async fn ledger_totals(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<fees::LedgerTotals> {
    let vouchers = vouchers_for_student(conn, student_id).await?;
    let entries: Vec<fees::LedgerEntry> = vouchers
        .iter()
        .filter(|v| v.status != "cancelled")
        .map(|v| fees::LedgerEntry {
            gross: v.gross,
            discounts: v.sibling_discount + v.concession_discount,
            late_fee: v.late_fee,
            net_paid: v.net_paid,
        })
        .collect();
    Ok(fees::ledger_totals(&entries))
}

/// Students whose balance is overdue as of `as_of`: days overdue count from the oldest month
/// still unpaid (including months carried forward), the balance is the student's open balance.
pub async fn defaulters(
    conn: &mut PgConnection,
    school_id: Uuid,
    as_of: NaiveDate,
    class_id: Option<Uuid>,
    min_days: i32,
) -> DbResult<Vec<Defaulter>> {
    Ok(sqlx::query_as::<_, Defaulter>(
        "SELECT s.id AS student_id, s.student_number, s.full_name AS student_name, s.father_name,
                c.name AS class_name, sec.name AS section_name,
                (SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
                  WHERE sg.student_id = s.id AND sg.deleted_at IS NULL ORDER BY sg.is_primary DESC LIMIT 1) AS guardian_phone,
                min(v.due_date) AS oldest_due_date,
                ($2::date - min(v.due_date))::int AS days_overdue,
                count(*) AS unpaid_vouchers,
                (SELECT coalesce(sum(o.total_due + o.late_fee - o.net_paid), 0)::numeric(14,2) FROM fee_vouchers o
                  WHERE o.student_id = s.id AND o.status IN ('unpaid', 'partial') AND o.deleted_at IS NULL) AS balance
         FROM fee_vouchers v JOIN students s ON s.id = v.student_id
         LEFT JOIN academic_years y ON y.school_id = s.school_id AND y.is_active AND y.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = y.id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id LEFT JOIN classes c ON c.id = sec.class_id
         WHERE v.school_id = $1 AND v.deleted_at IS NULL AND v.status IN ('unpaid', 'partial', 'carried') AND v.due_date < $2
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND EXISTS (SELECT 1 FROM fee_vouchers o WHERE o.student_id = s.id AND o.status IN ('unpaid', 'partial') AND o.deleted_at IS NULL AND o.due_date < $2)
         GROUP BY s.id, s.student_number, s.full_name, s.father_name, c.name, sec.name, c.level
         HAVING ($2::date - min(v.due_date))::int >= $4
         ORDER BY days_overdue DESC, balance DESC",
    )
    .bind(school_id)
    .bind(as_of)
    .bind(class_id)
    .bind(min_days)
    .fetch_all(conn)
    .await?)
}

/// Sum of outstanding balances of a student's open vouchers.
pub async fn student_balance(conn: &mut PgConnection, student_id: Uuid) -> DbResult<Decimal> {
    let v: Option<Decimal> = sqlx::query_scalar(
        "SELECT sum(total_due + late_fee - net_paid)::numeric(14,2) FROM fee_vouchers WHERE student_id = $1 AND status IN ('unpaid', 'partial') AND deleted_at IS NULL",
    )
    .bind(student_id)
    .fetch_one(conn)
    .await?;
    Ok(v.unwrap_or(Decimal::ZERO))
}

/// Section ids of a class in a year (helper for "generate for a class").
pub async fn class_section_ids(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    class_id: Uuid,
) -> DbResult<Vec<Uuid>> {
    Ok(
        academics::list_sections(conn, school_id, Some(academic_year_id), Some(class_id))
            .await?
            .into_iter()
            .map(|s| s.id)
            .collect(),
    )
}
