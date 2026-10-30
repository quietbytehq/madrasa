/**
 * TypeScript mirrors of the Rust API DTOs (crates/api). Keep in sync with the serde structs.
 */

export type Edition = "lite" | "standard" | "nexus";

/** Feature codes; must match madrasa_core::edition::Feature. */
export type Feature =
  | "dashboard"
  | "students"
  | "attendance"
  | "examinations"
  | "fees"
  | "teachers"
  | "reports"
  | "backup"
  | "settings"
  | "campuses"
  | "transport"
  | "library"
  | "hostel"
  | "hr_payroll"
  | "parents_portal"
  | "nexus_hub";

export interface FeaturesResponse {
  edition: Edition;
  features: Feature[];
}

export interface HealthResponse {
  status: "ok";
  version: string;
  database: "ok" | "error";
}

export interface SetupStatus {
  needs_setup: boolean;
}

export interface SetupRequest {
  school_name: string;
  school_code: string;
  city: string;
  academic_year_start_month: 4 | 8;
  currency: "PKR";
  admin_full_name: string;
  admin_username: string;
  admin_password: string;
  backup_password: string;
}

export interface SetupResponse {
  school_id: string;
  admin_user_id: string;
  /** Shown exactly once. */
  recovery_key: string;
}

export interface UserSummary {
  id: string;
  school_id: string;
  username: string;
  full_name: string;
  email: string | null;
  is_active: boolean;
  roles: string[];
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  expires_at: string;
  user: UserSummary;
}

export interface MeResponse {
  user: UserSummary;
  permissions: string[];
}

export interface SchoolSummary {
  id: string;
  name: string;
  code: string;
  city: string;
  is_demo: boolean;
  academic_year_start_month: number;
  currency: string;
}

export interface ApiErrorBody {
  error: { code: string; message: string };
}

// ---- M1 stage 1 ---------------------------------------------------------------------------

export type Gender = "male" | "female" | "other";
export type StudentStatus = "active" | "left" | "graduated";
export type Relationship = "father" | "mother" | "guardian" | "other";

export interface AcademicYear {
  id: string;
  school_id: string;
  name: string;
  start_date: string;
  end_date: string;
  start_month: 4 | 8;
  is_active: boolean;
}

export interface SchoolClass {
  id: string;
  school_id: string;
  name: string;
  level: number;
}

export interface Subject {
  id: string;
  school_id: string;
  name: string;
  code: string;
}

export interface Section {
  id: string;
  school_id: string;
  class_id: string;
  academic_year_id: string;
  name: string;
  class_teacher_id: string | null;
  capacity: number | null;
  class_name: string;
  class_level: number;
  class_teacher_name: string | null;
  student_count: number;
}

export interface Staff {
  id: string;
  school_id: string;
  user_id: string | null;
  staff_number: string;
  full_name: string;
  father_name: string;
  gender: Gender;
  phone: string | null;
  email: string | null;
  cnic: string | null;
  designation: string;
  joined_on: string | null;
  status: "active" | "left";
  custom: Record<string, unknown>;
  roles: string[];
  username: string | null;
}

export interface Assignment {
  section_id: string;
  subject_id: string | null;
}

export interface StudentListRow {
  id: string;
  student_number: string;
  full_name: string;
  father_name: string;
  gender: Gender;
  date_of_birth: string;
  status: StudentStatus;
  admission_date: string;
  class_id: string | null;
  class_name: string | null;
  section_id: string | null;
  section_name: string | null;
  roll_no: number | null;
  has_photo: boolean;
}

export interface Page<T> {
  items: T[];
  page: number;
  page_size: number;
  total: number;
}

export interface GuardianLink {
  id: string;
  school_id: string;
  full_name: string;
  phone: string | null;
  email: string | null;
  cnic: string | null;
  occupation: string;
  address: string;
  relationship: Relationship;
  is_primary: boolean;
}

