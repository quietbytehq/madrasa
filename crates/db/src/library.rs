//! Library (Standard stage 4): the catalogue (books with copies), loans to students and
//! staff with renewals, fines, waivers and settlement, and the stock summary. Rules live
//! in `madrasa_core::library` and in the setting `library.rules`.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::library::{self as rules, LibraryRules, MemberKind};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{fees, map_unique, settings, Actor, DbError, DbResult};

pub const KEY_RULES: &str = "library.rules";
/// Fee head for posting student fines as a voucher (empty = fines are only recorded).
pub const KEY_FINE_FEE_HEAD: &str = "library.fine_fee_head_id";

// ---- rules and settings ------------------------------------------------------------------------

pub async fn rules(conn: &mut PgConnection, school_id: Uuid) -> DbResult<LibraryRules> {
    Ok(settings::get(conn, Some(school_id), KEY_RULES)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub async fn set_rules(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    r: &LibraryRules,
) -> DbResult<()> {
    r.check().map_err(DbError::Conflict)?;
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_RULES,
        serde_json::to_value(r).unwrap_or(Value::Null),
    )
    .await?;
    Ok(())
}

pub async fn fine_fee_head(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Option<Uuid>> {
    Ok(settings::get(conn, Some(school_id), KEY_FINE_FEE_HEAD)
        .await?
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok())))
}

pub async fn set_fine_fee_head(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    head: Option<Uuid>,
) -> DbResult<()> {
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_FINE_FEE_HEAD,
        match head {
            Some(id) => json!(id.to_string()),
            None => Value::Null,
        },
    )
    .await?;
    Ok(())
}

// ---- books ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Book {
    pub id: Uuid,
    pub school_id: Uuid,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub isbn: String,
    pub category: String,
    pub language: String,
    pub subject: String,
    pub year: Option<i32>,
    pub custom: Value,
    pub hlc: String,
}

/// A book with its copy counts. Mirrored by `BookView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BookView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub book: Book,
    pub copies: i64,
    pub available: i64,
    pub on_loan: i64,
}

#[derive(Debug, Clone, Default)]
pub struct BookInput {
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub isbn: String,
    pub category: String,
    pub language: String,
    pub subject: String,
    pub year: Option<i32>,
    pub custom: Value,
}

const BOOK_COLS: &str = "b.id, b.school_id, b.title, b.author, b.publisher, b.isbn, b.category, b.language, b.subject, b.year, b.custom, b.hlc";
const COUNTS: &str = "(SELECT count(*) FROM library_copies c WHERE c.book_id = b.id AND c.deleted_at IS NULL AND c.status <> 'withdrawn') AS copies,
            (SELECT count(*) FROM library_copies c WHERE c.book_id = b.id AND c.deleted_at IS NULL AND c.status = 'available') AS available,
            (SELECT count(*) FROM library_copies c WHERE c.book_id = b.id AND c.deleted_at IS NULL AND c.status = 'on_loan') AS on_loan";

#[derive(Debug, Clone, Default)]
pub struct BookFilter {
    /// Title, author, ISBN or an accession number.
    pub q: Option<String>,
    pub category: Option<String>,
    pub subject: Option<String>,
}

