# Milestone M2 — Lite complete

Goal: Madrasa Lite becomes the complete single-school product. M1 gave the pilot school its
daily work (students, fees, attendance, exams, backup, import). M2 adds everything the
office still does outside the app: a library of printable reports, a report builder for the
questions nobody anticipated, examination-board results, a term and component structure for
results, per-student subject allocation, allocation screens for teachers and sections, and a
consent register.

Scope and data model: docs/spec.md §14 (M2 overview) and the per-stage sections it points to.
Pilot school: K-12, ~1,200 students, ~120 staff (docs/spec.md "Pilot school profile").

## How to work

- Other sessions work on this repository at the same time. **Start every stage from the
  latest `main`** (`git fetch origin && git checkout -b dev/m2-stage-N-name origin/main`),
  use the **next free migration number after pulling `main`**, and **rebase on `main`
  before opening the PR**.
- One stage at a time, in order. One branch and one pull request per stage. When the PR is
  open and CI is green, merge it into `main` and continue with the next stage.
- Do not change how Lite works except as a stage requires. Everything existing keeps
  working: current exam setups, report cards, imports and permissions.
- Follow every rule in CLAUDE.md (UUID v7, `school_id`, sync columns, soft delete, audit
  log, money as exact decimals, backend permissions, en/ur i18n with RTL, tests before
  ticking). Use the demo data generator for all testing; never real data.
- Tick `[x]` only when built and tested. Keep docs/PROGRESS.md updated after each stage;
  log blockers in docs/BLOCKERS.md and move on rather than guess.
- Every report and printout: English and Urdu, light theme, works on A4 portrait or
  landscape, school name and date in the header, page numbers in the footer.

---

## Stage 1 — Report engine and student reports

The foundation every later stage builds on: one way to describe a report, three ways to
render it (on screen, PDF, Excel), one page to find and run them.

- [x] `madrasa_api::reports`: a `Report` document model (title, subtitle, school header,
      parameters shown, columns with kind and alignment, rows, group headings, subtotals and
      totals, notes, orientation) built by each report from the database
- [x] Renderers: JSON (on-screen table), Excel (`.xlsx` via the existing export module) and
      PDF (server-side, typeset with embedded Inter and Noto Nastaliq Urdu fonts so Urdu is
      shaped and right-to-left; no browser print dialog needed)
