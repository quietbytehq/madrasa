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
  | "hr"
  | "parents_portal"
  | "nexus_hub"
  | "timetable"
  | "leave"
  | "admissions"
  | "inventory"
  | "directory_login"
  | "readonly_api"
  | "nexus_link";

export interface EditionSettings {
  edition: Edition;
  available: Edition[];
  features: Feature[];
  standard_only: Feature[];
}

export interface FeaturesResponse {
  edition: Edition;
  features: Feature[];
  /** Release version, e.g. "0.1.3-alpha" on a tagged build. */
  version?: string;
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
  /** 1–12. */
  academic_year_start_month: number;
  /** ISO 3166-1 alpha-2. */
  country: string;
  /** ISO 4217. */
  currency: string;
  /** IANA name. */
  time_zone: string;
  /** Create and activate the current academic year. */
  first_year: boolean;
  /** Also create the usual classes with a section A each. */
  default_classes: boolean;
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
  /** Plain-language notes about anything setup could not do (demo load only; usually empty). */
  notes?: string[];
}

export interface UserSummary {
  id: string;
  school_id: string;
  username: string;
  full_name: string;
  email: string | null;
  is_active: boolean;
  roles: string[];
  /** `local` or `directory` (Standard stage 8). */
  auth_source?: "local" | "directory";
}

/** Mirrors `UserAccount` (GET /users). */
export interface UserAccount extends UserSummary {
  staff_id: string | null;
  staff_name: string | null;
  last_login_at: string | null;
}

/** Mirrors `CreateUserBody` (POST /users). */
export interface CreateUserBody {
  staff_id: string;
  username: string;
  password: string;
  role: LoginRole;
}