pub async fn list_books(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &BookFilter,
) -> DbResult<Vec<BookView>> {
    Ok(sqlx::query_as::<_, BookView>(sqlx::AssertSqlSafe(format!(
        "SELECT {BOOK_COLS}, {COUNTS} FROM library_books b
         WHERE b.school_id = $1 AND b.deleted_at IS NULL
           AND ($2::text IS NULL OR b.title ILIKE '%' || $2 || '%' OR b.author ILIKE '%' || $2 || '%' OR b.isbn ILIKE '%' || $2 || '%'
                OR EXISTS (SELECT 1 FROM library_copies c WHERE c.book_id = b.id AND c.deleted_at IS NULL AND c.accession_no ILIKE $2))
           AND ($3::text IS NULL OR b.category = $3)
           AND ($4::text IS NULL OR b.subject = $4)
         ORDER BY b.title, b.author"
    )))
    .bind(school_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(f.category.as_deref().filter(|s| !s.is_empty()))
    .bind(f.subject.as_deref().filter(|s| !s.is_empty()))
    .fetch_all(conn)
    .await?)
}

pub async fn get_book(conn: &mut PgConnection, id: Uuid) -> DbResult<BookView> {
    sqlx::query_as::<_, BookView>(sqlx::AssertSqlSafe(format!(
        "SELECT {BOOK_COLS}, {COUNTS} FROM library_books b WHERE b.id = $1 AND b.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("book".into()))
}

fn check_book(input: &BookInput) -> DbResult<()> {
    if input.title.trim().is_empty() {
        return Err(DbError::Conflict("the book needs a title".into()));
    }
    Ok(())
}

pub async fn create_book(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &BookInput,
) -> DbResult<Book> {
    check_book(input)?;
    let row = sqlx::query_as::<_, Book>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO library_books AS b (id, school_id, title, author, publisher, isbn, category, language, subject, year, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING {BOOK_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.title.trim())
    .bind(input.author.trim())
    .bind(input.publisher.trim())
    .bind(input.isbn.trim())
    .bind(input.category.trim())
    .bind(input.language.trim())
    .bind(input.subject.trim())
    .bind(input.year)
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "library_book",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_book(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &BookInput,
) -> DbResult<Book> {
    check_book(input)?;
    let old = get_book(&mut *conn, id).await?.book;
    let row = sqlx::query_as::<_, Book>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_books AS b SET title = $2, author = $3, publisher = $4, isbn = $5, category = $6, language = $7, subject = $8, year = $9, custom = $10,
                updated_at = now(), updated_by = $11, hlc = $12
         WHERE b.id = $1 AND b.deleted_at IS NULL RETURNING {BOOK_COLS}"
    )))
    .bind(id)
    .bind(input.title.trim())
    .bind(input.author.trim())
    .bind(input.publisher.trim())
    .bind(input.isbn.trim())
    .bind(input.category.trim())
    .bind(input.language.trim())
    .bind(input.subject.trim())
    .bind(input.year)
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_book",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Remove a book and its copies; refused while a copy is on loan.
pub async fn delete_book(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_book(&mut *conn, id).await?;
    if old.on_loan > 0 {
        return Err(DbError::Conflict("a copy of this book is on loan".into()));
    }
    sqlx::query("UPDATE library_copies SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE book_id = $1 AND deleted_at IS NULL")
        .bind(id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE library_books SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "library_book",
        Some(id),
        Some(snapshot(&old.book)),
        None,
    )
    .await?;
    Ok(())
}

// ---- copies ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Copy {
    pub id: Uuid,
    pub school_id: Uuid,
    pub book_id: Uuid,
    pub accession_no: String,
    pub shelf: String,
    pub condition: String,
    /// `available`, `on_loan`, `lost` or `withdrawn`.
    pub status: String,
    pub hlc: String,
}

/// A copy with its book, for desks and stock lists. Mirrored by `CopyView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CopyView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub copy: Copy,
    pub title: String,
    pub author: String,
    pub category: String,
}

const COPY_COLS: &str =
    "c.id, c.school_id, c.book_id, c.accession_no, c.shelf, c.condition, c.status, c.hlc";

pub async fn copies_of(conn: &mut PgConnection, book_id: Uuid) -> DbResult<Vec<Copy>> {
    Ok(sqlx::query_as::<_, Copy>(sqlx::AssertSqlSafe(format!(
        "SELECT {COPY_COLS} FROM library_copies c WHERE c.book_id = $1 AND c.deleted_at IS NULL ORDER BY c.accession_no"
    )))
    .bind(book_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_copy(conn: &mut PgConnection, id: Uuid) -> DbResult<CopyView> {
    sqlx::query_as::<_, CopyView>(sqlx::AssertSqlSafe(format!(
        "SELECT {COPY_COLS}, b.title, b.author, b.category FROM library_copies c JOIN library_books b ON b.id = c.book_id
         WHERE c.id = $1 AND c.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("copy".into()))
}

/// The copy with this accession number (barcode text), if any.
pub async fn find_copy(
    conn: &mut PgConnection,
    school_id: Uuid,
    accession_no: &str,
) -> DbResult<Option<CopyView>> {
    Ok(sqlx::query_as::<_, CopyView>(sqlx::AssertSqlSafe(format!(
        "SELECT {COPY_COLS}, b.title, b.author, b.category FROM library_copies c JOIN library_books b ON b.id = c.book_id
         WHERE c.school_id = $1 AND upper(c.accession_no) = upper($2) AND c.deleted_at IS NULL"
    )))
    .bind(school_id)
    .bind(accession_no.trim())
    .fetch_optional(conn)
    .await?)
}

/// Every copy, optionally by status, for the stock take.
pub async fn list_copies(
    conn: &mut PgConnection,
    school_id: Uuid,
    status: Option<&str>,
) -> DbResult<Vec<CopyView>> {
    Ok(sqlx::query_as::<_, CopyView>(sqlx::AssertSqlSafe(format!(
        "SELECT {COPY_COLS}, b.title, b.author, b.category FROM library_copies c JOIN library_books b ON b.id = c.book_id
         WHERE c.school_id = $1 AND c.deleted_at IS NULL AND b.deleted_at IS NULL AND ($2::text IS NULL OR c.status = $2)
         ORDER BY b.category, b.title, c.accession_no"
    )))
    .bind(school_id)
    .bind(status)
    .fetch_all(conn)
    .await?)
}

/// Add copies with the given accession numbers (unique per school).
pub async fn add_copies(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    book_id: Uuid,
    accession_nos: &[String],
    shelf: &str,
) -> DbResult<Vec<Copy>> {
    let mut out = Vec::new();
    for no in accession_nos {
        let no = no.trim();
        if no.is_empty() {
            return Err(DbError::Conflict("an accession number is empty".into()));
        }
        let row = sqlx::query_as::<_, Copy>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO library_copies AS c (id, school_id, book_id, accession_no, shelf, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COPY_COLS}"
        )))
        .bind(new_id())
        .bind(school_id)
        .bind(book_id)
        .bind(no)
        .bind(shelf.trim())
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| map_unique(e, &format!("a copy with accession number {no}")))?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Create,
            "library_copy",
            Some(row.id),
            None,
            Some(snapshot(&row)),
        )
        .await?;
        out.push(row);
    }
    Ok(out)
}

/// Change a copy's shelf, condition or status (lost / withdrawn / back to available); a
/// copy on loan keeps its status until returned.
pub async fn update_copy(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    shelf: &str,
    condition: &str,
    status: &str,
) -> DbResult<Copy> {
    let old = get_copy(&mut *conn, id).await?.copy;
    if !["good", "fair", "poor"].contains(&condition) {
        return Err(DbError::Conflict("condition is good, fair or poor".into()));
    }
    if !["available", "lost", "withdrawn"].contains(&status) {
        return Err(DbError::Conflict(
            "status is available, lost or withdrawn".into(),
        ));
    }
    if old.status == "on_loan" {
        return Err(DbError::Conflict(
            "this copy is on loan; return it first".into(),
        ));
    }
    let row = sqlx::query_as::<_, Copy>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_copies AS c SET shelf = $2, condition = $3, status = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE c.id = $1 RETURNING {COPY_COLS}"
    )))
    .bind(id)
    .bind(shelf.trim())
    .bind(condition)
    .bind(status)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_copy",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

