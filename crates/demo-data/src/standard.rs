//! Demo data for the Standard modules: a generated timetable, leave, admissions, library,
//! transport, hostel, inventory and a read-only API key. Loaded with the rest of the demo
//! school; it stays out of sight until the install is switched to Standard.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use chrono::{Datelike, NaiveDate};
use madrasa_core::api_keys::Scope;
use madrasa_core::hlc::Hlc;
use madrasa_core::inventory::{MovementKind, RecipientKind};
use madrasa_core::library::MemberKind;
use madrasa_core::timetable as engine;
use madrasa_db::models::AcademicYear;
use madrasa_db::{
    admissions, api_keys, fees, hostel, inventory, leave, library, settings, timetable, transport,
    Actor,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::generate::DemoSchool;
use crate::load::LoadError;

/// The read-only API key of the demo school. Documented on purpose; demo installs are not
/// secure and the loader refuses to run next to a real school.
pub const DEMO_API_KEY: &str = "mk_demo-readonly-key-0000000000000000000";

/// What the rest of the loader already created.
pub struct Context<'a> {
    pub school_id: Uuid,
    pub year: &'a AcademicYear,
    pub demo: &'a DemoSchool,
    pub class_ids: &'a HashMap<u8, Uuid>,
    pub section_ids: &'a HashMap<String, Uuid>,
    pub student_ids: &'a HashMap<String, Uuid>,
    pub staff_by_username: &'a HashMap<String, Uuid>,
    pub principal_user: Uuid,
    pub admission_head: Uuid,
    /// The first day of the demo year's April.
    pub april: NaiveDate,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StandardReport {
    pub timetable_cards: usize,
    /// True when the generated timetable placed every lesson and was published.
    pub timetable_published: bool,
    /// Things the loader could not do, in plain words, for the setup wizard to show.
    pub notes: Vec<String>,
    pub leave_requests: usize,
    pub applications: usize,
    pub books: usize,
    pub loans: usize,
    pub routes: usize,
    pub transport_students: usize,
    pub hostel_residents: usize,
    pub inventory_items: usize,
    pub api_key_prefix: String,
}

/// Record something the loader skipped so it is seen (the wizard shows the notes) and not
/// silently missing from the demo school.
fn note(report: &mut StandardReport, message: String) {
    tracing::warn!(note = %message, "demo load note");
    report.notes.push(message);
}

fn teachers(ctx: &Context<'_>) -> Vec<Uuid> {
    ctx.demo
        .staff
        .iter()
        .filter(|m| m.role == "teacher")
        .filter_map(|m| ctx.staff_by_username.get(&m.username).copied())
        .collect()
}

/// Students of a section in roll order.
fn students_of(ctx: &Context<'_>, class: &str) -> Vec<Uuid> {
    students_matching(ctx, |s| s.class == class)
}

/// Boys of a section in roll order (the demo hostel is a boys' hostel).
fn boys_of(ctx: &Context<'_>, class: &str) -> Vec<Uuid> {
    students_matching(ctx, |s| s.class == class && s.gender == 'M')
}

fn students_matching(
    ctx: &Context<'_>,
    keep: impl Fn(&crate::generate::Student) -> bool,
) -> Vec<Uuid> {
    let mut rows: Vec<_> = ctx.demo.students.iter().filter(|s| keep(s)).collect();
    rows.sort_by_key(|s| s.roll_no);
    rows.iter()
        .filter_map(|s| ctx.student_ids.get(&s.admission_no).copied())
        .collect()
}

pub async fn load(
    tx: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    ctx: &Context<'_>,
) -> Result<StandardReport, LoadError> {
    let mut report = StandardReport::default();
    let school = ctx.school_id;
    let year = ctx.year;
    let april = ctx.april;
    let teacher_ids = teachers(ctx);

    // ---- Timetable: a shift, rooms, lessons from the allocations, one generated and published.
    timetable::create_shift(
        tx,
        hlc,
        actor,
        school,
        year.id,
        &timetable::default_shift_input("Morning"),
    )
    .await?;
    for (i, (name, building)) in [
        ("Room 1", "Main"),
        ("Room 2", "Main"),
        ("Room 3", "Main"),
        ("Science Lab", "Annex"),
        ("Computer Lab", "Annex"),
        ("Library Hall", "Annex"),
    ]
    .iter()
    .enumerate()
    {
        timetable::create_room(
            tx,
            hlc,
            actor,
            school,
            &timetable::RoomInput {
                name: (*name).into(),
                short_name: format!("R{}", i + 1),
                building: (*building).into(),
                capacity: Some(40),
            },
        )
        .await?;
    }
    let lessons =
        timetable::seed_lessons_from_allocations(tx, hlc, actor, school, year.id, 4).await?;
    if lessons > 0 {
        let tt =
            timetable::create_timetable(tx, hlc, actor, school, year.id, "Demo timetable").await?;
        let ys = timetable::setup_for_year(tx, school, year, Some(tt.id)).await?;
        let check = engine::check::run(&ys.setup);
        if check.errors > 0 {
            let first = check
                .findings
                .iter()
                .find(|f| f.level == "error")
                .map(|f| f.message.as_str())
                .unwrap_or_default();
            note(
                &mut report,
                format!(
                    "The demo timetable was not generated: the timetable check found {} problem(s). First: {first}",
                    check.errors
                ),
            );
        } else {
            let opts = engine::Options {
                time_limit: Duration::from_secs(20),
                improve_time: Duration::ZERO,
                seed: 1,
                ..Default::default()
            };
            let cancel = AtomicBool::new(false);
            let solved = engine::solve(&ys.setup, &[], &opts, &cancel, &mut |_| {});
            let placed: Vec<timetable::PlacedInput> = solved
                .timetable
                .cards
                .iter()
                .filter_map(timetable::PlacedInput::from_engine)
                .collect();
            report.timetable_cards = placed.len();
            timetable::replace_cards(tx, hlc, actor, school, tt.id, &placed, None).await?;
            if solved.unplaced.is_empty() {
                timetable::publish_timetable(tx, hlc, actor, tt.id).await?;
                report.timetable_published = true;
            } else {
                note(
                    &mut report,
                    format!(
                        "The demo timetable was generated, but {} of {} lessons could not be placed, so it was saved as a draft and not published. Open Timetable to finish it.",
                        solved.unplaced.len(),
                        solved.total
                    ),
                );
            }
        }
    } else {
        note(
            &mut report,
            "The demo timetable was not generated: no lessons could be made from the teacher allocations.".into(),
        );
    }

    // ---- Leave: three types, balances for every teacher, a few requests.
    let mut type_ids = Vec::new();
    for (i, (name, code, days)) in [
        ("Casual", "CL", 10),
        ("Sick", "SL", 8),
        ("Annual", "AL", 15),
    ]
    .iter()
    .enumerate()
    {
        let t = leave::create_type(
            tx,
            hlc,
            actor,
            school,
            &leave::LeaveTypeInput {
                name: (*name).into(),
                code: (*code).into(),
                entitlement_days: Decimal::from(*days),
                carry_over_max: if *code == "AL" {
                    dec!(5)
                } else {
                    Decimal::ZERO
                },
                needs_approval: true,
                position: i as i32,
            },
        )
        .await?;
        type_ids.push(t.id);
    }
    for staff_id in &teacher_ids {
        for t in &type_ids {
            leave::set_balance(
                tx,
                hlc,
                actor,
                school,
                year.id,
                &leave::BalanceInput {
                    staff_id: *staff_id,
                    leave_type_id: *t,
                    entitlement: None,
                    carried_over: Decimal::ZERO,
                    adjustment: Decimal::ZERO,
                    note: String::new(),
                },
            )
            .await?;
        }
    }
    for (i, staff_id) in teacher_ids.iter().take(6).enumerate() {
        let from = april + chrono::Duration::days(7 + i as i64 * 5);
        let to = from + chrono::Duration::days((i % 2) as i64);
        let r = leave::create_request(
            tx,
            hlc,
            actor,
            school,
            &leave::NewRequest {
                staff_id: *staff_id,
                leave_type_id: type_ids[i % 2],
                from_date: from,
                to_date: to,
                half_day: None,
                reason: [
                    "Family wedding",
                    "Fever",
                    "Travel",
                    "Doctor's appointment",
                    "Personal",
                    "Exam duty elsewhere",
                ][i]
                    .into(),
                attachment_note: String::new(),
                requested_by: ctx.principal_user,
            },
        )
        .await?;
        if i < 4 {
            leave::decide_request(tx, hlc, actor, r.id, i != 3, "", ctx.principal_user).await?;
        }
        report.leave_requests += 1;
    }
    for (i, student_id) in students_of(ctx, "9-A").iter().take(3).enumerate() {
        let from = april + chrono::Duration::days(10 + i as i64 * 3);
        let r = leave::create_student_request(
            tx,
            hlc,
            actor,
            school,
            &leave::NewStudentRequest {
                student_id: *student_id,
                from_date: from,
                to_date: from + chrono::Duration::days(1),
                reason: "Family event".into(),
                guardian_id: None,
                recorded_by: ctx.principal_user,
            },
        )
        .await?;
        leave::decide_student_request(tx, hlc, actor, r.id, true, "", ctx.principal_user).await?;
    }

    // ---- Admissions: the default stages, a fee head and applications along the pipeline.
    let stages = admissions::ensure_stages(tx, hlc, actor, school).await?;
    settings::set(
        tx,
        hlc,
        actor,
        Some(school),
        admissions::KEY_FEE_HEAD,
        json!(ctx.admission_head.to_string()),
    )
    .await?;
    let stage = |code: &str| stages.iter().find(|s| s.code == code).map(|s| s.id);
    let applicants = [
        ("Hamza Siddiqui", "Imran Siddiqui", "male", 6u8, "enquiry"),
        ("Ayesha Rauf", "Rauf Ahmed", "female", 6, "applied"),
        ("Bilal Tariq", "Tariq Mehmood", "male", 7, "applied"),
        ("Fatima Noor", "Noor Alam", "female", 1, "test_scheduled"),
        ("Zain Abbas", "Abbas Ali", "male", 9, "test_scheduled"),
        ("Hira Shah", "Shah Nawaz", "female", 9, "interviewed"),
        ("Usman Ghani", "Ghani Khan", "male", 6, "interviewed"),
        ("Maryam Asif", "Asif Iqbal", "female", 1, "offered"),
        ("Saad Rehman", "Rehman Gul", "male", 7, "offered"),
        ("Laiba Farooq", "Farooq Azam", "female", 6, "admitted"),
        ("Daniyal Yousuf", "Yousuf Raza", "male", 0, "rejected"),
        ("Rida Anwar", "Anwar Hussain", "female", 1, "withdrawn"),
    ];
    for (i, (name, father, gender, grade, stage_code)) in applicants.iter().enumerate() {
        let enquiry = april - chrono::Duration::days(60 - i as i64 * 4);
        let app = admissions::create(
            tx,
            hlc,
            actor,
            school,
            &admissions::ApplicationInput {
                academic_year_id: year.id,
                class_id: ctx.class_ids.get(grade).copied(),
                enquiry_date: enquiry,
                source: ["Walk-in", "Phone", "Website", "Referral"][i % 4].into(),
                full_name: (*name).into(),
                father_name: (*father).into(),
                date_of_birth: NaiveDate::from_ymd_opt(
                    april.year() - 5 - i32::from(*grade),
                    3,
                    1 + i as u32,
                ),
                gender: (*gender).into(),
                cnic: None,
                address: "Gulshan-e-Iqbal, Karachi".into(),
                phone: Some(format!("0300 55{:05}", 1000 + i)),
                email: None,
                previous_school: if *grade > 1 {
                    "City Public School".into()
                } else {
                    String::new()
                },
                guardian_name: (*father).into(),
                guardian_relationship: "father".into(),
                guardian_phone: Some(format!("0300 55{:05}", 1000 + i)),
                guardian_cnic: None,
                guardian_email: None,
                guardian_occupation: "Business".into(),
                test_date: (i >= 3).then(|| enquiry + chrono::Duration::days(7)),
                test_marks: (i >= 5).then(|| Decimal::from(60 + (i * 7) % 35)),
                test_total: (i >= 5).then(|| dec!(100)),
                interview_date: (i >= 5).then(|| enquiry + chrono::Duration::days(12)),
                interview_notes: if i >= 5 {
                    "Confident; reads well.".into()
                } else {
                    String::new()
                },
                offer_date: (i >= 7).then(|| enquiry + chrono::Duration::days(15)),
                offer_fee: (i >= 7).then(|| dec!(5000)),
                offer_expires: (i >= 7).then(|| enquiry + chrono::Duration::days(30)),
                notes: String::new(),
                custom: json!({ "demo": true }),
            },
            stage("enquiry"),
            Some(ctx.principal_user),
        )
        .await?;
        if *stage_code != "enquiry" {
            if let Some(to) = stage(stage_code) {
                admissions::move_stage(tx, hlc, actor, app.id, to, "", Some(ctx.principal_user))
                    .await?;
            }
        }
        report.applications += 1;
    }

    // ---- Library: rules, books with copies, loans (some overdue).
    library::set_rules(
        tx,
        hlc,
        actor,
        school,
        &madrasa_core::library::LibraryRules::default(),
    )
    .await?;
    let books = [
        ("Physics for Class 9", "Sindh Textbook Board", "Science", 4),
        (
            "Chemistry for Class 9",
            "Sindh Textbook Board",
            "Science",
            4,
        ),
        ("Biology for Class 10", "Sindh Textbook Board", "Science", 3),
        ("Mathematics 8", "Oxford", "Mathematics", 4),
        ("Mathematics 10", "Oxford", "Mathematics", 3),
        ("English Reader 7", "Oxford", "English", 4),
        ("Urdu Qawaid", "Ferozsons", "Urdu", 3),
        (
            "Pakistan Studies",
            "Punjab Textbook Board",
            "Social studies",
            3,
        ),
        ("Islamiat Class 8", "Sindh Textbook Board", "Islamiat", 3),
        ("Computer Science 9", "Punjab Textbook Board", "Computer", 3),
        ("Atlas of Pakistan", "Survey of Pakistan", "Reference", 2),
        ("Oxford Dictionary", "Oxford", "Reference", 2),
        ("Alif Laila", "Ferozsons", "Stories", 3),
        ("Treasure Island", "Penguin", "Stories", 3),
        ("The Little Prince", "Penguin", "Stories", 2),
        ("Science Encyclopedia", "DK", "Reference", 2),
        ("Mathematics Puzzles", "Puffin", "Mathematics", 2),
        ("Seerat un Nabi", "Dar-ul-Ishaat", "Islamiat", 2),
        ("Urdu Nazam Ki Kitab", "Ferozsons", "Urdu", 2),
        ("World History", "Oxford", "Social studies", 2),
    ];
    let mut copy_ids = Vec::new();
    for (i, (title, publisher, category, copies)) in books.iter().enumerate() {
        let b = library::create_book(
            tx,
            hlc,
            actor,
            school,
            &library::BookInput {
                title: (*title).into(),
                author: String::new(),
                publisher: (*publisher).into(),
                isbn: String::new(),
                category: (*category).into(),
                language: if *category == "Urdu" || *category == "Islamiat" {
                    "Urdu".into()
                } else {
                    "English".into()
                },
                subject: String::new(),
                year: None,
                custom: json!({ "demo": true }),
            },
        )
        .await?;
        let nos: Vec<String> = (1..=*copies)
            .map(|c| format!("DEMO-{:03}-{c}", i + 1))
            .collect();
        let created = library::add_copies(
            tx,
            hlc,
            actor,
            school,
            b.id,
            &nos,
            &format!("S{}", i % 5 + 1),
        )
        .await?;
        copy_ids.extend(created.into_iter().map(|c| c.id));
        report.books += 1;
    }
    let borrowers: Vec<Uuid> = students_of(ctx, "9-A")
        .into_iter()
        .chain(students_of(ctx, "8-A"))
        .chain(students_of(ctx, "10-A"))
        .take(12)
        .collect();
    for (i, student_id) in borrowers.iter().enumerate() {
        let Some(copy_id) = copy_ids.get(i * 2) else {
            break;
        };
        let issued_on = april + chrono::Duration::days(2 + i as i64 * 3);
        library::issue(
            tx,
            hlc,
            actor,
            school,
            *copy_id,
            MemberKind::Student,
            *student_id,
            issued_on,
            ctx.principal_user,
        )
        .await?;
        report.loans += 1;
    }
    for (i, staff_id) in teacher_ids.iter().take(3).enumerate() {
        if let Some(copy_id) = copy_ids.get(30 + i) {
            library::issue(
                tx,
                hlc,
                actor,
                school,
                *copy_id,
                MemberKind::Staff,
                *staff_id,
                april + chrono::Duration::days(20),
                ctx.principal_user,
            )
            .await?;
            report.loans += 1;
        }
    }

    // ---- Transport: a fee head, two vehicles, two routes with stops, students assigned.
    let transport_head =
        fees::create_head(tx, hlc, actor, school, "Transport", "TRP", true, 3).await?;
    transport::set_fee_head(tx, hlc, actor, school, Some(transport_head.id)).await?;
    let mut vehicle_ids = Vec::new();
    for (reg, model, cap, driver) in [
        ("KHI-2041", "Toyota Hiace", 18, "Rashid Khan"),
        ("KHI-7719", "Toyota Coaster", 28, "Saleem Baig"),
    ] {
        let v = transport::create_vehicle(
            tx,
            hlc,
            actor,
            school,
            &transport::VehicleInput {
                registration_no: reg.into(),
                make_model: model.into(),
                capacity: Some(cap),
                driver_name: driver.into(),
                driver_phone: "0300 1234567".into(),
                helper_name: "Helper".into(),
                helper_phone: String::new(),
                active: true,
                notes: String::new(),
                custom: json!({ "demo": true }),
            },
        )
        .await?;
        vehicle_ids.push(v.id);
    }
    let routes = [
        (
            "North route",
            "N1",
            1500,
            &["Gulshan Chowrangi", "NIPA", "Sohrab Goth"][..],
        ),
        (
            "South route",
            "S1",
            1800,
            &["Clifton", "Defence Phase 2", "Korangi"][..],
        ),
    ];
    let mut stop_ids = Vec::new();
    for (i, (name, code, fee, stops)) in routes.iter().enumerate() {
        let r = transport::create_route(
            tx,
            hlc,
            actor,
            school,
            &transport::RouteInput {
                name: (*name).into(),
                code: (*code).into(),
                vehicle_id: vehicle_ids.get(i).copied(),
                monthly_fee: Decimal::from(*fee),
                active: true,
                notes: String::new(),
                stops: stops
                    .iter()
                    .enumerate()
                    .map(|(j, s)| transport::StopInput {
                        id: None,
                        name: (*s).into(),
                        pickup_time: chrono::NaiveTime::from_hms_opt(7, j as u32 * 15, 0),
                        drop_time: chrono::NaiveTime::from_hms_opt(13, 30 + j as u32 * 10, 0),
                        monthly_fee: (j == 2).then(|| Decimal::from(*fee - 300)),
                    })
                    .collect(),
            },
        )
        .await?;
        stop_ids.extend(
            transport::stops_of(tx, r.route.id)
                .await?
                .into_iter()
                .map(|s| s.id),
        );
        report.routes += 1;
    }
    let riders: Vec<Uuid> = ["6-A", "7-A", "8-A", "9-A", "10-A", "5-A"]
        .iter()
        .flat_map(|c| students_of(ctx, c).into_iter().take(6))
        .collect();
    for (i, student_id) in riders.iter().enumerate() {
        let Some(stop_id) = stop_ids.get(i % stop_ids.len().max(1)) else {
            break;
        };
        transport::assign(
            tx,
            hlc,
            actor,
            school,
            &transport::AssignInput {
                student_id: *student_id,
                academic_year_id: year.id,
                stop_id: *stop_id,
                from_date: april,
                amount: None,
                notes: String::new(),
            },
        )
        .await?;
        report.transport_students += 1;
    }

    // ---- Hostel: one hostel with a fee head, rooms, residents, a register.
    let hostel_head = fees::create_head(tx, hlc, actor, school, "Hostel", "HST", true, 4).await?;
    let h = hostel::create_hostel(
        tx,
        hlc,
        actor,
        school,
        &hostel::HostelInput {
            name: "Iqbal House".into(),
            kind: "boys".into(),
            warden_name: "Mr Rashid Ali".into(),
            warden_phone: "0300 7654321".into(),
            monthly_fee: dec!(8000),
            fee_head_id: Some(hostel_head.id),
            requires_consent: true,
            active: true,
            notes: String::new(),
            custom: json!({ "demo": true }),
        },
    )
    .await?;
    let mut rooms = Vec::new();
    for i in 1..=6 {
        let r = hostel::create_room(
            tx,
            hlc,
            actor,
            h.hostel.id,
            &hostel::RoomInput {
                floor: if i <= 3 {
                    "Ground".into()
                } else {
                    "First".into()
                },
                name: format!("{}-{}", if i <= 3 { "G" } else { "F" }, (i - 1) % 3 + 1),
                beds: 4,
                monthly_fee: (i > 3).then(|| dec!(9000)),
                active: true,
                notes: String::new(),
            },
        )
        .await?;
        rooms.push(r.room.id);
    }
    let mut residents: Vec<Uuid> = ["9-A", "10-A", "8-A"]
        .iter()
        .flat_map(|c| {
            boys_of(ctx, c)
                .into_iter()
                .filter(|id| !riders.contains(id))
                .take(5)
        })
        .collect();
    if residents.len() < 4 {
        // A small demo: fill the beds from the boys who are left (then from boys who ride).
        let mut rest: Vec<Uuid> = students_matching(ctx, |s| s.gender == 'M')
            .into_iter()
            .filter(|id| !residents.contains(id) && !riders.contains(id))
            .collect();
        if rest.is_empty() {
            rest = students_matching(ctx, |s| s.gender == 'M');
        }
        let room_left = 15usize.saturating_sub(residents.len());
        let extra: Vec<Uuid> = rest
            .into_iter()
            .filter(|id| !residents.contains(id))
            .take(room_left)
            .collect();
        residents.extend(extra);
    }
    for (i, student_id) in residents.iter().enumerate() {
        let room = rooms[i / 4 % rooms.len()];
        hostel::allocate(
            tx,
            hlc,
            actor,
            school,
            &hostel::AllocateInput {
                student_id: *student_id,
                academic_year_id: year.id,
                room_id: room,
                bed_no: (i % 4) as i32 + 1,
                from_date: april,
                amount: None,
                notes: String::new(),
            },
        )
        .await?;
        report.hostel_residents += 1;
    }
    if let Some(first) = residents.first() {
        let out_at = april.and_hms_opt(9, 0, 0).unwrap().and_utc() + chrono::Duration::days(20);
        hostel::open_register(
            tx,
            hlc,
            actor,
            school,
            &hostel::RegisterInput {
                student_id: *first,
                kind: "leave_out".into(),
                started_at: Some(out_at),
                expected_back: Some(out_at + chrono::Duration::days(2)),
                visitor_name: String::new(),
                visitor_relationship: String::new(),
                visitor_phone: String::new(),
                guardian_consent: true,
                consent_note: "Father phoned the warden".into(),
                notes: String::new(),
            },
            out_at,
        )
        .await?;
    }

    // ---- Inventory: categories, locations, items with stock, a few issues and assets.
    let mut categories = HashMap::new();
    for (i, name) in ["Stationery", "Lab supplies", "Furniture", "Electronics"]
        .iter()
        .enumerate()
    {
        let c = inventory::save_category(
            tx,
            hlc,
            actor,
            school,
            None,
            &inventory::CategoryInput {
                name: (*name).into(),
                position: i as i32,
            },
        )
        .await?;
        categories.insert(*name, c.id);
    }
    let mut locations = Vec::new();
    for (name, kind) in [
        ("Main store", "store"),
        ("Science lab", "lab"),
        ("Staff room", "office"),
    ] {
        let l = inventory::save_location(
            tx,
            hlc,
            actor,
            school,
            None,
            &inventory::LocationInput {
                name: name.into(),
                kind: kind.into(),
                active: true,
                notes: String::new(),
            },
        )
        .await?;
        locations.push(l.id);
    }
    let items = [
        ("Chalk (box)", "Stationery", "box", 10, 8, 120),
        ("Whiteboard marker", "Stationery", "pcs", 20, 150, 60),
        ("A4 paper (ream)", "Stationery", "ream", 10, 40, 950),
        (
            "Exam answer sheets (pack)",
            "Stationery",
            "pack",
            5,
            25,
            400,
        ),
        ("Test tubes", "Lab supplies", "pcs", 30, 200, 35),
        ("Litmus paper (book)", "Lab supplies", "book", 5, 12, 150),
        ("Student desk", "Furniture", "pcs", 0, 30, 4500),
        ("Plastic chair", "Furniture", "pcs", 10, 80, 900),
        ("Projector", "Electronics", "pcs", 0, 3, 85000),
        ("Printer toner", "Electronics", "pcs", 2, 1, 7500),
        ("Extension lead", "Electronics", "pcs", 2, 10, 1200),
        ("Floor cleaner (5L)", "Stationery", "can", 3, 8, 1300),
    ];
    let mut item_ids = Vec::new();
    for (i, (name, category, unit, reorder, qty, cost)) in items.iter().enumerate() {
        let it = inventory::save_item(
            tx,
            hlc,
            actor,
            school,
            None,
            &inventory::ItemInput {
                category_id: categories.get(category).copied(),
                name: (*name).into(),
                code: format!("IT-{:03}", i + 1),
                unit: (*unit).into(),
                reorder_level: (*reorder > 0).then(|| Decimal::from(*reorder)),
                is_asset: *name == "Projector",
                active: true,
                notes: String::new(),
                custom: json!({ "demo": true }),
            },
        )
        .await?;
        inventory::record(
            tx,
            hlc,
            actor,
            school,
            &inventory::MovementInput {
                item_id: it.item.id,
                location_id: locations[if *category == "Lab supplies" { 1 } else { 0 }],
                kind: MovementKind::PurchaseIn,
                quantity: Decimal::from(*qty),
                unit_cost: Some(Decimal::from(*cost)),
                moved_on: Some(april + chrono::Duration::days(1)),
                recipient_kind: None,
                recipient_id: None,
                recipient_name: String::new(),
                reference: format!("Bill {}", 100 + i),
                notes: String::new(),
            },
            april,
            None,
        )
        .await?;
        item_ids.push(it.item.id);
        report.inventory_items += 1;
    }
    for (i, staff_id) in teacher_ids.iter().take(4).enumerate() {
        let name = ctx
            .demo
            .staff
            .iter()
            .find(|m| ctx.staff_by_username.get(&m.username) == Some(staff_id))
            .map(|m| m.full_name.clone())
            .unwrap_or_default();
        inventory::record(
            tx,
            hlc,
            actor,
            school,
            &inventory::MovementInput {
                item_id: item_ids[1 + i % 3],
                location_id: locations[0],
                kind: MovementKind::Issue,
                quantity: Decimal::from(2 + i as i64),
                unit_cost: None,
                moved_on: Some(april + chrono::Duration::days(5 + i as i64)),
                recipient_kind: Some(RecipientKind::Staff),
                recipient_id: Some(*staff_id),
                recipient_name: name,
                reference: String::new(),
                notes: String::new(),
            },
            april,
            None,
        )
        .await?;
    }
    for n in 1..=3 {
        inventory::save_asset(
            tx,
            hlc,
            actor,
            school,
            None,
            &inventory::AssetInput {
                item_id: item_ids[8],
                tag: format!("PJ-{n:03}"),
                serial_no: format!("SN-DEMO-{n:04}"),
                location_id: Some(locations[if n == 1 { 1 } else { 0 }]),
                status: if n == 1 {
                    "in_use".into()
                } else {
                    "in_store".into()
                },
                assigned_kind: (n == 1).then_some(RecipientKind::Room),
                assigned_id: None,
                assigned_name: if n == 1 {
                    "Science lab".into()
                } else {
                    String::new()
                },
                purchased_on: Some(april - chrono::Duration::days(200)),
                cost: Some(dec!(85000)),
                notes: String::new(),
            },
        )
        .await?;
    }

    // ---- A read-only API key with a documented secret.
    let key = api_keys::create_with_secret(
        tx,
        hlc,
        actor,
        school,
        &api_keys::ApiKeyInput {
            name: "Demo portal".into(),
            scopes: Scope::ALL.iter().map(|s| s.as_str().to_string()).collect(),
            ip_allow: Vec::new(),
            expires_at: None,
        },
        DEMO_API_KEY,
    )
    .await?;
    report.api_key_prefix = key.key_prefix;
    Ok(report)
}
