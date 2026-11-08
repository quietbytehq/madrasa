//! Library (Standard stage 4): catalogue and copies, member search, the issue/return desk
//! (issue, renew, return, lost), fines (waive, settle), loan rules and the stock take.
//! Every route needs the `library` feature and `library.read`; changes need
//! `library.manage`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::edition::Feature;
use madrasa_core::library::{LibraryRules, MemberKind};
use madrasa_core::permissions::Permission;
use madrasa_db::library as repo;
use madrasa_db::library::{Book, BookView, Copy, CopyView, Loan, LoanView, StockRow};
use madrasa_db::{academics, fees, staff as staff_repo, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/library/books", get(list_books).post(create_book))
        .route(
            "/library/books/{id}",
            get(get_book).put(update_book).delete(delete_book),
        )
        .route("/library/books/{id}/copies", post(add_copies))
        .route("/library/copies", get(list_copies))
        .route("/library/copies/lookup", get(lookup_copy))
        .route("/library/copies/{id}", put(update_copy))
        .route("/library/members", get(members))
        .route("/library/members/{kind}/{id}", get(member))
        .route("/library/loans", get(list_loans).post(issue))
        .route("/library/loans/{id}/renew", post(renew))
        .route("/library/loans/{id}/return", post(return_loan))
        .route("/library/loans/{id}/waive", post(waive))
        .route("/library/loans/{id}/settle", post(settle))
        .route("/library/settings", get(get_settings).put(put_settings))
        .route("/library/stock", get(stock))
}

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Library)?;
    user.require(if manage {
        Permission::LibraryManage
    } else {
        Permission::LibraryRead
    })
}

fn today() -> NaiveDate {
    crate::clock::today()
}

// ---- books and copies -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BookQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub subject: Option<String>,
}