async fn set_copy_status(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    status: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE library_copies SET status = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(conn)
        .await?;
    Ok(())
}

// ---- loans ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Loan {
    pub id: Uuid,
    pub school_id: Uuid,
    pub copy_id: Uuid,
    pub member_kind: String,
    pub member_id: Uuid,
    pub issued_on: NaiveDate,
    pub due_on: NaiveDate,
    pub returned_on: Option<NaiveDate>,
    pub renewals: i32,
    pub issued_by: Option<Uuid>,
    pub returned_by: Option<Uuid>,
    pub lost: bool,
    pub fine: Decimal,
    pub fine_waived: Decimal,
    pub waiver_reason: String,
    pub waived_by: Option<Uuid>,
    pub fine_voucher_id: Option<Uuid>,
    pub fine_settled_on: Option<NaiveDate>,
    pub hlc: String,
}

impl Loan {
    pub fn kind(&self) -> MemberKind {
        MemberKind::parse(&self.member_kind).unwrap_or(MemberKind::Student)
    }
    /// Fine still owed: the fine less the waiver, zero once settled.
    pub fn fine_due(&self) -> Decimal {
        if self.fine_settled_on.is_some() || self.fine_voucher_id.is_some() {
            Decimal::ZERO
        } else {
            (self.fine - self.fine_waived).max(Decimal::ZERO)
        }
    }
}