export interface EnrollmentView {
  id: string;
  student_id: string;
  academic_year_id: string;
  section_id: string;
  roll_no: number | null;
  academic_year_name: string;
  class_id: string;
  class_name: string;
  section_name: string;
}

export interface StudentDetail {
  id: string;
  school_id: string;
  student_number: string;
  legacy_id: string | null;
  full_name: string;
  father_name: string;
  date_of_birth: string;
  gender: Gender;
  cnic: string | null;
  address: string;
  admission_date: string;
  status: StudentStatus;
  left_on: string | null;
  left_reason: string | null;
  board_roll_no: string | null;
  board_registration_no: string | null;
  has_photo: boolean;
  custom: Record<string, unknown>;
  guardians: GuardianLink[];
  enrollments: EnrollmentView[];
  current: EnrollmentView | null;
  siblings: StudentListRow[];
}

export interface GuardianInput {
  guardian_id?: string;
  full_name?: string;
  relationship: Relationship;
  phone?: string;
  email?: string;
  cnic?: string;
  occupation?: string;
  address?: string;
  is_primary?: boolean;
}

export interface NewStudentRequest {
  full_name: string;
  father_name: string;
  date_of_birth: string;
  gender: Gender;
  cnic?: string;
  address?: string;
  admission_date?: string;
  student_number?: string;
  legacy_id?: string;
  board_roll_no?: string;
  board_registration_no?: string;
  section_id?: string;
  roll_no?: number;
  custom?: Record<string, unknown>;
  guardians?: GuardianInput[];
  force?: boolean;
}

export interface StudentPatch {
  full_name?: string;
  father_name?: string;
  date_of_birth?: string;
  gender?: Gender;
  cnic?: string | null;
  address?: string;
  admission_date?: string;
  legacy_id?: string | null;
  board_roll_no?: string | null;
  board_registration_no?: string | null;
  custom?: Record<string, unknown>;
}

export interface DuplicateMatch {
  student: StudentDetail;
  reason: "same_cnic" | "same_name_dob_father";
}

export interface GuardianView {
  id: string;
  school_id: string;
  full_name: string;
  phone: string | null;
  email: string | null;
  cnic: string | null;
  occupation: string;
  address: string;
  students: StudentListRow[];
}

export interface StudentListQuery {
  q?: string;
  class_id?: string;
  section_id?: string;
  status?: StudentStatus | "";
  sort?: "name" | "number" | "class" | "admission_date";
  order?: "asc" | "desc";
  page?: number;
  page_size?: number;
}

export interface SchoolSettings {
  student_number_prefix: string;
}

// ---- M1 stage 2: fees ----------------------------------------------------------------------

/** Money amounts are decimal strings (e.g. "2500.00"); zero may arrive as "0". */
export type Money = string;
export type PaymentMethod = "cash" | "bank" | "online" | "one_bill" | "kuickpay";
export type VoucherStatus = "unpaid" | "partial" | "paid" | "carried" | "cancelled";
export type ConcessionKind = "staff_child" | "scholarship" | "custom";

export interface FeePolicy {
  sibling_discount_percent: Money;
  sibling_rule: "all" | "second_child_onwards";
  /** What a percent concession is a percentage of. Default "gross". */
  concession_base: ConcessionBase;
  late_fee_percent: Money;
  due_day: number;
}

export type ConcessionBase = "gross" | "after_sibling_discount";

export interface FeeHead {
  id: string;
  school_id: string;
  name: string;
  code: string;
  is_recurring: boolean;
  position: number;
}

export interface FeeStructureLine {
  id: string;
  academic_year_id: string;
  class_id: string;
  fee_head_id: string;
  amount: Money;
}

export interface FeeConcession {
  id: string;
  student_id: string;
  kind: ConcessionKind;
  percent: Money | null;
  amount: Money | null;
  reason: string;
  approved_by: string;
  approved_at: string;
  valid_from: string;
  valid_to: string | null;
}