/** Mirrors `DemoSetupRequest` (POST /setup/demo). */
export interface DemoSetupRequest {
  admin_username: string;
  admin_password: string;
  backup_password: string;
  size?: "full" | "small";
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

export type DateOrder = "day_first" | "month_first";

export interface SchoolSummary {
  id: string;
  name: string;
  code: string;
  city: string;
  is_demo: boolean;
  academic_year_start_month: number;
  currency: string;
  currency_symbol: string;
  country: string;
  country_name: string;
  time_zone: string;
  /** What the national identity number is called here. */
  id_label: string;
  date_order: DateOrder;
}

/** Mirrors `ProfileBody` (PATCH /schools/current); omitted fields stay as they are. */
export interface SchoolProfileUpdate {
  name?: string;
  city?: string;
  currency?: string;
  country?: string;
  time_zone?: string;
}

export interface CountryInfo {
  code: string;
  name: string;
  time_zone: string;
  currency: string;
  weekend: number[];
  id_label: string;
  date_order: DateOrder;
}

export interface CurrencyInfo {
  code: string;
  name: string;
  symbol: string;
}

/** Mirrors `LocaleLists` (GET /system/locale). */
export interface LocaleLists {
  countries: CountryInfo[];
  currencies: CurrencyInfo[];
  time_zones: string[];
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
  start_month: number;
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
  /** Whether the login may sign in; null without a login. */
  login_active: boolean | null;
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
  /** Heading of the payment-reference block on vouchers (country default until edited). */
  payment_reference_label: string;
  payment_reference_note: string;
}

export interface PaymentReference {
  label: string;
  note: string;
  reference: string;
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
  currency: string;
}

export interface VoucherDetail extends FeeVoucher {
  payment_reference: PaymentReference;
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

/** Result of the automatic late fee pass (`POST /fees/late-fees/run`). */
export interface LateFeeReport {
  as_of: string | null;
  assessed: number;
  total: string;
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
export type StaffStatus = "present" | "absent" | "late" | "leave";

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
  /** "HH:MM:SS" of a late mark (Standard). */
  arrived_at: string | null;
  /** Set when an approved leave request wrote the row (Standard). */
  leave_request_id: string | null;
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

export interface DashboardFees {
  collected_this_month: Money;
  month_start: string;
  defaulters: number;
  outstanding: Money;
}

export interface DashboardView {
  date: string;
  day_kind: DayKind;
  attendance: { totals: StatusTotals; unmarked: UnmarkedSection[] } | null;
  backup: BackupStatus;
  students_active: number | null;
  staff_active: number | null;
  fees: DashboardFees | null;
}

// ---- M1 stage 4: exams ---------------------------------------------------------------------

export type SheetStatus = "draft" | "submitted" | "approved";

export type FormulaOp = "sum" | "average" | "weighted" | "best_of";

export type AbsentRule = "zero" | "skip";

export interface Formula {
  op: FormulaOp;
  inputs: Array<{ assessment_type_id: string; weight?: string | null }>;
  n?: number | null;
  /** How an absence in an input counts; omitted = the operation's default. */
  absent?: AbsentRule | null;
}

export interface AssessmentType {
  id: string;
  school_id: string;
  academic_year_id: string;
  name: string;
  position: number;
  /** Weight within its term. */
  weight: Money;
  term_id: string | null;
  kind: "entered" | "calculated";
  formula: Formula | null;
}

export interface Term {
  id: string;
  school_id: string;
  academic_year_id: string;
  name: string;
  position: number;
  starts_on: string | null;
  ends_on: string | null;
  weight: Money;
}

export interface ExamStructure {
  terms: Array<Term & { components: AssessmentType[] }>;
  unassigned: AssessmentType[];
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

/** Mirrors `UploadMarksRequest` (POST /exams/sheet/upload). */
export interface UploadMarksRequest {
  assessment_id: string;
  section_id: string;
  subject_id: string;
  file_name: string;
  content_base64: string;
}

/** Mirrors `UploadMarksResult`: the draft sheet when every row was good, else the issues. */
export interface UploadMarksResult {
  sheet: MarksSheetView | null;
  saved: number;
  issues: Array<{ row_no: number; message: string }>;
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
  /** False when the student does not take this subject (the cell is empty). */
  takes: boolean;
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
  term_id: string | null;
  kind: "entered" | "calculated";
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
  /** One per term, in order. */
  term_percents: Array<Money | null>;
  weighted_percent: Money | null;
  grade: string | null;
}

export interface CardTerm {
  id: string;
  name: string;
  weight: Money;
  component_ids: string[];
  percent: Money | null;
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
  terms: CardTerm[];
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

export type ImportKind = "students" | "staff" | "opening_balances" | "enquiries";
export type DuplicatePolicy = "skip" | "update" | "merge";

export interface ImportField {
  key: string;
  label: string;
  required: boolean;
  aliases: string[];
  example: string;
}

/** Mirrors `CreateLoginBody` (POST /staff/{id}/login). */
export interface CreateLoginBody {
  username: string;
  password: string;
  role: LoginRole;
}

export type LoginRole =
  "principal" | "admin" | "accountant" | "teacher" | "librarian" | "storekeeper";

/** Mirrors `UpdateLoginBody` (PATCH /staff/{id}/login); omitted fields stay as they are. */
export interface UpdateLoginBody {
  role?: LoginRole;
  is_active?: boolean;
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
  /** File columns mapped to nothing; shown so no column is dropped unseen. */
  unmapped: string[];
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

// ---- M1 stage 7: year rollover ----------------------------------------------------------

export interface SectionMove {
  section_id: string;
  class_name: string;
  section_name: string;
  students: number;
  target_class_id: string | null;
  target_class_name: string | null;
}

/** Mirrors `RolloverPreview` in the API. */
export interface RolloverPreview {
  old_year: AcademicYear;
  new_year_name: string;
  new_start_year: number;
  moves: SectionMove[];
  promoted: number;
  graduating: number;
  students_with_balance: number;
  balance_carried: Money;
  new_year_exists: boolean;
  months_into_year: number;
}

export interface RolloverReport {
  new_year: AcademicYear;
  sections_created: number;
  promoted: number;
  retained: number;
  graduated: number;
  structure_lines_copied: number;
  assessments_created: number;
  balance_carried: Money;
}

// ---- Reports (M2 stage 1) --------------------------------------------------------------------

export type ReportLang = "en" | "ur";
export type ReportModule =
  | "students"
  | "fees"
  | "attendance"
  | "exams"
  | "timetable"
  | "leave"
  | "admissions"
  | "library"
  | "transport"
  | "hostel"
  | "inventory";
export type ReportParamKind =
  | "academic_year"
  | "class"
  | "section"
  | "status"
  | "month"
  | "date_from"
  | "date_to"
  | "date"
  | "assessment"
  | "number"
  | "student_number"
  | "board_exam"
  | "consent_type"
  | "leave_status";

export type ReportAccess =
  | "reports_read"
  | "attendance_read"
  | "exams_read"
  | "timetable_read"
  | "leave_read"
  | "library_read"
  | "transport_read"
  | "hostel_read"
  | "inventory_read";

export interface ReportParamDef {
  name: string;
  kind: ReportParamKind;
  label: string;
  required: boolean;
}

export interface ReportCatalogueEntry {
  code: string;
  title: string;
  description: string;
  landscape: boolean;
  access: ReportAccess;
  params: ReportParamDef[];
}

export interface ReportCatalogueModule {
  module: ReportModule;
  label: string;
  reports: ReportCatalogueEntry[];
}

export interface ReportColumn {
  label: string;
  align: "start" | "end" | "center";
  stretch: boolean;
}

export interface ReportGroup {
  heading: string | null;
  rows: string[][];
  subtotal: string[] | null;
}

/** Mirrors `madrasa_api::reports::Report`. */
export interface ReportDocument {
  code: string;
  lang: ReportLang;
  school_name: string;
  title: string;
  subtitle: string | null;
  params: Array<{ label: string; value: string }>;
  columns: ReportColumn[];
  groups: ReportGroup[];
  total: string[] | null;
  notes: string[];
  landscape: boolean;
  generated_on: string;
  row_count: number;
}

// ---- Custom report builder (M2 stage 3) ----------------------------------------------------

export type BuilderFieldType = "text" | "number" | "money" | "date" | "boolean" | "choice";
export type BuilderOp =
  | "eq"
  | "ne"
  | "contains"
  | "starts_with"
  | "in"
  | "is_empty"
  | "not_empty"
  | "gt"
  | "gte"
  | "lt"
  | "lte"
  | "between"
  | "before"
  | "after"
  | "is_true"
  | "is_false";

export interface BuilderField {
  key: string;
  label: string;
  type: BuilderFieldType;
  choices: Array<{ value: string; label: string }>;
  operators: BuilderOp[];
}

export interface BuilderSource {
  key: string;
  label: string;
  fields: BuilderField[];
}

export interface BuilderCatalogue {
  sources: BuilderSource[];
  operators: Array<{ value: BuilderOp; label: string }>;
  default_limit: number;
  max_limit: number;
}

export interface ReportFilter {
  field: string;
  op: BuilderOp;
  value: string;
  value2?: string | null;
}

export interface ReportDefinition {
  source: string;
  columns: string[];
  filters: ReportFilter[];
  sort: Array<{ field: string; desc: boolean }>;
  group_by: string | null;
  limit: number | null;
}

export interface CustomReport {
  id: string;
  name: string;
  description: string;
  source: string;
  definition: ReportDefinition;
  owner_user_id: string;
  owner_name: string | null;
  is_shared: boolean;
  can_edit: boolean;
  updated_at: string;
}

export interface CustomReportRequest {
  name: string;
  description: string;
  definition: ReportDefinition;
  is_shared: boolean;
}

// ---- Examination boards (M2 stage 4) ---------------------------------------------------------

export interface BoardBand {
  grade: string;
  min_percent: string | number;
}

export interface ExamBoard {
  id: string;
  school_id: string;
  name: string;
  short_code: string;
  country: string;
  grading_mode: "marks" | "grades" | "both";
  pass_percent: string;
  grade_bands: BoardBand[];
}

export interface BoardRequest {
  name: string;
  short_code: string;
  country?: string;
  grading_mode?: "marks" | "grades" | "both";
  pass_percent?: string;
  grade_bands: BoardBand[];
}

export interface BoardSubject {
  id: string;
  board_exam_id: string;
  name: string;
  subject_id: string | null;
  max_marks: string;
  position: number;
}

export type BoardExamStatus = "announced" | "results_imported" | "closed";

export interface BoardExamView {
  id: string;
  school_id: string;
  board_id: string;
  academic_year_id: string;
  name: string;
  status: BoardExamStatus;
  board_name: string;
  academic_year_name: string;
  class_ids: string[];
  class_names: string[];
  subjects: BoardSubject[];
  candidates: number;
  results: number;
}

export interface BoardExamRequest {
  board_id?: string;
  academic_year_id?: string;
  name: string;
  status?: BoardExamStatus;
  class_ids: string[];
  subjects: Array<{ name: string; subject_id: string | null; max_marks: string }>;
}

export interface BoardCandidate {
  id: string;
  board_exam_id: string;
  student_id: string;
  roll_no: string | null;
  registration_no: string | null;
  group_name: string | null;
  student_number: string;
  full_name: string;
  father_name: string;
  class_name: string | null;
  section_name: string | null;
  section_id: string | null;
}

export type BoardSubjectStatus = "pass" | "fail" | "absent" | "withheld";
export type BoardCandidateStatus = BoardSubjectStatus | "pending";

export interface BoardSheetRow {
  candidate: BoardCandidate;
  cells: Array<{
    board_subject_id: string;
    obtained: string | null;
    max_marks: string;
    grade: string | null;
    status: BoardSubjectStatus | null;
  }>;
  total: string;
  max: string;
  percent: string | null;
  grade: string | null;
  status: BoardCandidateStatus;
  position: number | null;
}

export interface BoardResultSheet {
  exam: BoardExamView;
  board: ExamBoard;
  rows: BoardSheetRow[];
  subjects: Array<{
    board_subject_id: string;
    name: string;
    appeared: number;
    passed: number;
    average_percent: string | null;
  }>;
  candidates: number;
  with_results: number;
  passed: number;
  pass_percent: string | null;
}

export type BoardImportLayout = "wide" | "long";

export interface BoardParsedFile {
  file_name: string;
  sheets: string[];
  sheet: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
  layout: BoardImportLayout;
  mapping_saved: boolean;
}

export interface BoardImportRequest {
  file_name?: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
  layout: BoardImportLayout;
  save_mapping?: boolean;
}

export interface BoardImportRow {
  row_no: number;
  roll_no: string;
  candidate_id: string | null;
  candidate_name: string | null;
  results: Array<{
    board_subject_id: string;
    subject: string;
    obtained: string | null;
    max_marks: string;
    grade: string | null;
    status: BoardSubjectStatus;
    replaces: boolean;
  }>;
  errors: string[];
  warnings: string[];
}

export interface BoardImportSummary {
  rows: number;
  candidates: number;
  results: number;
  replaced: number;
  errors: number;
}

export interface BoardImportPreview {
  rows: BoardImportRow[];
  summary: BoardImportSummary;
  unmapped: string[];
}

export interface StudentBoardResult {
  exam: BoardExamView;
  board_name: string;
  row: BoardSheetRow;
}

// ---- Subject allocation (M2 stage 6) -------------------------------------------------------

export interface ClassSubjectIds {
  subject_ids: string[];
  elective_ids: string[];
}

export interface SubjectGroup {
  id: string;
  school_id: string;
  class_id: string;
  name: string;
  position: number;
  hlc: string;
  subject_ids: string[];
}

export interface SubjectGroupRequest {
  name: string;
  position?: number;
  subject_ids: string[];
}

export interface AllocationSubject {
  subject_id: string;
  name: string;
  code: string;
  is_elective: boolean;
}

export interface AllocationStudent {
  enrollment_id: string;
  student_id: string;
  student_number: string;
  full_name: string;
  roll_no: number | null;
  group_id: string | null;
  allocated: boolean;
  allocated_ids: string[];
  subject_ids: string[];
}

export interface SectionAllocations {
  warnings?: string[];
  section_id: string;
  class_id: string;
  class_name: string;
  section_name: string;
  subjects: AllocationSubject[];
  groups: SubjectGroup[];
  students: AllocationStudent[];
}

export interface AllocationLine {
  enrollment_id: string;
  group_id?: string | null;
  subject_ids?: string[];
}

export interface StudentSubjects {
  warnings?: string[];
  enrollment_id: string;
  academic_year_id: string;
  class_id: string;
  class_name: string;
  group_id: string | null;
  allocated: boolean;
  subjects: Array<AllocationSubject & { taken: boolean }>;
  groups: SubjectGroup[];
}

export interface AllocationParsedFile {
  file_name: string;
  sheets: string[];
  sheet: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
}

export interface AllocationImportRequest {
  file_name?: string;
  headers: string[];
  rows: string[][];
  mapping: Record<string, number>;
}

export interface AllocationImportRow {
  row_no: number;
  student_number: string;
  student_id: string | null;
  student_name: string | null;
  class_name: string | null;
  enrollment_id: string | null;
  group_id: string | null;
  group_name: string | null;
  subject_ids: string[];
  subject_names: string[];
  changed: boolean;
  errors: string[];
  warnings: string[];
}

export interface AllocationImportSummary {
  rows: number;
  changed: number;
  unchanged: number;
  errors: number;
}

export interface AllocationImportPreview {
  rows: AllocationImportRow[];
  summary: AllocationImportSummary;
}

// ---- Allocation screens (M2 stage 7) ------------------------------------------------------

export interface GridSubject {
  id: string;
  name: string;
  code: string;
}

export interface GridCell {
  subject_id: string;
  staff_id: string | null;
  staff_name: string | null;
  extra: number;
}

export interface GridSection {
  section_id: string;
  class_id: string;
  class_name: string;
  section_name: string;
  class_teacher_id: string | null;
  class_teacher_name: string | null;
  subject_ids: string[];
  cells: GridCell[];
}

export interface GridStaff {
  id: string;
  full_name: string;
  designation: string;
  cells: number;
  class_teacher_of: number;
}

export interface TeacherGrid {
  academic_year_id: string;
  subjects: GridSubject[];
  sections: GridSection[];
  staff: GridStaff[];
}

export interface SectionStudent {
  student_id: string;
  enrollment_id: string;
  section_id: string;
  student_number: string;
  full_name: string;
  gender: Gender;
  roll_no: number | null;
}

export interface MoveSection {
  section_id: string;
  name: string;
  capacity: number | null;
  class_teacher_name: string | null;
  size: number;
  male: number;
  female: number;
  other: number;
  students: SectionStudent[];
}

export interface ClassSections {
  academic_year_id: string;
  class_id: string;
  class_name: string;
  sections: MoveSection[];
}

export interface PlannedMove {
  student_id: string;
  full_name: string;
  student_number: string;
  roll_no: number | null;
  from_section_id: string;
  from_name: string;
  to_section_id: string;
  to_name: string;
}

export interface BalancePreview {
  moves: PlannedMove[];
  sizes: Array<{ section_id: string; name: string; before: number; after: number }>;
}

export type RollNumbersMode = "keep" | "reassign";

export interface MovesApplied {
  moved: number;
  view: ClassSections;
}

// ---- Consent manager (M2 stage 8) ---------------------------------------------------------

export type ConsentGivenBy = "guardian" | "student" | "either";
export type ConsentRequired = "none" | "all" | "class_range";
export type ConsentRenewal = "none" | "yearly";
export type ConsentStatus = "given" | "refused" | "withdrawn";
export type ConsentCurrent = ConsentStatus | "expired" | "missing";

export interface ConsentType {
  id: string;
  school_id: string;
  name: string;
  description: string;
  given_by: ConsentGivenBy;
  required: ConsentRequired;
  min_level: number | null;
  max_level: number | null;
  renewal: ConsentRenewal;
  position: number;
  hlc: string;
}

export interface ConsentTypeRequest {
  name: string;
  description?: string;
  given_by: ConsentGivenBy;
  required: ConsentRequired;
  min_level?: number | null;
  max_level?: number | null;
  renewal: ConsentRenewal;
  position?: number;
}

export interface ConsentRecord {
  id: string;
  school_id: string;
  student_id: string;
  consent_type_id: string;
  status: ConsentStatus;
  guardian_id: string | null;
  academic_year_id: string | null;
  recorded_on: string;
  recorded_by: string | null;
  note: string;
  created_at: string;
  type_name: string;
  guardian_name: string | null;
  recorded_by_name: string | null;
}

export interface GuardianOption {
  id: string;
  full_name: string;
  relationship: string;
}

export interface StudentConsents {
  academic_year_id: string;
  class_level: number | null;
  types: Array<
    ConsentType & { needed: boolean; current: ConsentCurrent; latest: ConsentRecord | null }
  >;
  guardians: GuardianOption[];
  history: ConsentRecord[];
}

export interface ConsentRecordRequest {
  consent_type_id: string;
  status: ConsentStatus;
  guardian_id?: string | null;
  recorded_on?: string;
  note?: string;
  academic_year_id?: string;
}

export interface ConsentCell {
  consent_type_id: string;
  required: boolean;
  current: ConsentCurrent;
  recorded_on: string | null;
  guardian_name: string | null;
}

export interface SectionConsentRow {
  student_id: string;
  student_number: string;
  full_name: string;
  roll_no: number | null;
  guardians: GuardianOption[];
  cells: ConsentCell[];
}

export interface SectionConsents {
  section_id: string;
  academic_year_id: string;
  class_name: string;
  section_name: string;
  class_level: number;
  types: ConsentType[];
  students: SectionConsentRow[];
  missing: number;
}

export interface ConsentBulkLine {
  student_id: string;
  consent_type_id: string;
  status: ConsentStatus;
  guardian_id?: string | null;
  note?: string;
}

// ---- Timetable (Standard stage 1) --------------------------------------------------------------
// Mirrors crates/api/src/routes/timetable.rs and crates/db/src/timetable.rs. Objects produced
// by the scheduling engine (check findings, slot options, cover days, the generator report)
// keep its camelCase keys.

export interface ShiftDay {
  working: boolean;
  periods: number;
}

export interface PeriodTime {
  start: string;
  end: string;
}

export interface ShiftBreak {
  after_period: number;
  name: string;
  cannot_cross: boolean;
}

export interface TimetableShift {
  id: string;
  school_id: string;
  academic_year_id: string;
  name: string;
  enabled: boolean;
  /** Seven entries, Monday first. */
  days: ShiftDay[];
  period_times: PeriodTime[];
  breaks: ShiftBreak[];
  position: number;
  hlc: string;
}

export interface ShiftBody {
  academic_year_id?: string;
  name: string;
  enabled: boolean;
  days: ShiftDay[];
  period_times: PeriodTime[];
  breaks: ShiftBreak[];
  position?: number;
}

export interface Room {
  id: string;
  school_id: string;
  name: string;
  short_name: string;
  building: string;
  capacity: number | null;
  hlc: string;
}

export interface RoomBody {
  name: string;
  short_name?: string;
  building?: string;
  capacity?: number | null;
}

export interface TimetableLesson {
  id: string;
  school_id: string;
  academic_year_id: string;
  subject_id: string;
  teacher_ids: string[];
  section_ids: string[];
  room_ids: string[];
  periods_per_card: number;
  cards_per_week: number;
  hlc: string;
}

export interface LessonView extends TimetableLesson {
  subject: string;
  subject_short: string;
  teachers: string[];
  sections: string[];
  rooms: string[];
}

export interface LessonBody {
  academic_year_id?: string;
  subject_id: string;
  teacher_ids: string[];
  section_ids: string[];
  room_ids: string[];
  periods_per_card: number;
  cards_per_week: number;
}

export interface TimeOffCell {
  shift_id: string;
  /** 0 = Monday. */
  day: number;
  /** 1-based. */
  period: number;
  /** 1 = avoid, 2 = never. */
  state: number;
}

export interface TeacherLimits {
  max_per_day?: number | null;
  max_per_week?: number | null;
  max_consecutive?: number | null;
  max_days_per_week?: number | null;
}

export interface SchoolRules {
  default_max_per_day: number | null;
  default_max_per_week: number | null;
  max_same_subject_per_day: number;
  allow_relaxation: boolean;
  improve_seconds: number;
}

export type RuleKind = "school" | "teacher" | "section" | "room";

export interface TimetableRule {
  id: string;
  school_id: string;
  academic_year_id: string;
  kind: RuleKind;
  entity_id: string | null;
  shift_ids: string[];
  home_room_id: string | null;
  time_off: TimeOffCell[];
  limits: TeacherLimits | SchoolRules | Record<string, never>;
  hlc: string;
}

export interface RuleBody {
  academic_year_id?: string;
  kind: RuleKind;
  entity_id?: string | null;
  shift_ids?: string[];
  home_room_id?: string | null;
  time_off?: TimeOffCell[];
  limits?: TeacherLimits | SchoolRules | Record<string, never>;
}

export interface Named {
  id: string;
  name: string;
  class_id?: string;
  code?: string;
}

export interface CheckFinding {
  level: "error" | "warning";
  kind: string;
  title: string;
  message: string;
  fix: string;
}

export interface CheckReport {
  errors: number;
  warnings: number;
  cards: number;
  periods: number;
  findings: CheckFinding[];
}

export interface LoadRow {
  id: string;
  name: string;
  used: number;
  available: number;
}

export interface LoadSummary {
  cards: number;
  classes: LoadRow[];
  teachers: LoadRow[];
}

export type TimetableStatus = "draft" | "published";

export interface Timetable {
  id: string;
  school_id: string;
  academic_year_id: string;
  name: string;
  status: TimetableStatus;
  report: GenerationReportView | null;
  generated_at: string | null;
  created_at: string;
  hlc: string;
}

export interface TimetableSetup {
  academic_year: AcademicYear;
  shifts: TimetableShift[];
  rooms: Room[];
  lessons: LessonView[];
  rules: TimetableRule[];
  sections: Named[];
  teachers: Named[];
  subjects: Named[];
  timetables: Timetable[];
  published_id: string | null;
  check: CheckReport;
  load: LoadSummary;
  school_rules: SchoolRules;
}

export interface UnplacedCard {
  lessonId: string;
  number: number;
  title: string;
  message: string;
  fix: string;
}

export interface QualityItem {
  key: string;
  label: string;
  value: number;
  weight: number;
}

export interface GenerationReportView {
  total: number;
  placed: number;
  unplaced: UnplacedCard[];
  seconds: number;
  cancelled: boolean;
  timedOut: boolean;
  qualityBefore: { items: QualityItem[]; penalty: number };
  qualityAfter: { items: QualityItem[]; penalty: number };
  relaxed: { kind: string; message: string }[];
  locksInTheWay?: unknown[];
}

export type JobStatus = "running" | "done" | "failed";

export interface TimetableJob {
  id: string;
  timetable_id: string;
  academic_year_id: string;
  status: JobStatus;
  phase: string;
  placed: number;
  total: number;
  fraction: number;
  error: string | null;
  report: GenerationReportView | null;
}

export interface GenerateBody {
  academic_year_id?: string;
  timetable_id?: string;
  name?: string;
  time_limit_seconds: number;
  improve_seconds?: number;
  seed?: number;
}

export interface TimetableCard {
  id: string;
  timetable_id: string;
  lesson_id: string;
  number: number;
  shift_id: string;
  day: number;
  period: number;
  length: number;
  room_id: string | null;
  locked: boolean;
}

export interface CoverDuty {
  date: string;
  shift_id: string;
  period: number;
  length: number;
  lesson_id: string;
  subject: string;
  sections: string[];
  absent_teacher: string;
}

export interface TimetableView {
  timetable: Timetable | null;
  academic_year: AcademicYear;
  shifts: TimetableShift[];
  lessons: LessonView[];
  cards: TimetableCard[];
  rooms: Room[];
  sections: Named[];
  teachers: Named[];
  problems: string[];
  job: TimetableJob | null;
  covers: CoverDuty[];
  today: number | null;
}

export interface RoomOption {
  id: string;
  name: string;
  free: boolean;
}

export interface SlotOption {
  shift: string;
  day: number;
  period: number;
  status: "ok" | "soft" | "blocked";
  reasons: string[];
  rooms: RoomOption[];
}

export interface SwapOption {
  lessonId: string;
  number: number;
  ok: boolean;
  reasons: string[];
}

export interface PeriodRef {
  shift_id: string;
  period: number;
}

export interface TimetableAbsence {
  id: string;
  school_id: string;
  academic_year_id: string;
  staff_id: string;
  from_date: string;
  to_date: string;
  periods: PeriodRef[];
  note: string;
  hlc: string;
}

export interface AbsenceView extends TimetableAbsence {
  staff_name: string;
}

export interface AbsenceBody {
  academic_year_id?: string;
  staff_id: string;
  from_date: string;
  to_date?: string;
  periods?: PeriodRef[];
  note?: string;
}

export interface CoverCandidate {
  teacherId: string;
  name: string;
  sameSubject: boolean;
  covers: number;
  suitable: boolean;
  reasons: string[];
}

export interface CoverNeed {
  absenceId: string;
  date: string;
  lessonId: string;
  number: number;
  absentTeacher: string;
  absentName: string;
  shift: string;
  shiftName: string;
  period: number;
  length: number;
  start: string;
  end: string;
  classes: string;
  subject: string;
  room: string;
  note: string;
  coverTeacher: string | null;
  coverName: string | null;
  noCover: boolean;
  warning: string | null;
  candidates: CoverCandidate[];
}

export interface CoverDay {
  date: string;
  title: string;
  working: boolean;
  needs: CoverNeed[];
  stale: unknown[];
  hasTimetable: boolean;
}

export interface CoverCounts {
  year: string;
  since: string | null;
  total: number;
  totalPeriods: number;
  teachers: { teacherId: string; name: string; covers: number; periods: number }[];
}

export interface SubstitutionsView {
  academic_year: AcademicYear;
  date: string;
  day: CoverDay;
  absences: AbsenceView[];
  counts: CoverCounts;
  teachers: Named[];
  shifts: TimetableShift[];
}

export interface CoverBody {
  absence_id: string;
  date: string;
  lesson_id: string;
  number: number;
  cover_staff_id: string | null;
  note?: string;
}

export interface SheetRow {
  shift: string;
  period: string;
  start: string;
  end: string;
  classes: string;
  subject: string;
  absent: string;
  cover: string;
  covered: boolean;
  room: string;
  note: string;
}

export interface SheetDay {
  date: string;
  title: string;
  rows: SheetRow[];
}

// ---- Leave (Standard stage 2) ---------------------------------------------------------------
// Mirrors crates/api/src/routes/leave.rs and crates/db/src/leave.rs. Day counts are decimal
// strings ("0.5", "3.0").

export type LeaveRequestStatus = "pending" | "approved" | "rejected" | "withdrawn";

export interface LeaveType {
  id: string;
  school_id: string;
  name: string;
  code: string;
  entitlement_days: string;
  carry_over_max: string;
  needs_approval: boolean;
  position: number;
  hlc: string;
}

export interface LeaveTypeBody {
  name: string;
  code?: string;
  entitlement_days: number;
  carry_over_max?: number;
  needs_approval: boolean;
  position?: number;
}

export interface LeaveSettings {
  approver_staff_id: string | null;
  approver_name: string | null;
}

export interface LeaveBalance {
  id: string | null;
  staff_id: string;
  staff_name: string;
  leave_type_id: string;
  type_name: string;
  entitlement: string;
  carried_over: string;
  adjustment: string;
  used: string;
  pending: string;
  note: string;
  granted: string;
  available: string;
  remaining: string;
}

export interface LeaveBalanceBody {
  academic_year_id?: string;
  staff_id: string;
  leave_type_id: string;
  entitlement?: number | null;
  carried_over?: number;
  adjustment?: number;
  note?: string;
}

export interface LeaveRequest {
  id: string;
  school_id: string;
  staff_id: string;
  leave_type_id: string;
  academic_year_id: string;
  from_date: string;
  to_date: string;
  half_day: "am" | "pm" | null;
  days: string;
  reason: string;
  attachment_note: string;
  status: LeaveRequestStatus;
  requested_by: string | null;
  decided_by: string | null;
  decided_at: string | null;
  decision_note: string;
  created_at: string;
  hlc: string;
}

export interface LeaveRequestView extends LeaveRequest {
  staff_name: string;
  staff_number: string;
  type_name: string;
  decided_by_name: string | null;
}

export interface LeaveRequestsView {
  requests: LeaveRequestView[];
  can_decide: boolean;
  own_staff_id: string | null;
}

export interface LeaveRequestBody {
  staff_id?: string;
  leave_type_id: string;
  from_date: string;
  to_date?: string;
  half_day?: "am" | "pm" | "";
  reason?: string;
  attachment_note?: string;
}

export interface StudentLeaveRequest {
  id: string;
  school_id: string;
  student_id: string;
  academic_year_id: string;
  from_date: string;
  to_date: string;
  reason: string;
  guardian_id: string | null;
  status: LeaveRequestStatus;
  recorded_by: string | null;
  decided_by: string | null;
  decided_at: string | null;
  decision_note: string;
  created_at: string;
  hlc: string;
}

export interface StudentLeaveView extends StudentLeaveRequest {
  student_number: string;
  full_name: string;
  class_name: string;
  section_id: string | null;
  section_name: string;
  guardian_name: string | null;
  decided_by_name: string | null;
  days: number;
}

export interface StudentLeaveList {
  requests: StudentLeaveView[];
  decide_sections: string[] | null;
  can_manage: boolean;
}

export interface StudentLeaveBody {
  student_id: string;
  from_date: string;
  to_date?: string;
  reason?: string;
  guardian_id?: string | null;
}

export interface StaffRegisterCell {
  status: StaffStatus;
  arrived_at: string | null;
  approved_leave: boolean;
}

export interface StaffRegisterRow {
  staff_id: string;
  staff_number: string;
  full_name: string;
  designation: string;
  cells: (StaffRegisterCell | null)[];
  present: number;
  absent: number;
  late: number;
  leave: number;
}

export interface StaffRegisterView {
  month: string;
  days: string[];
  school_days: boolean[];
  rows: StaffRegisterRow[];
}

// ---- Admissions (Standard stage 3) ------------------------------------------------------------
// Mirrors crates/api/src/routes/admissions.rs and crates/db/src/admissions.rs.

export type StageKind = "open" | "admitted" | "rejected" | "withdrawn";

export interface AdmissionStage {
  id: string;
  school_id: string;
  code: string;
  name: string;
  kind: StageKind;
  position: number;
  hlc: string;
}

export interface StageBody {
  name: string;
  code?: string;
  kind: StageKind;
  position?: number;
}

export interface AdmissionSettings {
  fee_head_id: string | null;
  fee_head_name: string | null;
}

export interface Application {
  id: string;
  school_id: string;
  application_no: string;
  academic_year_id: string;
  class_id: string | null;
  stage_id: string;
  enquiry_date: string;
  source: string;
  full_name: string;
  father_name: string;
  date_of_birth: string | null;
  gender: string;
  cnic: string | null;
  address: string;
  phone: string | null;
  email: string | null;
  previous_school: string;
  guardian_name: string;
  guardian_relationship: string;
  guardian_phone: string | null;
  guardian_cnic: string | null;
  guardian_email: string | null;
  guardian_occupation: string;
  test_date: string | null;
  test_marks: string | null;
  test_total: string | null;
  interview_date: string | null;
  interview_notes: string;
  offer_date: string | null;
  offer_fee: string | null;
  offer_expires: string | null;
  notes: string;
  custom: Record<string, unknown>;
  student_id: string | null;
  admitted_on: string | null;
  created_at: string;
  updated_at: string;
  hlc: string;
}

export interface ApplicationView extends Application {
  stage_name: string;
  stage_kind: StageKind;
  class_name: string | null;
  student_number: string | null;
}

export interface AdmissionEvent {
  id: string;
  application_id: string;
  from_stage_id: string | null;
  to_stage_id: string;
  to_stage_name: string;
  note: string;
  changed_by: string | null;
  changed_by_name: string | null;
  changed_at: string;
}

export interface ApplicationDetail extends ApplicationView {
  events: AdmissionEvent[];
}

export interface ApplicationBody {
  academic_year_id?: string;
  class_id?: string | null;
  stage_id?: string | null;
  enquiry_date?: string | null;
  source?: string;
  full_name: string;
  father_name?: string;
  date_of_birth?: string | null;
  gender?: string;
  cnic?: string | null;
  address?: string;
  phone?: string | null;
  email?: string | null;
  previous_school?: string;
  guardian_name?: string;
  guardian_relationship?: string;
  guardian_phone?: string | null;
  guardian_cnic?: string | null;
  guardian_email?: string | null;
  guardian_occupation?: string;
  test_date?: string | null;
  test_marks?: number | null;
  test_total?: number | null;
  interview_date?: string | null;
  interview_notes?: string;
  offer_date?: string | null;
  offer_fee?: number | null;
  offer_expires?: string | null;
  notes?: string;
}

export interface FunnelRow {
  stage_id: string;
  reached: number;
  current: number;
}

export interface AdmissionsBoard {
  academic_year: AcademicYear;
  stages: AdmissionStage[];
  applications: ApplicationView[];
  funnel: FunnelRow[];
}

export interface BulkMoveResult {
  moved: number;
  refused: [string, string][];
}

export interface ConversionResult {
  application: ApplicationDetail;
  student_id: string;
  student_number: string;
  guardian_id: string | null;
  voucher_id: string | null;
}

// ---- Library (Standard stage 4) --------------------------------------------------------------

export type LibraryMemberKind = "student" | "staff";
export type CopyCondition = "good" | "fair" | "poor";
export type CopyStatus = "available" | "on_loan" | "lost" | "withdrawn";

/** Mirrors `LoanRules` in madrasa_core::library. `fine_per_day` is a decimal string from the API. */
export interface LoanRules {
  max_books: number;
  loan_days: number;
  fine_per_day: string | number;
  max_renewals: number;
}

/** Mirrors `LibrarySettings` (GET/PUT /library/settings). */
export interface LibrarySettings {
  rules: { student: LoanRules; staff: LoanRules };
  fine_fee_head_id: string | null;
  fine_fee_head_name?: string | null;
}

/** Mirrors `BookView` (GET /library/books): the book plus copy counts. */
export interface BookView {
  id: string;
  school_id: string;
  title: string;
  author: string;
  publisher: string;
  isbn: string;
  category: string;
  language: string;
  subject: string;
  year: number | null;
  custom: Record<string, unknown>;
  hlc: string;
  copies: number;
  available: number;
  on_loan: number;
}

/** Mirrors `Copy`: one physical copy with its accession number. */
export interface LibraryCopy {
  id: string;
  school_id: string;
  book_id: string;
  accession_no: string;
  shelf: string;
  condition: CopyCondition;
  status: CopyStatus;
  hlc: string;
}

/** Mirrors `CopyView` (GET /library/copies): a copy with its book's title. */
export interface CopyView extends LibraryCopy {
  title: string;
  author: string;
  category: string;
}

/** Mirrors `CopyLookup` (GET /library/copies/lookup): the copy and its open loan, if any. */
export interface CopyLookup extends CopyView {
  loan: LoanView | null;
}

/** Mirrors `BookDetail` (GET /library/books/{id}). */
export interface BookDetail extends BookView {
  copy_list: LibraryCopy[];
  loans: LoanView[];
}

/** Book fields of `BookRequest`; copies are added through `accession_nos`. */
export interface BookBody {
  title: string;
  author: string;
  publisher: string;
  isbn: string;
  category: string;
  language: string;
  subject: string;
  year: number | null;
}

/** Mirrors `LoanView` (loans with copy and member names). Money fields are decimal strings. */
export interface LoanView {
  id: string;
  school_id: string;
  copy_id: string;
  member_kind: LibraryMemberKind;
  member_id: string;
  issued_on: string;
  due_on: string;
  returned_on: string | null;
  renewals: number;
  issued_by: string | null;
  returned_by: string | null;
  lost: boolean;
  fine: string;
  fine_waived: string;
  waiver_reason: string;
  waived_by: string | null;
  fine_voucher_id: string | null;
  fine_settled_on: string | null;
  hlc: string;
  accession_no: string;
  title: string;
  author: string;
  member_name: string;
  member_number: string;
  member_detail: string;
}

/** Mirrors `Member` (GET /library/members): a student or staff member who can borrow. */
export interface LibraryMember {
  kind: LibraryMemberKind;
  id: string;
  name: string;
  number: string;
  detail: string;
}

/** Mirrors `MemberPage` (GET /library/members/{kind}/{id}). */
export interface MemberPage {
  member: LibraryMember;
  loans: LoanView[];
  open: number;
  overdue: number;
  fines_due: string;
  rules: LoanRules;
}

/** Mirrors `IssueRequest` (POST /library/loans). */
export interface IssueBody {
  copy_id?: string;
  accession_no?: string;
  member_kind: LibraryMemberKind;
  member_id: string;
  issued_on?: string;
}

/** Mirrors `ReturnRequest` (POST /library/loans/{id}/return). */
export interface ReturnBody {
  returned_on?: string;
  lost?: boolean;
  lost_charge?: number;
}

/** Mirrors `SettleResult` (POST /library/loans/{id}/settle). */
export interface SettleResult {
  loan: LoanView;
  voucher_id: string | null;
}

/** Mirrors `StockRow` (GET /library/stock). */
export interface StockRow {
  category: string;
  titles: number;
  copies: number;
  available: number;
  on_loan: number;
  lost: number;
  withdrawn: number;
}

// ---- Transport (Standard stage 5) ------------------------------------------------------------

/** Mirrors `CapacityWarning` in madrasa_core::transport. */
export type CapacityWarning = { kind: "full" } | { kind: "over"; by: number };

/** Mirrors `Vehicle` (GET /transport/vehicles). */
export interface Vehicle {
  id: string;
  school_id: string;
  registration_no: string;
  make_model: string;
  capacity: number | null;
  driver_name: string;
  driver_phone: string;
  helper_name: string;
  helper_phone: string;
  active: boolean;
  notes: string;
  custom: Record<string, unknown>;
  hlc: string;
}

export type VehicleBody = Omit<Vehicle, "id" | "school_id" | "hlc">;

/** Mirrors `Stop`. Times are `HH:MM:SS`. */
export interface TransportStop {
  id: string;
  school_id: string;
  route_id: string;
  name: string;
  position: number;
  pickup_time: string | null;
  drop_time: string | null;
  monthly_fee: string | null;
  hlc: string;
}

/** Mirrors `StopInput`: an existing stop (with `id`) or a new one. */
export interface StopBody {
  id?: string;
  name: string;
  pickup_time: string | null;
  drop_time: string | null;
  monthly_fee: string | number | null;
}

/** Mirrors `RouteView` (GET /transport/routes). */
export interface RouteView {
  id: string;
  school_id: string;
  name: string;
  code: string;
  vehicle_id: string | null;
  monthly_fee: string | number;
  active: boolean;
  notes: string;
  hlc: string;
  vehicle_registration: string | null;
  vehicle_capacity: number | null;
  driver_name: string | null;
  stops: number;
  students: number;
}

/** Mirrors `RouteDetail` (GET /transport/routes/{id}). */
export interface RouteDetail extends RouteView {
  stop_list: TransportStop[];
}

/** Mirrors `RouteInput` (POST/PUT /transport/routes). */
export interface RouteBody {
  name: string;
  code: string;
  vehicle_id: string | null;
  monthly_fee: string | number;
  active: boolean;
  notes: string;
  stops: StopBody[];
}

/** Mirrors `AssignmentView`. */
export interface AssignmentView {
  id: string;
  school_id: string;
  student_id: string;
  academic_year_id: string;
  route_id: string;
  stop_id: string;
  from_date: string;
  to_date: string | null;
  amount: string | null;
  notes: string;
  hlc: string;
  student_number: string;
  student_name: string;
  class_name: string;
  section_name: string;
  section_id: string;
  roll_no: number | null;
  guardian_phone: string;
  route_name: string;
  stop_name: string;
  stop_position: number;
  pickup_time: string | null;
  charge: string;
}

/** Mirrors `StudentRow` (GET /transport/students). */
export interface TransportStudent {
  student_id: string;
  student_number: string;
  full_name: string;
  class_name: string;
  section_name: string;
  section_id: string;
  roll_no: number | null;
  assignment_id: string | null;
  route_id: string | null;
  route_name: string | null;
  stop_id: string | null;
  stop_name: string | null;
  charge: string | null;
}

/** Mirrors `AssignInput` (POST /transport/assignments). */
export interface AssignBody {
  student_id: string;
  academic_year_id: string;
  stop_id: string;
  from_date: string;
  amount?: string | number | null;
  notes?: string;
}

/** Mirrors `AssignResult`. */
export interface AssignResult {
  assignment: AssignmentView;
  route_students: number;
  seats_left: number | null;
  warning: CapacityWarning | null;
}

/** Mirrors `BulkAssignResult` (POST /transport/assignments/bulk). */
export interface BulkAssignResult {
  assigned: number;
  refused: [string, string][];
  route_students: number;
  seats_left: number | null;
  warning: CapacityWarning | null;
}

/** Mirrors `AssignmentPatch` (PUT /transport/assignments/{id}); omitted fields stay. */
export interface AssignmentPatch {
  stop_id?: string;
  to_date?: string | null;
  amount?: string | number | null;
  notes?: string;
}

/** Mirrors `Roster` (GET /transport/routes/{id}/roster). */
export interface RouteRoster extends RouteView {
  academic_year_id: string;
  by_stop: { stop: TransportStop; students: AssignmentView[] }[];
  total: number;
  seats_left: number | null;
  warning: CapacityWarning | null;
}

/** Mirrors `TransportSettings` (GET/PUT /transport/settings). */
export interface TransportSettings {
  fee_head_id: string | null;
  fee_head_name?: string | null;
}

// ---- Hostel (Standard stage 6) ---------------------------------------------------------------

export type HostelKind = "boys" | "girls" | "mixed";
export type RegisterKind = "leave_out" | "visitor";

/** Mirrors `HostelView` (GET /hostel/hostels). */
export interface HostelView {
  id: string;
  school_id: string;
  name: string;
  kind: HostelKind;
  warden_name: string;
  warden_phone: string;
  monthly_fee: string | number;
  fee_head_id: string | null;
  requires_consent: boolean;
  active: boolean;
  notes: string;
  custom: Record<string, unknown>;
  hlc: string;
  fee_head_name: string | null;
  rooms: number;
  beds: number;
  occupied: number;
}

/** Mirrors `HostelInput`. */
export interface HostelBody {
  name: string;
  kind: HostelKind;
  warden_name: string;
  warden_phone: string;
  monthly_fee: string | number;
  fee_head_id: string | null;
  requires_consent: boolean;
  active: boolean;
  notes: string;
  custom: Record<string, unknown>;
}

/** Mirrors `HostelRoom`. */
export interface HostelRoom {
  id: string;
  school_id: string;
  hostel_id: string;
  floor: string;
  name: string;
  beds: number;
  monthly_fee: string | null;
  active: boolean;
  notes: string;
  hlc: string;
  occupied: number;
}

/** Mirrors `RoomInput`. */
export interface HostelRoomBody {
  floor: string;
  name: string;
  beds: string | number;
  monthly_fee: string | number | null;
  active: boolean;
  notes: string;
}

/** Mirrors `HostelDetail` (GET /hostel/hostels/{id}). */
export interface HostelDetail extends HostelView {
  room_list: HostelRoom[];
}

/** Mirrors `AllocationView`. */
export interface AllocationView {
  id: string;
  school_id: string;
  student_id: string;
  academic_year_id: string;
  hostel_id: string;
  room_id: string;
  bed_no: number;
  from_date: string;
  to_date: string | null;
  amount: string | null;
  notes: string;
  hlc: string;
  student_number: string;
  student_name: string;
  gender: string;
  class_name: string;
  section_name: string;
  roll_no: number | null;
  guardian_phone: string;
  hostel_name: string;
  room_name: string;
  floor: string;
  charge: string;
}

/** Mirrors `AllocateInput` (POST /hostel/allocations). */
export interface AllocateBody {
  student_id: string;
  academic_year_id: string;
  room_id: string;
  bed_no: number;
  from_date: string;
  amount?: string | number | null;
  notes?: string;
}

/** Mirrors `AllocationPatch` (PUT /hostel/allocations/{id}); omitted fields stay. */
export interface AllocationPatch {
  to_date?: string | null;
  amount?: string | number | null;
  notes?: string;
}

/** Mirrors `Board` (GET /hostel/board). */
export interface HostelBoard extends HostelView {
  academic_year_id: string;
  by_room: { room: HostelRoom; beds: { bed_no: number; resident: AllocationView | null }[] }[];
  occupancy: { beds: number; occupied: number; free: number };
}

/** Mirrors `HostelRegisterEntry` (GET /hostel/register). Times are RFC 3339. */
export interface HostelRegisterEntry {
  id: string;
  school_id: string;
  hostel_id: string;
  student_id: string;
  kind: RegisterKind;
  started_at: string;
  expected_back: string | null;
  ended_at: string | null;
  visitor_name: string;
  visitor_relationship: string;
  visitor_phone: string;
  guardian_consent: boolean;
  consent_note: string;
  recorded_by: string | null;
  closed_by: string | null;
  notes: string;
  hlc: string;
  student_number: string;
  student_name: string;
  hostel_name: string;
  room_name: string;
}

/** Mirrors `RegisterInput` (POST /hostel/register). */
export interface HostelRegisterBody {
  student_id: string;
  kind: RegisterKind;
  started_at?: string | null;
  expected_back?: string | null;
  visitor_name?: string;
  visitor_relationship?: string;
  visitor_phone?: string;
  guardian_consent?: boolean;
  consent_note?: string;
  notes?: string;
}

// ---- Inventory (Standard stage 7) ------------------------------------------------------------

export type MovementKind =
  | "purchase_in"
  | "issue"
  | "return"
  | "write_off"
  | "adjust_in"
  | "adjust_out"
  | "transfer_out"
  | "transfer_in";
export type RecipientKind = "staff" | "student" | "room";
export type LocationKind = "store" | "lab" | "room" | "office" | "other";
export type AssetStatus = "in_store" | "in_use" | "under_repair" | "disposed";

export interface InventoryCategory {
  id: string;
  school_id: string;
  name: string;
  position: number;
  hlc: string;
}

export interface InventoryLocation {
  id: string;
  school_id: string;
  name: string;
  kind: LocationKind;
  active: boolean;
  notes: string;
  hlc: string;
}

export interface LocationBody {
  name: string;
  kind: LocationKind;
  active: boolean;
  notes: string;
}

/** Mirrors `ItemView` (GET /inventory/items). Quantities are decimal strings. */
export interface ItemView {
  id: string;
  school_id: string;
  category_id: string | null;
  name: string;
  code: string;
  unit: string;
  reorder_level: string | null;
  is_asset: boolean;
  active: boolean;
  notes: string;
  custom: Record<string, unknown>;
  hlc: string;
  category_name: string | null;
  on_hand: string;
  below_reorder: boolean;
  assets: number;
}

/** Mirrors `ItemInput`. */
export interface ItemBody {
  category_id: string | null;
  name: string;
  code: string;
  unit: string;
  reorder_level: string | number | null;
  is_asset: boolean;
  active: boolean;
  notes: string;
  custom: Record<string, unknown>;
}

/** Mirrors `StockRow` (GET /inventory/stock). */
export interface InventoryStockRow {
  item_id: string;
  item_name: string;
  code: string;
  unit: string;
  category_name: string | null;
  location_id: string;
  location_name: string;
  on_hand: string;
}

/** Mirrors `MovementView`. */
export interface MovementView {
  id: string;
  school_id: string;
  item_id: string;
  location_id: string;
  kind: MovementKind;
  quantity: string;
  unit_cost: string | null;
  total_cost: string | null;
  moved_on: string;
  recipient_kind: RecipientKind | null;
  recipient_id: string | null;
  recipient_name: string;
  reference: string;
  transfer_id: string | null;
  notes: string;
  recorded_by: string | null;
  hlc: string;
  item_name: string;
  unit: string;
  location_name: string;
  recorded_by_name: string | null;
}

/** Mirrors `MovementInput` (POST /inventory/movements). */
export interface MovementBody {
  item_id: string;
  location_id: string;
  kind: MovementKind;
  quantity: string | number;
  unit_cost?: string | number | null;
  moved_on?: string | null;
  recipient_kind?: RecipientKind | null;
  recipient_id?: string | null;
  recipient_name?: string;
  reference?: string;
  notes?: string;
}

/** Mirrors `MovementResult`. */
export interface MovementResult {
  movement: MovementView;
  on_hand: string;
  below_reorder: boolean;
}

/** Mirrors `AssetView`. */
export interface AssetView {
  id: string;
  school_id: string;
  item_id: string;
  tag: string;
  serial_no: string;
  location_id: string | null;
  status: AssetStatus;
  assigned_kind: RecipientKind | null;
  assigned_id: string | null;
  assigned_name: string;
  purchased_on: string | null;
  cost: string | null;
  notes: string;
  hlc: string;
  item_name: string;
  location_name: string | null;
}

/** Mirrors `AssetInput`. */
export interface AssetBody {
  item_id: string;
  tag: string;
  serial_no: string;
  location_id: string | null;
  status: AssetStatus;
  assigned_kind: RecipientKind | null;
  assigned_id: string | null;
  assigned_name: string;
  purchased_on: string | null;
  cost: string | number | null;
  notes: string;
}

/** Mirrors `ItemDetail` (GET /inventory/items/{id}). */
export interface ItemDetail extends ItemView {
  by_location: InventoryStockRow[];
  recent: MovementView[];
  asset_list: AssetView[];
}

/** Mirrors `Recipient` (GET /inventory/recipients). */
export interface InventoryRecipient {
  kind: RecipientKind;
  id: string;
  name: string;
  detail: string;
}

// ---- Directory login (Standard stage 8) ------------------------------------------------------

export interface DirectoryGroupRole {
  group: string;
  role: string;
}

/** Mirrors `DirectorySettings` (GET/PUT /settings/directory). The bind password is never returned. */
export interface DirectorySettings {
  enabled: boolean;
  url: string;
  bind_dn: string;
  user_base_dn: string;
  user_filter: string;
  username_attr: string;
  name_attr: string;
  email_attr: string;
  group_attr: string;
  group_roles: DirectoryGroupRole[];
  default_role: string;
  login_hint: string;
  has_bind_password: boolean;
  roles: string[];
}

/** Mirrors `DirectorySettingsBody`: the settings plus the bind password when it changes. */
export type DirectorySettingsBody = Omit<DirectorySettings, "has_bind_password" | "roles"> & {
  bind_password?: string | null;
};

/** Mirrors `DirectoryUser`. */
export interface DirectoryUser {
  dn: string;
  username: string;
  full_name: string;
  email: string | null;
  groups: string[];
  disabled: boolean;
}

/** Mirrors `DirectoryTestResult` (POST /settings/directory/test). */
export interface DirectoryTestResult {
  bind_ok: boolean;
  message: string;
  user: DirectoryUser | null;
  role: string | null;
}

/** Mirrors `AuthOptions` (GET /auth/options, no token needed). */
export interface AuthOptions {
  directory_login: boolean;
  directory_hint: string;
}

// ---- Read-only API keys (Standard stage 9) ---------------------------------------------------

/** Mirrors `ApiKey` (the secret and its hash are never returned). */
export interface ApiKey {
  id: string;
  school_id: string;
  name: string;
  key_prefix: string;
  scopes: string[];
  ip_allow: string[];
  expires_at: string | null;
  last_used_at: string | null;
  created_by: string | null;
  revoked_at: string | null;
  created_at: string;
  hlc: string;
}

/** Mirrors `ApiKeysView` (GET /settings/api-keys). */
export interface ApiKeysView {
  keys: ApiKey[];
  scopes: string[];
  rate_limit_per_minute: number;
  base_path: string;
}

/** Mirrors `ApiKeyInput` (POST /settings/api-keys). */
export interface ApiKeyBody {
  name: string;
  scopes: string[];
  ip_allow?: string[];
  expires_at?: string | null;
}

/** Mirrors `CreatedApiKey`: the key appears here once. */
export interface CreatedApiKey extends ApiKey {
  key: string;
}

/** Mirrors `ApiKeyCall`. */
export interface ApiKeyCall {
  id: string;
  key_id: string;
  called_at: string;
  method: string;
  path: string;
  status: number;
  ip: string;
}

// ---- Nexus link (Settings → Nexus) ----

/** An incoming record that could not be stored yet, with a plain-language reason. */
export interface NexusParked {
  table: string;
  id: string;
  reason: string;
}

export interface NexusRoundReport {
  sent: number;
  received: number;
  overwrites: number;
  parked_at_hub: NexusParked[];
  parked_here: NexusParked[];
}

export interface NexusEnded {
  at: string;
  by: "school" | "head_office";
  reason: string;
}

/** `GET /nexus/link`: whether this school has joined a hub and how syncing is going. */
export interface NexusLinkStatus {
  linked: boolean;
  hub_url: string | null;
  hub_name: string | null;
  joined_at: string | null;
  last_sync_at: string | null;
  /** Why the last sync failed, in plain words. */
  last_error: string | null;
  last_report: NexusRoundReport | null;
  ended: NexusEnded | null;
  pending: number;
  parked: NexusParked[];
  overwrites: number;
}

/** One entry of the overwrite log: a value that sync replaced or refused. */
export interface NexusOverwrite {
  uid: string | null;
  at: string | null;
  table: string;
  id: string;
  field: string;
  kept: unknown;
  discarded: unknown;
  kept_origin: "school" | "head_office";
  reason: string;
}

export interface NexusLeaveOutcome {
  hub_told: boolean;
  status: NexusLinkStatus;
}

/** One time head office released this school's backup key: who, when and why. */
export interface NexusKeyRelease {
  id: string;
  released_at: string;
  released_by: string;
  reason: string;
  key_id: string;
  /** The school has recorded it in its own audit log. */
  seen: boolean;
}

/** `GET /nexus/link/escrow`: the school's backup key and head office. */
export interface NexusEscrowView {
  /** Head office has set key escrow up. */
  available: boolean;
  /** This school has left a key with head office. */
  deposited: boolean;
  /** The key left there is the school's current backup key. */
  current: boolean;
  deposited_at: string | null;
  releases: NexusKeyRelease[];
}