/// A loan with the book and the member's name. Mirrored by `LoanView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LoanView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub loan: Loan,
    pub accession_no: String,
    pub title: String,
    pub author: String,
    pub member_name: String,
    pub member_number: String,
    /// Class and section for students, designation for staff.
    pub member_detail: String,
}

const LOAN_COLS: &str = "l.id, l.school_id, l.copy_id, l.member_kind, l.member_id, l.issued_on, l.due_on, l.returned_on, l.renewals, l.issued_by, l.returned_by,
            l.lost, l.fine, l.fine_waived, l.waiver_reason, l.waived_by, l.fine_voucher_id, l.fine_settled_on, l.hlc";
const LOAN_JOIN: &str = "FROM library_loans l
         JOIN library_copies c ON c.id = l.copy_id
         JOIN library_books b ON b.id = c.book_id
         LEFT JOIN students s ON l.member_kind = 'student' AND s.id = l.member_id
         LEFT JOIN staff st ON l.member_kind = 'staff' AND st.id = l.member_id
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.deleted_at IS NULL
              AND e.academic_year_id = (SELECT id FROM academic_years y WHERE y.school_id = l.school_id AND y.is_active AND y.deleted_at IS NULL LIMIT 1)
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes cl ON cl.id = sec.class_id";
const LOAN_NAMES: &str = "c.accession_no, b.title, b.author,
            COALESCE(s.full_name, st.full_name, '') AS member_name,
            COALESCE(s.student_number, st.staff_number, '') AS member_number,
            CASE WHEN l.member_kind = 'student' THEN COALESCE(cl.name || '-' || sec.name, '') ELSE COALESCE(st.designation, '') END AS member_detail";