export interface FeeVoucher {
  id: string;
  student_id: string;
  academic_year_id: string;
  section_id: string | null;
  voucher_no: string;
  billing_month: string;
  issue_date: string;
  due_date: string;
  gross: Money;
  sibling_discount: Money;
  concession_discount: Money;
  net_monthly: Money;
  arrears: Money;
  total_due: Money;
  late_fee: Money;
  late_fee_waived: boolean;
  waiver_reason: string | null;
  waived_by: string | null;
  net_paid: Money;
  status: VoucherStatus;
}

export interface FeeVoucherRow {
  id: string;
  voucher_no: string;
  billing_month: string;
  due_date: string;
  student_id: string;
  student_number: string;
  student_name: string;
  class_name: string | null;
  section_name: string | null;
  gross: Money;
  sibling_discount: Money;
  concession_discount: Money;
  arrears: Money;
  total_due: Money;
  late_fee: Money;
  late_fee_waived: boolean;
  net_paid: Money;
  status: VoucherStatus;
}

export interface FeeVoucherLine {
  id: string;
  fee_head_id: string | null;
  description: string;
  amount: Money;
  position: number;
}

export interface FeeReceipt {
  id: string;
  voucher_id: string;
  student_id: string;
  receipt_no: string;
  paid_on: string;
  amount: Money;
  method: PaymentMethod;
  reference: string | null;
  note: string | null;
  reverses_id: string | null;
  reversal_reason: string | null;
  received_by: string;
  created_at: string;
}

export interface SchoolHeader {
  name: string;
  code: string;
  city: string;
}

export interface VoucherDetail extends FeeVoucher {
  balance: Money;
  lines: FeeVoucherLine[];
  receipts: FeeReceipt[];
  student_name: string;
  student_number: string;
  father_name: string;
  class_name: string | null;
  section_name: string | null;
  guardian_phone: string | null;
  school: SchoolHeader;
  policy: FeePolicy;
}

export interface ReceiptDetail extends FeeReceipt {
  voucher_no: string;
  billing_month: string;
  student_name: string;
  student_number: string;
  father_name: string;
  class_name: string | null;
  section_name: string | null;
  received_by_name: string;
  reversed_by: FeeReceipt | null;
  voucher_balance_after: Money;
  school: SchoolHeader;
}

export interface LedgerTotals {
  billed: Money;
  discounts: Money;
  fines: Money;
  paid: Money;
  balance: Money;
}

export interface Ledger {
  student_id: string;
  totals: LedgerTotals;
  vouchers: Array<FeeVoucher & { balance: Money }>;
  receipts: FeeReceipt[];
  concessions: FeeConcession[];
  payable: string[];
}

export interface Defaulter {
  student_id: string;
  student_number: string;
  student_name: string;
  father_name: string;
  class_name: string | null;
  section_name: string | null;
  guardian_phone: string | null;
  oldest_due_date: string;
  days_overdue: number;
  unpaid_vouchers: number;
  balance: Money;
}

export interface Collections {
  date: string;
  total: Money;
  by_method: Array<{ method: PaymentMethod; receipts: number; amount: Money }>;
  receipts: ReceiptDetail[];
}

export interface GenerationReport {
  created: number;
  skipped_existing: number;
  skipped_no_structure: number;
  voucher_ids: string[];
}

// ---- M1 stage 3: attendance ----------------------------------------------------------------

export type AttendanceStatus = "present" | "absent" | "late" | "leave";
export type StaffStatus = "present" | "absent" | "leave";

export type DayKind = { kind: "school" } | { kind: "weekend" } | { kind: "holiday"; name: string };

export interface Holiday {
  id: string;
  school_id: string;
  name: string;
  starts_on: string;
  ends_on: string;
}

export interface CalendarView {
  weekend_days: number[];
  holidays: Holiday[];
}

export interface AttendanceSummary {
  present: number;
  absent: number;
  late: number;
  leave: number;
}