async fn list_books(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<BookQuery>,
) -> ApiResult<Json<Vec<BookView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_books(
            &mut c,
            user.user.school_id,
            &repo::BookFilter {
                q: q.q,
                category: q.category,
                subject: q.subject,
            },
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct BookRequest {
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub isbn: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub custom: Value,
    /// Accession numbers of copies to add at once (new books).
    #[serde(default)]
    pub accession_nos: Vec<String>,
    #[serde(default)]
    pub shelf: String,
}

impl BookRequest {
    fn input(&self) -> ApiResult<repo::BookInput> {
        if self.title.trim().is_empty() {
            return Err(ApiError::validation("The book needs a title."));
        }
        Ok(repo::BookInput {
            title: self.title.clone(),
            author: self.author.clone(),
            publisher: self.publisher.clone(),
            isbn: self.isbn.clone(),
            category: self.category.clone(),
            language: self.language.clone(),
            subject: self.subject.clone(),
            year: self.year,
            custom: self.custom.clone(),
        })
    }
}

/// Mirrored by `BookDetail` in the UI.
#[derive(Debug, Serialize)]
pub struct BookDetail {
    #[serde(flatten)]
    pub book: BookView,
    /// Every copy of the book (the flattened `copies` is the count).
    pub copy_list: Vec<Copy>,
    /// Open loans of this book's copies.
    pub loans: Vec<LoanView>,
}

async fn book_detail(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<BookDetail> {
    let book = repo::get_book(c, id).await?;
    if book.book.school_id != user.user.school_id {
        return Err(ApiError::not_found("book"));
    }
    let copies = repo::copies_of(c, id).await?;
    let loans = repo::list_loans(
        c,
        user.user.school_id,
        &repo::LoanFilter {
            state: Some("open".into()),
            ..Default::default()
        },
        today(),
    )
    .await?
    .into_iter()
    .filter(|l| copies.iter().any(|cp| cp.id == l.loan.copy_id))
    .collect();
    Ok(BookDetail {
        book,
        copy_list: copies,
        loans,
    })
}

async fn create_book(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<BookRequest>,
) -> ApiResult<(StatusCode, Json<BookDetail>)> {
    gate(&state, &user, true)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    let book = repo::create_book(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &input,
    )
    .await?;
    if !req.accession_nos.is_empty() {
        repo::add_copies(
            &mut tx,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            book.id,
            &req.accession_nos,
            &req.shelf,
        )
        .await?;
    }
    let d = book_detail(&mut tx, &user, book.id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

async fn get_book(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookDetail>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(book_detail(&mut c, &user, id).await?))
}

async fn update_book(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<BookRequest>,
) -> ApiResult<Json<BookDetail>> {
    gate(&state, &user, true)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    book_detail(&mut tx, &user, id).await?;
    let _: Book = repo::update_book(&mut tx, &state.hlc, user.actor(), id, &input).await?;
    let d = book_detail(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

async fn delete_book(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    book_detail(&mut tx, &user, id).await?;
    repo::delete_book(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct CopiesRequest {
    pub accession_nos: Vec<String>,
    #[serde(default)]
    pub shelf: String,
}

async fn add_copies(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CopiesRequest>,
) -> ApiResult<Json<BookDetail>> {
    gate(&state, &user, true)?;
    if req.accession_nos.is_empty() {
        return Err(ApiError::validation("Give at least one accession number."));
    }
    let mut tx = state.pool.begin().await?;
    book_detail(&mut tx, &user, id).await?;
    repo::add_copies(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        id,
        &req.accession_nos,
        &req.shelf,
    )
    .await?;
    let d = book_detail(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

#[derive(Debug, Deserialize)]
pub struct CopyQuery {
    pub status: Option<String>,
}

async fn list_copies(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<CopyQuery>,
) -> ApiResult<Json<Vec<CopyView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_copies(
            &mut c,
            user.user.school_id,
            q.status.as_deref().filter(|s| !s.is_empty()),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    pub accession_no: String,
}

/// The copy behind an accession number (barcode text) with its open loan, for the desk.
#[derive(Debug, Serialize)]
pub struct CopyLookup {
    #[serde(flatten)]
    pub copy: CopyView,
    pub loan: Option<LoanView>,
}

async fn lookup_copy(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<LookupQuery>,
) -> ApiResult<Json<CopyLookup>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let copy = repo::find_copy(&mut c, user.user.school_id, &q.accession_no)
        .await?
        .ok_or_else(|| ApiError::not_found("copy"))?;
    let loan = repo::list_loans(
        &mut c,
        user.user.school_id,
        &repo::LoanFilter {
            state: Some("open".into()),
            ..Default::default()
        },
        today(),
    )
    .await?
    .into_iter()
    .find(|l| l.loan.copy_id == copy.copy.id);
    Ok(Json(CopyLookup { copy, loan }))
}

#[derive(Debug, Deserialize)]
pub struct CopyRequest {
    #[serde(default)]
    pub shelf: String,
    #[serde(default = "good")]
    pub condition: String,
    #[serde(default = "available")]
    pub status: String,
}

fn good() -> String {
    "good".into()
}
fn available() -> String {
    "available".into()
}

async fn update_copy(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CopyRequest>,
) -> ApiResult<Json<Copy>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let copy = repo::get_copy(&mut tx, id).await?;
    if copy.copy.school_id != user.user.school_id {
        return Err(ApiError::not_found("copy"));
    }
    let row = repo::update_copy(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        &req.shelf,
        &req.condition,
        &req.status,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- members --------------------------------------------------------------------------------

/// A borrower as shown on the desk. Mirrored by `LibraryMember` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub kind: MemberKind,
    pub id: Uuid,
    pub name: String,
    pub number: String,
    /// Class and section, or designation.
    pub detail: String,
}

#[derive(Debug, Deserialize)]
pub struct MemberQuery {
    pub kind: Option<MemberKind>,
    pub q: Option<String>,
}

fn parse_kind(s: &str) -> ApiResult<MemberKind> {
    MemberKind::parse(s)
        .ok_or_else(|| ApiError::validation("A member is a student or a staff member."))
}

/// Search students and staff by name or number.
async fn members(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<MemberQuery>,
) -> ApiResult<Json<Vec<Member>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let mut out = Vec::new();
    if q.kind != Some(MemberKind::Staff) {
        let page = students::list(
            &mut c,
            school,
            &students::StudentFilter {
                q: q.q.clone(),
                class_id: None,
                section_id: None,
                status: Some("active".into()),
                section_scope: None,
            },
            students::StudentSort::Name,
            false,
            1,
            15,
        )
        .await?;
        out.extend(page.items.into_iter().map(|s| Member {
            kind: MemberKind::Student,
            id: s.id,
            name: s.full_name,
            number: s.student_number,
            detail: match (s.class_name, s.section_name) {
                (Some(c), Some(sec)) => format!("{c}-{sec}"),
                (Some(c), None) => c,
                _ => String::new(),
            },
        }));
    }
    if q.kind != Some(MemberKind::Student) {
        out.extend(
            staff_repo::list(&mut c, school, q.q.as_deref())
                .await?
                .into_iter()
                .filter(|s| s.status == "active")
                .take(15)
                .map(|s| Member {
                    kind: MemberKind::Staff,
                    id: s.id,
                    name: s.full_name,
                    number: s.staff_number,
                    detail: s.designation,
                }),
        );
    }
    Ok(Json(out))
}

/// Mirrored by `MemberPage` in the UI.
#[derive(Debug, Serialize)]
pub struct MemberPage {
    pub member: Member,
    pub loans: Vec<LoanView>,
    pub open: usize,
    pub overdue: usize,
    /// Fines still owed.
    pub fines_due: Decimal,
    pub rules: madrasa_core::library::LoanRules,
}

async fn load_member(
    c: &mut PgConnection,
    user: &CurrentUser,
    kind: MemberKind,
    id: Uuid,
) -> ApiResult<Member> {
    match kind {
        MemberKind::Student => {
            let s = students::get(c, id).await?;
            if s.school_id != user.user.school_id {
                return Err(ApiError::not_found("student"));
            }
            let year = academics::active_year(c, user.user.school_id).await?;
            let detail = match year {
                Some(y) => match students::section_in_year(c, s.id, y.id).await? {
                    Some(sec) => {
                        let section = academics::get_section(c, sec).await?;
                        let class = academics::get_class(c, section.class_id).await?;
                        format!("{}-{}", class.name, section.name)
                    }
                    None => String::new(),
                },
                None => String::new(),
            };
            Ok(Member {
                kind,
                id: s.id,
                name: s.full_name,
                number: s.student_number,
                detail,
            })
        }
        MemberKind::Staff => {
            let s = staff_repo::get(c, id).await?;
            if s.school_id != user.user.school_id {
                return Err(ApiError::not_found("staff"));
            }
            Ok(Member {
                kind,
                id: s.id,
                name: s.full_name,
                number: s.staff_number,
                detail: s.designation,
            })
        }
    }
}

async fn member_page(
    c: &mut PgConnection,
    user: &CurrentUser,
    kind: MemberKind,
    id: Uuid,
) -> ApiResult<MemberPage> {
    let member = load_member(c, user, kind, id).await?;
    let loans = repo::list_loans(
        c,
        user.user.school_id,
        &repo::LoanFilter {
            member_kind: Some(kind),
            member_id: Some(id),
            ..Default::default()
        },
        today(),
    )
    .await?;
    let t = today();
    let open = loans
        .iter()
        .filter(|l| l.loan.returned_on.is_none())
        .count();
    let overdue = loans
        .iter()
        .filter(|l| l.loan.returned_on.is_none() && l.loan.due_on < t)
        .count();
    let fines_due = loans.iter().map(|l| l.loan.fine_due()).sum();
    let rules = repo::rules(c, user.user.school_id).await?;
    Ok(MemberPage {
        member,
        loans,
        open,
        overdue,
        fines_due,
        rules: rules.for_kind(kind).clone(),
    })
}

async fn member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((kind, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<MemberPage>> {
    gate(&state, &user, false)?;
    let kind = parse_kind(&kind)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(member_page(&mut c, &user, kind, id).await?))
}

// ---- loans ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoanQuery {
    pub state: Option<String>,
    pub member_kind: Option<MemberKind>,
    pub member_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

async fn list_loans(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<LoanQuery>,
) -> ApiResult<Json<Vec<LoanView>>> {
    gate(&state, &user, false)?;
    if let Some(s) = q.state.as_deref() {
        if !["", "open", "overdue", "returned", "fines"].contains(&s) {
            return Err(ApiError::validation(
                "state is open, overdue, returned or fines.",
            ));
        }
    }
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_loans(
            &mut c,
            user.user.school_id,
            &repo::LoanFilter {
                member_kind: q.member_kind,
                member_id: q.member_id,
                state: q.state.filter(|s| !s.is_empty()),
                from: q.from,
                to: q.to,
            },
            today(),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct IssueRequest {
    /// Either the copy id or its accession number.
    #[serde(default)]
    pub copy_id: Option<Uuid>,
    #[serde(default)]
    pub accession_no: Option<String>,
    pub member_kind: MemberKind,
    pub member_id: Uuid,
    #[serde(default)]
    pub issued_on: Option<NaiveDate>,
}

async fn issue(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<IssueRequest>,
) -> ApiResult<(StatusCode, Json<MemberPage>)> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    load_member(&mut tx, &user, req.member_kind, req.member_id).await?;
    let copy_id = match (req.copy_id, req.accession_no.as_deref()) {
        (Some(id), _) => id,
        (None, Some(no)) => {
            repo::find_copy(&mut tx, school, no)
                .await?
                .ok_or_else(|| ApiError::not_found("copy"))?
                .copy
                .id
        }
        (None, None) => return Err(ApiError::validation("Give the copy's accession number.")),
    };
    let issued_on = req.issued_on.unwrap_or_else(today);
    if issued_on > today() {
        return Err(ApiError::validation(
            "A book cannot be issued on a future date.",
        ));
    }
    let _: Loan = repo::issue(
        &mut tx,
        &state.hlc,
        user.actor(),
        school,
        copy_id,
        req.member_kind,
        req.member_id,
        issued_on,
        user.user.id,
    )
    .await?;
    let page = member_page(&mut tx, &user, req.member_kind, req.member_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(page)))
}

async fn load_loan(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<LoanView> {
    let l = repo::get_loan(c, id).await?;
    if l.loan.school_id != user.user.school_id {
        return Err(ApiError::not_found("loan"));
    }
    Ok(l)
}

async fn renew(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<LoanView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_loan(&mut tx, &user, id).await?;
    repo::renew(&mut tx, &state.hlc, user.actor(), id, today()).await?;
    let v = load_loan(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(v))
}

#[derive(Debug, Deserialize, Default)]
pub struct ReturnRequest {
    #[serde(default)]
    pub returned_on: Option<NaiveDate>,
    #[serde(default)]
    pub lost: bool,
    /// Charge for a lost copy (the school's replacement price).
    #[serde(default)]
    pub lost_charge: Option<Decimal>,
}

async fn return_loan(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<ReturnRequest>>,
) -> ApiResult<Json<LoanView>> {
    gate(&state, &user, true)?;
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let returned_on = req.returned_on.unwrap_or_else(today);
    if returned_on > today() {
        return Err(ApiError::validation(
            "A book cannot be returned on a future date.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    load_loan(&mut tx, &user, id).await?;
    repo::return_loan(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        returned_on,
        req.lost,
        req.lost_charge.unwrap_or(Decimal::ZERO),
        user.user.id,
    )
    .await?;
    let v = load_loan(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
pub struct WaiveRequest {
    pub amount: Decimal,
    pub reason: String,
}

async fn waive(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<WaiveRequest>,
) -> ApiResult<Json<LoanView>> {
    gate(&state, &user, true)?;
    if req.reason.trim().is_empty() {
        return Err(ApiError::validation("A waiver needs a reason."));
    }
    let mut tx = state.pool.begin().await?;
    load_loan(&mut tx, &user, id).await?;
    repo::waive(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        req.amount,
        &req.reason,
        user.user.id,
    )
    .await?;
    let v = load_loan(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(v))
}

/// Mirrored by `SettleResult` in the UI.
#[derive(Debug, Serialize)]
pub struct SettleResult {
    pub loan: LoanView,
    pub voucher_id: Option<Uuid>,
}

async fn settle(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SettleResult>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_loan(&mut tx, &user, id).await?;
    let year = academics::active_year(&mut tx, user.user.school_id)
        .await?
        .ok_or_else(|| ApiError::conflict("There is no active academic year."))?;
    let (_, voucher_id) =
        repo::settle(&mut tx, &state.hlc, user.actor(), id, today(), year.id).await?;
    let loan = load_loan(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(SettleResult { loan, voucher_id }))
}

// ---- settings and stock -------------------------------------------------------------------------

/// Mirrored by `LibrarySettings` in the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct LibrarySettings {
    pub rules: LibraryRules,
    pub fine_fee_head_id: Option<Uuid>,
    #[serde(default)]
    pub fine_fee_head_name: Option<String>,
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<LibrarySettings>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let head = repo::fine_fee_head(&mut c, user.user.school_id).await?;
    let name = match head {
        Some(id) => fees::get_head(&mut c, id).await.ok().map(|h| h.name),
        None => None,
    };
    Ok(Json(LibrarySettings {
        rules: repo::rules(&mut c, user.user.school_id).await?,
        fine_fee_head_id: head,
        fine_fee_head_name: name,
    }))
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<LibrarySettings>,
) -> ApiResult<Json<LibrarySettings>> {
    gate(&state, &user, true)?;
    req.rules.check().map_err(ApiError::validation)?;
    let mut tx = state.pool.begin().await?;
    let name = match req.fine_fee_head_id {
        Some(id) => {
            let h = fees::get_head(&mut tx, id).await?;
            if h.school_id != user.user.school_id {
                return Err(ApiError::not_found("fee head"));
            }
            Some(h.name)
        }
        None => None,
    };
    repo::set_rules(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &req.rules,
    )
    .await?;
    repo::set_fine_fee_head(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.fine_fee_head_id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(LibrarySettings {
        rules: req.rules,
        fine_fee_head_id: req.fine_fee_head_id,
        fine_fee_head_name: name,
    }))
}

async fn stock(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Vec<StockRow>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::stock(&mut c, user.user.school_id).await?))
}