- [x] Report library API: `GET /reports` (catalogue with each report's parameters), `GET
      /reports/{code}?format=json|pdf|xlsx&lang=en|ur&…params` (`reports.read`); every run is
      audit-logged with its parameters
- [x] Report headings, column labels and fixed texts in English and Urdu from one backend
      string table (`crates/api/src/reports/text.rs`); the UI passes the current language
- [x] Reports page: library grouped by module, parameter form per report (section, class,
      date range, month, assessment, status…), on-screen preview, Excel and PDF buttons,
      print-ready view
- [x] Student reports: student list (by class, section, status, with guardian phone),
      class strength (sections, boys/girls, totals), admissions and withdrawals in a period,
      guardian contact list, birthdays by month, missing-data check (no CNIC/B-Form, no
      photo, no guardian phone, no roll number)
- [x] Tests: report model → PDF (page count, text present, Urdu shaped) and → Excel; each
      student report against the demo school (figures match direct queries); permission test
      (teacher gets 403; accountant and admin get the report); vitest for the library page
      and parameter form

## Stage 2 — Fees, attendance and exams reports

- [x] Fees: daily collection (by method and by fee head), monthly collection summary,
      defaulters by class with ageing buckets (1–30, 31–60, 61–90, 90+ days), student fee
      ledger (one student, a date range), concessions register, voucher register for a month,
      late fees assessed and waived, head-wise billed vs collected for the year
- [x] Attendance: monthly register per section (landscape, one column per day, P/A/L/Lv
      marks and totals), attendance percentage by class and section for a range, absentees for
      a day or range, students below a percentage threshold, staff attendance for a month
- [x] Exams: result sheet per section and assessment, merit list (top N per class), subject
      analysis (average, highest, lowest, pass percentage), grade distribution per class,
      students failing per subject, report cards for a whole section as one PDF (one page per
      student, same content as the on-screen card)
- [x] Teachers with `exams.read` or `attendance.read` run the exam and attendance reports for
      their own sections only; office roles run everything with `reports.read`
- [x] Dashboard "Reports" shortcuts for today's collection and today's absentees
- [x] Tests: every report against the demo school with figures checked against the module's
      own endpoints (collection equals receipts, defaulters equals the defaulter list,
      register equals the sheets, result sheet equals the exams result endpoint); PDF of a
      section's report cards has one page per student; teacher scope enforced

## Stage 3 — Custom report builder

- [x] Data sources with a fixed field catalogue (students and guardians, enrolments, fee
      vouchers and receipts, fee balances, attendance entries and monthly summaries, marks and
      results, staff): each field has a type (text, number, money, date, boolean, choice), a
      label in both languages and, where it applies, the related entity it comes from
- [x] Query assembly from the catalogue only: columns, filters (operators per type: equals,
      contains, in, between, before/after, is empty), sort, grouping with counts and
      subtotals, row limit; everything parameterised, nothing from the user reaches SQL as
      text; teacher scope applied when a teacher runs a report
- [x] `report_definitions` table (name, description, source, definition JSONB, owner,
      shared flag, sync columns); `GET/POST/PUT/DELETE /reports/custom`, `POST
      /reports/custom/{id}/run` (preview with paging) and `?format=pdf|xlsx`; create, change
      and delete audit-logged; `reports.read` runs, `reports.manage` (new permission, default
      principal and admin) saves shared reports
- [x] Builder UI: choose a source, tick and order columns, add filter rows, sort, group,
      preview as you build, save with a name, run from the library like a built-in report
- [x] Tests: catalogue-only assembly (a field or operator outside the catalogue is refused),
      filters and grouping produce the expected rows on the demo school, teacher scope,
      permission split read/manage, builder vitest

## Stage 4 — Board Creator and external board result import

Pakistani schools send classes 9–12 to an examination board (BISE, Federal Board, AKU-EB
and others); the school keeps the board's results next to its own. The same model covers
any external examination (Cambridge, a provincial assessment).

- [x] Boards (`exam_boards`): name, short code, country, grading mode (marks, grades or
      both), default grade bands, pass percentage; created and edited under Examinations →
      Boards (`academics.manage`)
- [x] Board exams (`board_exams`): per board and academic year: name (e.g. "SSC Part I
      Annual 2026"), the classes that sit it, the subjects with maximum marks (board subject
      names may differ from the school's; each is matched to a school subject or kept as a
      board-only subject), status (announced, results imported, closed)
- [x] Candidates (`board_candidates`): students registered for a board exam with roll number,
      registration number and group (Science, Commerce, Arts…); bulk registration from a
      class or section; roll and registration numbers pre-filled from the student record
- [x] Results (`board_results`): per candidate and subject: obtained, maximum, grade, status
      (pass, fail, absent, withheld) and remarks; totals, percentage, overall grade and
      position computed from the board's rules
- [x] Import of board results from Excel/CSV through the import wizard engine: both layouts
      (one row per candidate with a column per subject; one row per candidate and subject),
      matched by roll number (then registration number), column mapping saved per board,
      preview with row errors (unknown roll, subject not in the exam, marks over maximum),
      commit as a batch with rollback; audit-logged
- [x] Reports (library): board result sheet per exam and class, board merit list, pass
      percentage by subject and by section, student board result card; the student profile's
      Results tab shows board results under the school's own
- [x] Demo data: one board with two exams and results for classes 9–10
- [x] Tests: board rules (totals, grades, pass/fail, positions) in `madrasa_core`; import of
      both layouts with errors and rollback; permission tests; vitest for the Boards tab and
      the import flow

## Stage 5 — Terms and assessment components

Academic year → terms → assessment components per term. Today a year has flat assessment
types with weights; schools work in terms (two or three per year) with several components
each (class test, assignment, mid-term, final) and calculated columns.

- [ ] `terms` per academic year: name, position, start and end dates, weight in the year;
      weights of a year's terms total 100
- [ ] Components: each existing assessment type becomes a component of a term; a component
      is **entered** (marks typed in or uploaded, has maximum marks per class and subject,
      has mark sheets) or **calculated** (formula over other components of the same term:
      sum, average, weighted average, best of N; no mark sheets); weights within a term
      total 100 over the components that count
- [ ] Calculation in `madrasa_core::exams` with exact decimals: component → term result →
      year result; a component not yet held is left out of averages as today; cycles and
      references outside the term are refused
- [ ] Results, result sheets, report cards and the exam reports follow the structure: term
      columns with their components, term totals and grades, year result
- [ ] Migration: an existing year gets one term ("Year") holding its current assessment
      types as entered components with their weights, so every existing setup, mark sheet,
      result and report card stays identical; the default for a new year is two terms with
      the current four assessments split between them
- [ ] Setup UI under Examinations → Setup: terms, components per term, formula editor for
      calculated components (pick the operation and the inputs), weights with live totals
- [ ] Tests: formulas (each operation, best-of-N, missing inputs, rounding), migration on a
      demo year (results before and after identical), report cards with terms, setup vitest

## Stage 6 — Subject allocation per student

- [ ] Subject groups per class (e.g. Science, Commerce, Arts for grades 9–12): a named set of
      subjects; compulsory subjects per class; electives chosen per student
- [ ] `student_subjects` per enrolment: the subjects a student takes this year; defaults to
      every class subject when nothing is allocated, so existing schools see no change
- [ ] Allocation UI on the student profile and in bulk per section (pick a group, tick
      electives); import of allocations from Excel; audit-logged
- [ ] Marks entry rosters, Excel marks templates, result sheets, report cards, merit lists
      and the exam reports show only a student's own subjects; a subject nobody takes in a
      section has no sheet
- [ ] Tests: default-to-all when nothing is allocated, roster and report card filtering,
      result totals over own subjects only, import of allocations, vitest

## Stage 7 — Allocation screens

- [ ] Teacher allocation grid: sections down, subjects across, a cell is the teacher (or
      empty); class teacher per section in the first column; filter by class; changes save
      one cell at a time and are audit-logged; a teacher's load (sections × subjects) shown
      per teacher
- [ ] Bulk student moves: pick a class, see its sections with sizes and gender split, tick
      students and move them to another section (roll numbers reassigned or kept, by
      choice); "balance sections" proposes moves so sizes differ by at most one, shown as a
      preview before applying; every move audit-logged; attendance and marks already
      recorded stay with the student
- [ ] Tests: balancing proposal (sizes, stability, no move of excluded students), moves
      with roll-number options, audit entries, permission (`academics.manage`,
      `students.manage`), vitest for both screens

## Stage 8 — Consent manager

- [ ] Consent types defined by the school: name, description, who gives it (guardian,
      student, either), required for all or for a class range, renewal (none, yearly)
- [ ] Consents recorded per student: type, status (given, refused, withdrawn), the guardian
      who gave it, date, who recorded it, note; history kept (a new status is a new row, the
      old one stays); shown on the student profile; bulk recording per section after a form
      is collected
- [ ] Report (library): missing consents by section and type; consents expiring with the
      year; export of a consent form list for a section
- [ ] Permissions: `students.read` sees consents, `students.manage` records them; every
      change audit-logged
- [ ] Tests: required-for-class rule, renewal, missing-consent report against the demo
      school, permissions, vitest

## Stage 9 — M2 close-out

- [ ] Demo data covers every M2 feature (terms, groups, boards, consents, saved reports)
- [ ] User guide (en/ur), TESTING.md tasks for every stage, spec sections checked against
      the code, permissions matrix updated
- [ ] docs/PROGRESS.md: what to test by hand and the decisions left for the owner

## Done when

- The office can print every routine list and summary from the Reports page in English or
  Urdu without Excel gymnastics, and build its own report for anything else.
- Board results live next to school results and appear on the student's record and reports.
- Results follow terms and components, students take only their own subjects, and the
  allocation and consent screens replace the paper registers.
- Every stage's tests pass in CI, and nothing that worked in M1 behaves differently unless a
  stage says so.