export interface AttendanceSheet {
  id: string;
  section_id: string;
  academic_year_id: string;
  date: string;
  marked_by: string | null;
  submitted_at: string;
  present: number;
  absent: number;
  late: number;
  leave: number;
}

export interface RosterRow {
  student_id: string;
  student_number: string;
  full_name: string;
  father_name: string;
  roll_no: number | null;
  status: AttendanceStatus | null;
  note: string | null;
}

export interface SectionHeader {
  id: string;
  class_id: string;
  class_name: string;
  section_name: string;
  class_teacher_name: string | null;
}

/** Mirrors `SheetView` in the API. */
export interface SheetView {
  section: SectionHeader;
  date: string;
  day_kind: DayKind;
  sheet: AttendanceSheet | null;
  marked_by_name: string | null;
  students: RosterRow[];
  can_edit: boolean;
  is_correction: boolean;
}

export interface SaveSheetRequest {
  section_id: string;
  date: string;
  /** Exceptions only; students not listed are present. */
  entries: Array<{ student_id: string; status: AttendanceStatus; note?: string }>;
}

export interface SheetHistoryEntry {
  at: string;
  actor_name: string | null;
  action: string;
  entity: string;
  student_name: string | null;
  old_value: Record<string, unknown> | null;
  new_value: Record<string, unknown> | null;
}

export interface SectionDayStatus {
  section_id: string;
  class_id: string;
  class_name: string;
  section_name: string;
  class_teacher_name: string | null;
  enrolled: number;
  sheet_id: string | null;
  submitted_at: string | null;
  present: number | null;
  absent: number | null;
  late: number | null;
  leave: number | null;
}

export interface StatusTotals {
  sections: number;
  marked: number;
  enrolled: number;
  present: number;
  absent: number;
  late: number;
  leave: number;
  percent: number | null;
}

export interface StatusView {
  date: string;
  day_kind: DayKind;
  sections: SectionDayStatus[];
  totals: StatusTotals;
}

export interface AbsenteeRow {
  student_id: string;
  student_number: string;
  full_name: string;
  father_name: string;
  class_name: string;
  section_id: string;
  section_name: string;
  roll_no: number | null;
  status: AttendanceStatus;
  note: string | null;
  guardian_phone: string | null;
}

export interface RegisterDay {
  date: string;
  kind: DayKind;
  marked: boolean;
}

export interface RegisterRow {
  student_id: string;
  student_number: string;
  full_name: string;
  roll_no: number | null;
  statuses: Array<AttendanceStatus | null>;
  summary: AttendanceSummary;
}

export interface RegisterView {
  section: SectionHeader;
  month: string;
  days: RegisterDay[];
  rows: RegisterRow[];
  school_days: number;
}

export interface StudentDayRow {
  date: string;
  status: AttendanceStatus;
  note: string | null;
  section_id: string;
}

export interface StudentAttendanceView {
  student_id: string;
  from: string;
  to: string;
  summary: AttendanceSummary;
  percent: number | null;
  days: StudentDayRow[];
}

export interface StaffDayRow {
  staff_id: string;
  staff_number: string;
  full_name: string;
  designation: string;
  status: StaffStatus | null;
  note: string | null;
}

export interface StaffDayView {
  date: string;
  day_kind: DayKind;
  rows: StaffDayRow[];
  can_edit: boolean;
}

export interface StaffHistoryView {
  staff_id: string;
  from: string;
  to: string;
  summary: { present: number; absent: number; leave: number };
  days: Array<{ id: string; date: string; status: StaffStatus; note: string | null }>;
}

export interface UnmarkedSection {
  section_id: string;
  class_name: string;
  section_name: string;
  class_teacher_name: string | null;
}

export interface DashboardView {
  date: string;
  day_kind: DayKind;
  attendance: { totals: StatusTotals; unmarked: UnmarkedSection[] } | null;
  backup: BackupStatus;
}