#[derive(Debug, Clone, Default)]
pub struct LoanFilter {
    pub member_kind: Option<MemberKind>,
    pub member_id: Option<Uuid>,
    /// `open`, `overdue`, `returned` or `fines` (returned with a fine still due).
    pub state: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

pub async fn list_loans(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &LoanFilter,
    today: NaiveDate,
) -> DbResult<Vec<LoanView>> {
    let state = f.state.clone().unwrap_or_default();
    Ok(sqlx::query_as::<_, LoanView>(sqlx::AssertSqlSafe(format!(
        "SELECT {LOAN_COLS}, {LOAN_NAMES} {LOAN_JOIN}
         WHERE l.school_id = $1 AND l.deleted_at IS NULL
           AND ($2::text IS NULL OR l.member_kind = $2)
           AND ($3::uuid IS NULL OR l.member_id = $3)
           AND ($4 <> 'open' OR l.returned_on IS NULL)
           AND ($4 <> 'overdue' OR (l.returned_on IS NULL AND l.due_on < $7))
           AND ($4 <> 'returned' OR l.returned_on IS NOT NULL)
           AND ($4 <> 'fines' OR (l.fine - l.fine_waived > 0 AND l.fine_settled_on IS NULL AND l.fine_voucher_id IS NULL))
           AND ($5::date IS NULL OR l.issued_on >= $5)
           AND ($6::date IS NULL OR l.issued_on <= $6)
         ORDER BY (l.returned_on IS NULL) DESC, l.due_on, l.issued_on DESC"
    )))
    .bind(school_id)
    .bind(f.member_kind.map(MemberKind::as_str))
    .bind(f.member_id)
    .bind(state)
    .bind(f.from)
    .bind(f.to)
    .bind(today)
    .fetch_all(conn)
    .await?)
}

pub async fn get_loan(conn: &mut PgConnection, id: Uuid) -> DbResult<LoanView> {
    sqlx::query_as::<_, LoanView>(sqlx::AssertSqlSafe(format!(
        "SELECT {LOAN_COLS}, {LOAN_NAMES} {LOAN_JOIN} WHERE l.id = $1 AND l.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("loan".into()))
}

/// Open loans of a member: (active, overdue) counts.
async fn member_counts(
    conn: &mut PgConnection,
    kind: MemberKind,
    member_id: Uuid,
    today: NaiveDate,
) -> DbResult<(u32, u32)> {
    let (active, overdue): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE due_on < $3) FROM library_loans
         WHERE member_kind = $1 AND member_id = $2 AND returned_on IS NULL AND deleted_at IS NULL",
    )
    .bind(kind.as_str())
    .bind(member_id)
    .bind(today)
    .fetch_one(conn)
    .await?;
    Ok((active as u32, overdue as u32))
}

#[allow(clippy::too_many_arguments)]
/// Issue a copy to a member; the rules of the member's kind apply.
pub async fn issue(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    copy_id: Uuid,
    kind: MemberKind,
    member_id: Uuid,
    issued_on: NaiveDate,
    issued_by: Uuid,
) -> DbResult<Loan> {
    let copy = get_copy(&mut *conn, copy_id).await?;
    if copy.copy.school_id != school_id {
        return Err(DbError::NotFound("copy".into()));
    }
    if copy.copy.status != "available" {
        return Err(DbError::Conflict(format!(
            "copy {} is {}",
            copy.copy.accession_no,
            copy.copy.status.replace('_', " ")
        )));
    }
    let r = rules(&mut *conn, school_id).await?;
    let lr = r.for_kind(kind);
    let (active, overdue) = member_counts(&mut *conn, kind, member_id, issued_on).await?;
    rules::can_issue(active, overdue, lr).map_err(DbError::Conflict)?;
    let due = rules::due_date(issued_on, lr);
    let row = sqlx::query_as::<_, Loan>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO library_loans AS l (id, school_id, copy_id, member_kind, member_id, issued_on, due_on, issued_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {LOAN_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(copy_id)
    .bind(kind.as_str())
    .bind(member_id)
    .bind(issued_on)
    .bind(due)
    .bind(issued_by)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    set_copy_status(&mut *conn, hlc, actor, copy_id, "on_loan").await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "library_loan",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn renew(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    today: NaiveDate,
) -> DbResult<Loan> {
    let old = get_loan(&mut *conn, id).await?.loan;
    if old.returned_on.is_some() {
        return Err(DbError::Conflict("this loan was returned".into()));
    }
    let r = rules(&mut *conn, old.school_id).await?;
    let due = rules::renew(
        old.due_on,
        old.renewals as u32,
        today,
        r.for_kind(old.kind()),
    )
    .map_err(DbError::Conflict)?;
    let row = sqlx::query_as::<_, Loan>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_loans AS l SET due_on = $2, renewals = renewals + 1, updated_at = now(), updated_by = $3, hlc = $4
         WHERE l.id = $1 RETURNING {LOAN_COLS}"
    )))
    .bind(id)
    .bind(due)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_loan",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Return a copy: the fine is computed from the rules; a lost copy is marked lost and
/// charged `lost_charge` instead (the school decides the amount).
#[allow(clippy::too_many_arguments)]
pub async fn return_loan(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    returned_on: NaiveDate,
    lost: bool,
    lost_charge: Decimal,
    returned_by: Uuid,
) -> DbResult<Loan> {
    let old = get_loan(&mut *conn, id).await?.loan;
    if old.returned_on.is_some() {
        return Err(DbError::Conflict("this loan was returned already".into()));
    }
    if returned_on < old.issued_on {
        return Err(DbError::Conflict(
            "the return date is before the issue date".into(),
        ));
    }
    if lost_charge < Decimal::ZERO {
        return Err(DbError::Conflict("the charge cannot be negative".into()));
    }
    let r = rules(&mut *conn, old.school_id).await?;
    let fine = if lost {
        lost_charge.round_dp(2)
    } else {
        rules::fine(old.due_on, returned_on, r.for_kind(old.kind()))
    };
    let row = sqlx::query_as::<_, Loan>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_loans AS l SET returned_on = $2, returned_by = $3, lost = $4, fine = $5, updated_at = now(), updated_by = $6, hlc = $7
         WHERE l.id = $1 RETURNING {LOAN_COLS}"
    )))
    .bind(id)
    .bind(returned_on)
    .bind(returned_by)
    .bind(lost)
    .bind(fine)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    set_copy_status(
        &mut *conn,
        hlc,
        actor,
        old.copy_id,
        if lost { "lost" } else { "available" },
    )
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_loan",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Waive part or all of a fine that is not settled yet; audited with the reason.
pub async fn waive(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    amount: Decimal,
    reason: &str,
    by: Uuid,
) -> DbResult<Loan> {
    let old = get_loan(&mut *conn, id).await?.loan;
    if old.fine_settled_on.is_some() || old.fine_voucher_id.is_some() {
        return Err(DbError::Conflict("this fine was settled already".into()));
    }
    if reason.trim().is_empty() {
        return Err(DbError::Conflict("a waiver needs a reason".into()));
    }
    rules::waive(old.fine, amount).map_err(DbError::Conflict)?;
    let row = sqlx::query_as::<_, Loan>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_loans AS l SET fine_waived = $2, waiver_reason = $3, waived_by = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE l.id = $1 RETURNING {LOAN_COLS}"
    )))
    .bind(id)
    .bind(amount.round_dp(2))
    .bind(reason.trim())
    .bind(by)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_loan",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Settle the fine due: students get a one-off voucher when a fee head is configured