// ---- M1 stage 4: exams ---------------------------------------------------------------------

export type SheetStatus = "draft" | "submitted" | "approved";

export interface AssessmentType {
  id: string;
  school_id: string;
  academic_year_id: string;
  name: string;
  position: number;
  weight: Money;
}

export interface GradeBand {
  grade: string;
  min_percent: Money;
  remarks: string;
}

/** Mirrors `GradingScheme` in the API. */
export interface GradingScheme {
  bands: GradeBand[];
  pass_percent: Money;
}

export interface MaxMarksLine {
  subject_id: string;
  subject_name: string;
  subject_code: string;
  max_marks: Money | null;
}

export interface ExamSectionHeader {
  id: string;
  class_id: string;
  class_name: string;
  section_name: string;
  class_teacher_name: string | null;
}

export interface SheetSummary {
  subject_id: string;
  subject_name: string;
  subject_code: string;
  max_marks: Money | null;
  sheet_id: string | null;
  status: SheetStatus | null;
  submitted_at: string | null;
  approved_at: string | null;
  entered: number;
  students: number;
  can_enter: boolean;
}

export interface SheetsView {
  assessment: AssessmentType;
  section: ExamSectionHeader;
  sheets: SheetSummary[];
}

export interface MarkSheet {
  id: string;
  assessment_type_id: string;
  section_id: string;
  subject_id: string;
  status: SheetStatus;
  submitted_by: string | null;
  submitted_at: string | null;
  approved_by: string | null;
  approved_at: string | null;
}

export interface SheetStudent {
  student_id: string;
  student_number: string;
  full_name: string;
  roll_no: number | null;
  obtained: Money | null;
  is_absent: boolean;
}

/** Mirrors `MarksSheetView` in the API. */
export interface MarksSheetView {
  assessment: AssessmentType;
  section: ExamSectionHeader;
  subject_id: string;
  subject_name: string;
  max_marks: Money | null;
  sheet: MarkSheet | null;
  status: SheetStatus;
  submitted_by_name: string | null;
  approved_by_name: string | null;
  students: SheetStudent[];
  can_edit: boolean;
  can_submit: boolean;
  can_approve: boolean;
  can_reopen: boolean;
}

export interface SaveMarksRequest {
  assessment_id: string;
  section_id: string;
  subject_id: string;
  marks: Array<{ student_id: string; obtained: Money | null; is_absent: boolean }>;
}

export interface ResultSubject {
  subject_id: string;
  subject_name: string;
  max_marks: Money;
  sheet_status: SheetStatus;
}

export interface ResultMark {
  subject_id: string;
  obtained: Money | null;
  is_absent: boolean;
  percent: Money;
  grade: string;
}

export interface ResultRow {
  student_id: string;
  student_number: string;
  full_name: string;
  roll_no: number | null;
  marks: ResultMark[];
  total: Money;
  max: Money;
  percent: Money;
  grade: string;
  pass: boolean;
  position: number;
}

export interface SubjectAnalysis {
  subject_id: string;
  subject_name: string;
  max_marks: Money;
  appeared: number;
  absent: number;
  average_percent: Money | null;
  highest: Money | null;
  lowest: Money | null;
  passed: number;
  failed: number;
}

/** Mirrors `ResultSheet` in the API. */
export interface ResultSheet {
  assessment: AssessmentType;
  section: ExamSectionHeader;
  subjects: ResultSubject[];
  rows: ResultRow[];
  analysis: SubjectAnalysis[];
  grading: GradingScheme;
  complete: boolean;
}

export interface CardAssessment {
  id: string;
  name: string;
  weight: Money;
  held: boolean;
}

export interface CardCell {
  max_marks: Money | null;
  obtained: Money | null;
  is_absent: boolean;
  percent: Money | null;
  approved: boolean;
}

export interface CardSubject {
  subject_id: string;
  subject_name: string;
  cells: CardCell[];
  weighted_percent: Money | null;
  grade: string | null;
}