/// (otherwise the fine is marked settled, i.e. collected at the desk); staff fines are
/// marked settled. Returns the voucher id when one was raised.
pub async fn settle(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    on: NaiveDate,
    academic_year_id: Uuid,
) -> DbResult<(Loan, Option<Uuid>)> {
    let view = get_loan(&mut *conn, id).await?;
    let old = view.loan.clone();
    let due = old.fine_due();
    if due <= Decimal::ZERO {
        return Err(DbError::Conflict("there is no fine to settle".into()));
    }
    let mut voucher = None;
    if old.kind() == MemberKind::Student {
        if let Some(head) = fine_fee_head(&mut *conn, old.school_id).await? {
            let section =
                crate::students::section_in_year(&mut *conn, old.member_id, academic_year_id)
                    .await?;
            // The fine joins this month's voucher when the student has one, else a one-off.
            let (v, _created) = fees::add_to_month_voucher(
                &mut *conn,
                hlc,
                actor,
                old.school_id,
                old.member_id,
                academic_year_id,
                section,
                on,
                head,
                &format!("Library fine: {} ({})", view.title, view.accession_no),
                due,
            )
            .await?;
            voucher = Some(v.id);
        }
    }
    let row = sqlx::query_as::<_, Loan>(sqlx::AssertSqlSafe(format!(
        "UPDATE library_loans AS l SET fine_voucher_id = $2, fine_settled_on = CASE WHEN $2::uuid IS NULL THEN $3 ELSE fine_settled_on END,
                updated_at = now(), updated_by = $4, hlc = $5
         WHERE l.id = $1 RETURNING {LOAN_COLS}"
    )))
    .bind(id)
    .bind(voucher)
    .bind(on)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "library_loan",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok((row, voucher))
}

// ---- summaries -----------------------------------------------------------------------------

/// Copies by status and category for the stock take and the report.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockRow {
    pub category: String,
    pub titles: i64,
    pub copies: i64,
    pub available: i64,
    pub on_loan: i64,
    pub lost: i64,
    pub withdrawn: i64,
}

pub async fn stock(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<StockRow>> {
    Ok(sqlx::query_as::<_, StockRow>(
        "SELECT b.category, count(DISTINCT b.id) AS titles, count(c.id) AS copies,
                count(c.id) FILTER (WHERE c.status = 'available') AS available,
                count(c.id) FILTER (WHERE c.status = 'on_loan') AS on_loan,
                count(c.id) FILTER (WHERE c.status = 'lost') AS lost,
                count(c.id) FILTER (WHERE c.status = 'withdrawn') AS withdrawn
         FROM library_books b LEFT JOIN library_copies c ON c.book_id = b.id AND c.deleted_at IS NULL
         WHERE b.school_id = $1 AND b.deleted_at IS NULL GROUP BY b.category ORDER BY b.category",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

/// Books by number of loans issued in a date range, most first.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PopularRow {
    pub book_id: Uuid,
    pub title: String,
    pub author: String,
    pub category: String,
    pub loans: i64,
}

pub async fn most_borrowed(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    limit: i64,
) -> DbResult<Vec<PopularRow>> {
    Ok(sqlx::query_as::<_, PopularRow>(
        "SELECT b.id AS book_id, b.title, b.author, b.category, count(l.id) AS loans
         FROM library_loans l JOIN library_copies c ON c.id = l.copy_id JOIN library_books b ON b.id = c.book_id
         WHERE l.school_id = $1 AND l.deleted_at IS NULL AND l.issued_on BETWEEN $2 AND $3
         GROUP BY b.id, b.title, b.author, b.category ORDER BY loans DESC, b.title LIMIT $4",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}