export interface CardAssessmentTotal {
  assessment_id: string;
  total: Money;
  max: Money;
  percent: Money | null;
  grade: string | null;
  position: number | null;
  students: number;
}

/** Mirrors `ReportCard` in the API. */
export interface ReportCard {
  school: { name: string; code: string; city: string };
  academic_year: string;
  student_id: string;
  student_number: string;
  full_name: string;
  father_name: string;
  class_name: string | null;
  section_name: string | null;
  roll_no: number | null;
  assessments: CardAssessment[];
  subjects: CardSubject[];
  totals: CardAssessmentTotal[];
  overall_percent: Money | null;
  overall_grade: string | null;
  overall_remarks: string | null;
  pass: boolean | null;
  attendance: { present: number; total: number; percent: number | null };
  grading: GradingScheme;
}

// ---- M1 stage 5: backup --------------------------------------------------------------------

export interface BackupSettings {
  enabled: boolean;
  /** Local time of day, HH:MM. */
  time: string;
  directory: string | null;
  keep: number;
}

export interface BackupRun {
  id: string;
  school_id: string;
  trigger: "manual" | "scheduled";
  status: "running" | "ok" | "failed";
  started_at: string;
  finished_at: string | null;
  file_path: string | null;
  file_size: number | null;
  error: string | null;
}

export interface BackupFile {
  path: string;
  name: string;
  size: number;
  modified: string;
}

export interface BackupMeta {
  created_at: string;
  app_version: string;
  school_id: string;
  school_name: string;
  school_code: string;
  migrations: number[];
  counts: Record<string, number>;
}

export interface RestorePreview {
  path: string;
  size: number;
  meta: BackupMeta;
  schema_matches: boolean;
  current_counts: Record<string, number>;
}

export interface RestoreResult {
  tables: number;
  rows: number;
  meta: BackupMeta;
  relogin: boolean;
}

/** Mirrors `BackupStatus` in the API. */
export interface BackupStatus {
  last_backup_at: string | null;
  last_backup_file: string | null;
  stale: boolean;
  enabled: boolean;
  time: string;
  directory: string;
  unattended_ready: boolean;
  last_error: string | null;
}

// ---- M1 stage 6: imports and export -------------------------------------------------------

export type ImportKind = "students" | "staff" | "opening_balances";
export type DuplicatePolicy = "skip" | "update" | "merge";

export interface ImportField {
  key: string;
  label: string;
  required: boolean;
  aliases: string[];
  example: string;
}

export interface ParsedFile {
  file_name: string;
  sheets: string[];
  sheet: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
  mapping_saved: boolean;
}

export interface SavedMapping {
  headers: string[];
  mapping: Record<string, number>;
}

export interface ImportRequest {
  kind: ImportKind;
  file_name: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
  options: { duplicates: DuplicatePolicy };
  save_mapping?: boolean;
}

export interface ImportIssue {
  field: string;
  message: string;
}

export type RowAction = "create" | "update" | "skip" | "error";

export interface RowOutcome {
  row_no: number;
  values: Record<string, string>;
  errors: ImportIssue[];
  warnings: ImportIssue[];
  action: RowAction;
  matched: string | null;
}

export interface ImportSummary {
  total: number;
  create: number;
  update: number;
  skip: number;
  error: number;
}

export interface ImportPreview {
  rows: RowOutcome[];
  summary: ImportSummary;
}

export interface ImportBatch {
  id: string;
  school_id: string;
  kind: ImportKind;
  file_name: string;
  status: "applied" | "rolled_back";
  options: Record<string, unknown>;
  total_rows: number;
  created_rows: number;
  updated_rows: number;
  skipped_rows: number;
  applied_by: string | null;
  applied_at: string;
  rolled_back_at: string | null;
}

export interface ImportResult {
  batch: ImportBatch;
  summary: ImportSummary;
  rows: RowOutcome[];
}
