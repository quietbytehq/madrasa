import { download, request } from "./client";
import type {
  RolloverPreview,
  RolloverReport,
  ImportBatch,
  ImportField,
  ImportKind,
  ImportPreview,
  ImportRequest,
  ImportResult,
  ParsedFile,
  SavedMapping,
  BackupFile,
  BackupRun,
  BackupSettings,
  BackupStatus,
  RestorePreview,
  RestoreResult,
  AssessmentType,
  GradingScheme,
  MarksSheetView,
  MaxMarksLine,
  ReportCard,
  ResultSheet,
  SaveMarksRequest,
  SheetsView,
  AbsenteeRow,
  CalendarView,
  DashboardView,
  Holiday,
  RegisterView,
  SaveSheetRequest,
  SheetHistoryEntry,
  SheetView,
  StaffDayView,
  StaffHistoryView,
  StaffStatus,
  StatusView,
  StudentAttendanceView,
  AcademicYear,
  Collections,
  Defaulter,
  FeeConcession,
  FeeHead,
  FeePolicy,
  FeeStructureLine,
  FeeVoucherRow,
  CreateLoginBody,
  CreateUserBody,
  DemoSetupRequest,
  GenerationReport,
  LateFeeReport,
  LocaleLists,
  SchoolProfileUpdate,
  UpdateLoginBody,
  UploadMarksRequest,
  UploadMarksResult,
  UserAccount,
  Ledger,
  PaymentMethod,
  ReceiptDetail,
  VoucherDetail,
  VoucherStatus,
  Assignment,
  DuplicateMatch,
  FeaturesResponse,
  GuardianInput,
  GuardianLink,
  GuardianView,
  HealthResponse,
  LoginRequest,
  LoginResponse,
  MeResponse,
  NewStudentRequest,
  Page,
  SchoolClass,
  SchoolSettings,
  SchoolSummary,
  Section,
  SetupRequest,
  SetupResponse,
  SetupStatus,
  Staff,
  StudentDetail,
  StudentListQuery,
  StudentListRow,
  StudentPatch,
  StudentStatus,
  Subject,
  ReportCatalogueModule,
  ReportDocument,
  ReportLang,
  BuilderCatalogue,
  CustomReport,
  CustomReportRequest,
  ReportDefinition,
  ExamStructure,
  Formula,
  Term,
  BoardCandidate,
  BoardExamRequest,
  BoardExamView,
  BoardImportPreview,
  BoardImportRequest,
  BoardParsedFile,
  BoardRequest,
  BoardResultSheet,
  ExamBoard,
  StudentBoardResult,
  ClassSubjectIds,
  SubjectGroup,
  SubjectGroupRequest,
  SectionAllocations,
  AllocationLine,
  StudentSubjects,
  AllocationParsedFile,
  AllocationImportRequest,
  AllocationImportPreview,
  AllocationImportSummary,
  TeacherGrid,
  ClassSections,
  BalancePreview,
  RollNumbersMode,
  MovesApplied,
  EditionSettings,
  Edition,
  ConsentType,
  ConsentTypeRequest,
  StudentConsents,
  ConsentRecordRequest,
  SectionConsents,
  ConsentBulkLine,
} from "./types";

function qs(params: object): string {
  const parts = Object.entries(params as Record<string, unknown>)
    .filter(([, v]) => v !== undefined && v !== null && v !== "")
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
  return parts.length ? `?${parts.join("&")}` : "";
}

export const api = {
  health: () => request<HealthResponse>("/health", { auth: false }),
  features: () => request<FeaturesResponse>("/system/features", { auth: false }),
  setupStatus: () => request<SetupStatus>("/setup/status", { auth: false }),
  demoSetup: (body: DemoSetupRequest) =>
    request<SetupResponse>("/setup/demo", { method: "POST", body, auth: false }),
  setup: (body: SetupRequest) =>
    request<SetupResponse>("/setup", { method: "POST", body, auth: false }),
  login: (body: LoginRequest) =>
    request<LoginResponse>("/auth/login", { method: "POST", body, auth: false }),
  logout: () => request<void>("/auth/logout", { method: "POST" }),
  me: () => request<MeResponse>("/auth/me"),
  currentSchool: () => request<SchoolSummary>("/schools/current"),
  updateSchool: (body: SchoolProfileUpdate) =>
    request<SchoolSummary>("/schools/current", { method: "PATCH", body }),
  localeLists: () => request<LocaleLists>("/system/locale"),
  users: () => request<UserAccount[]>("/users"),
  createUser: (body: CreateUserBody) => request<UserAccount>("/users", { method: "POST", body }),
  updateUser: (id: string, body: UpdateLoginBody) =>
    request<UserAccount>(`/users/${id}`, { method: "PATCH", body }),
  resetUserPassword: (id: string, password: string) =>
    request<UserAccount>(`/users/${id}/password`, { method: "POST", body: { password } }),

  // Academic structure
  academicYears: () => request<AcademicYear[]>("/academic-years"),
  createAcademicYear: (body: {
    start_year: number;
    start_month: number;
    name?: string;
    activate?: boolean;
  }) => request<AcademicYear>("/academic-years", { method: "POST", body }),
  activateAcademicYear: (id: string) =>
    request<AcademicYear>(`/academic-years/${id}/activate`, { method: "POST", body: {} }),
  classes: () => request<SchoolClass[]>("/classes"),
  createClass: (body: { name: string; level: number }) =>
    request<SchoolClass>("/classes", { method: "POST", body }),
  updateClass: (id: string, body: { name?: string; level?: number }) =>
    request<SchoolClass>(`/classes/${id}`, { method: "PATCH", body }),
  classSubjects: (id: string) => request<ClassSubjectIds>(`/classes/${id}/subjects`),
  setClassSubjects: (id: string, subject_ids: string[], elective_ids?: string[]) =>
    request<ClassSubjectIds>(`/classes/${id}/subjects`, {
      method: "PUT",
      body: { subject_ids, elective_ids },
    }),
  subjects: () => request<Subject[]>("/subjects"),
  createSubject: (body: { name: string; code: string }) =>
    request<Subject>("/subjects", { method: "POST", body }),
  updateSubject: (id: string, body: { name?: string; code?: string }) =>
    request<Subject>(`/subjects/${id}`, { method: "PATCH", body }),
  sections: (params: { academic_year_id?: string; class_id?: string } = {}) =>
    request<Section[]>(`/sections${qs(params)}`),
  createSection: (body: {
    class_id: string;
    academic_year_id: string;
    name: string;
    class_teacher_id?: string | null;
    capacity?: number | null;
  }) => request<Section>("/sections", { method: "POST", body }),
  updateSection: (
    id: string,
    body: { name?: string; class_teacher_id?: string | null; capacity?: number | null },
  ) => request<Section>(`/sections/${id}`, { method: "PATCH", body }),

  // Staff
  staff: (q?: string) => request<Staff[]>(`/staff${qs({ q })}`),
  staffMember: (id: string) => request<Staff>(`/staff/${id}`),
  createStaffLogin: (id: string, body: CreateLoginBody) =>
    request<Staff>(`/staff/${id}/login`, { method: "POST", body }),
  updateStaffLogin: (id: string, body: UpdateLoginBody) =>
    request<Staff>(`/staff/${id}/login`, { method: "PATCH", body }),
  resetStaffPassword: (id: string, password: string) =>
    request<Staff>(`/staff/${id}/login/password`, { method: "POST", body: { password } }),
  createStaff: (body: Partial<Staff>) => request<Staff>("/staff", { method: "POST", body }),
  updateStaff: (id: string, body: Partial<Staff>) =>
    request<Staff>(`/staff/${id}`, { method: "PATCH", body }),
  staffAssignments: (id: string) =>
    request<{ assignments: Assignment[] }>(`/staff/${id}/assignments`),
  setStaffAssignments: (id: string, assignments: Assignment[]) =>
    request<{ assignments: Assignment[] }>(`/staff/${id}/assignments`, {
      method: "PUT",
      body: { assignments },
    }),

  // Students
  students: (params: StudentListQuery = {}) =>
    request<Page<StudentListRow>>(`/students${qs(params)}`),
  student: (id: string) => request<StudentDetail>(`/students/${id}`),
  createStudent: (body: NewStudentRequest) =>
    request<StudentDetail>("/students", { method: "POST", body }),
  updateStudent: (id: string, body: StudentPatch) =>
    request<StudentDetail>(`/students/${id}`, { method: "PATCH", body }),
  checkDuplicates: (body: {
    full_name: string;
    father_name: string;
    date_of_birth: string;
    cnic?: string;
    exclude_id?: string;
  }) => request<DuplicateMatch[]>("/students/check-duplicates", { method: "POST", body }),
  withdrawStudent: (
    id: string,
    body: { status: StudentStatus; left_on?: string; reason?: string },
  ) => request<StudentDetail>(`/students/${id}/withdraw`, { method: "POST", body }),
  enrolStudent: (id: string, body: { section_id: string; roll_no?: number | null }) =>
    request<StudentDetail>(`/students/${id}/enrol`, { method: "POST", body }),
  setStudentPhoto: (id: string, body: { mime: string; data: string }) =>
    request<void>(`/students/${id}/photo`, { method: "PUT", body }),
  deleteStudentPhoto: (id: string) => request<void>(`/students/${id}/photo`, { method: "DELETE" }),
  linkGuardian: (id: string, body: GuardianInput) =>
    request<GuardianLink[]>(`/students/${id}/guardians`, { method: "POST", body }),
  unlinkGuardian: (id: string, guardianId: string) =>
    request<GuardianLink[]>(`/students/${id}/guardians/${guardianId}`, { method: "DELETE" }),
  searchGuardians: (q: string) => request<GuardianView[]>(`/guardians${qs({ q })}`),
  updateGuardian: (id: string, body: Partial<GuardianView>) =>
    request<GuardianView>(`/guardians/${id}`, { method: "PATCH", body }),

  // Settings
  editionSettings: () => request<EditionSettings>("/settings/edition"),
  updateEdition: (edition: Edition) =>
    request<EditionSettings>("/settings/edition", { method: "PUT", body: { edition } }),
  schoolSettings: () => request<SchoolSettings>("/settings/school"),
  updateSchoolSettings: (body: SchoolSettings) =>
    request<SchoolSettings>("/settings/school", { method: "PUT", body }),

  // Fees
  feePolicy: () => request<FeePolicy>("/fees/policy"),
  updateFeePolicy: (body: FeePolicy) => request<FeePolicy>("/fees/policy", { method: "PUT", body }),
  feeHeads: () => request<FeeHead[]>("/fees/heads"),
  createFeeHead: (body: { name: string; code: string; is_recurring: boolean; position?: number }) =>
    request<FeeHead>("/fees/heads", { method: "POST", body }),
  updateFeeHead: (id: string, body: Partial<FeeHead>) =>
    request<FeeHead>(`/fees/heads/${id}`, { method: "PATCH", body }),
  feeStructure: (academic_year_id: string, class_id?: string) =>
    request<FeeStructureLine[]>(`/fees/structure${qs({ academic_year_id, class_id })}`),
  setFeeStructure: (body: {
    academic_year_id: string;
    class_id: string;
    lines: Array<{ fee_head_id: string; amount: string }>;
  }) => request<FeeStructureLine[]>("/fees/structure", { method: "PUT", body }),
  concessions: (studentId: string) =>
    request<FeeConcession[]>(`/fees/students/${studentId}/concessions`),
  createConcession: (
    studentId: string,
    body: { kind: string; percent?: string; amount?: string; reason: string; valid_from?: string },
  ) => request<FeeConcession>(`/fees/students/${studentId}/concessions`, { method: "POST", body }),
  endConcession: (id: string) => request<void>(`/fees/concessions/${id}`, { method: "DELETE" }),
  vouchers: (params: {
    month?: string;
    class_id?: string;
    section_id?: string;
    status?: VoucherStatus | "";
    student_id?: string;
    q?: string;
  }) => request<FeeVoucherRow[]>(`/fees/vouchers${qs(params)}`),
  generateVouchers: (body: { month: string; class_id?: string; section_id?: string }) =>
    request<GenerationReport>("/fees/vouchers/generate", { method: "POST", body }),
  voucher: (id: string) => request<VoucherDetail>(`/fees/vouchers/${id}`),
  runLateFees: (as_of?: string) =>
    request<LateFeeReport>("/fees/late-fees/run", { method: "POST", body: { as_of } }),
  payVoucher: (
    id: string,
    body: {
      amount: string;
      method: PaymentMethod;
      reference?: string;
      note?: string;
      paid_on?: string;
    },
  ) =>
    request<{ receipt: ReceiptDetail; voucher: VoucherDetail }>(`/fees/vouchers/${id}/payments`, {
      method: "POST",
      body,
    }),
  waiveLateFee: (id: string, reason: string) =>
    request<VoucherDetail>(`/fees/vouchers/${id}/waive-late-fee`, {
      method: "POST",
      body: { reason },
    }),
  cancelVoucher: (id: string, reason: string) =>
    request<VoucherDetail>(`/fees/vouchers/${id}/cancel`, { method: "POST", body: { reason } }),
  receipt: (id: string) => request<ReceiptDetail>(`/fees/receipts/${id}`),
  reverseReceipt: (id: string, reason: string) =>
    request<ReceiptDetail>(`/fees/receipts/${id}/reverse`, { method: "POST", body: { reason } }),
  ledger: (studentId: string) => request<Ledger>(`/fees/students/${studentId}/ledger`),
  defaulters: (params: { as_of?: string; class_id?: string; min_days?: number } = {}) =>
    request<Defaulter[]>(`/fees/defaulters${qs(params)}`),
  collections: (date?: string) => request<Collections>(`/fees/collections${qs({ date })}`),

  // Attendance
  dashboard: () => request<DashboardView>("/dashboard"),
  calendar: (params: { from?: string; to?: string } = {}) =>
    request<CalendarView>(`/attendance/calendar${qs(params)}`),
  setWeekend: (weekend_days: number[]) =>
    request<CalendarView>("/attendance/calendar/weekend", {
      method: "PUT",
      body: { weekend_days },
    }),
  createHoliday: (body: { name: string; starts_on: string; ends_on?: string }) =>
    request<Holiday>("/attendance/calendar/holidays", { method: "POST", body }),
  deleteHoliday: (id: string) =>
    request<void>(`/attendance/calendar/holidays/${id}`, { method: "DELETE" }),
  attendanceSheet: (section_id: string, date: string) =>
    request<SheetView>(`/attendance/sheet${qs({ section_id, date })}`),
  saveAttendanceSheet: (body: SaveSheetRequest) =>
    request<SheetView>("/attendance/sheet", { method: "PUT", body }),
  attendanceSheetHistory: (section_id: string, date: string) =>
    request<SheetHistoryEntry[]>(`/attendance/sheet/history${qs({ section_id, date })}`),
  attendanceStatus: (date?: string) => request<StatusView>(`/attendance/status${qs({ date })}`),
  absentees: (params: { date?: string; class_id?: string } = {}) =>
    request<AbsenteeRow[]>(`/attendance/absentees${qs(params)}`),
  attendanceRegister: (section_id: string, month: string) =>
    request<RegisterView>(`/attendance/register${qs({ section_id, month })}`),
  studentAttendance: (studentId: string, params: { from?: string; to?: string } = {}) =>
    request<StudentAttendanceView>(`/attendance/students/${studentId}${qs(params)}`),
  staffAttendance: (date?: string) => request<StaffDayView>(`/attendance/staff${qs({ date })}`),
  saveStaffAttendance: (body: {
    date: string;
    entries: Array<{ staff_id: string; status: StaffStatus; note?: string }>;
  }) => request<StaffDayView>("/attendance/staff", { method: "PUT", body }),
  staffAttendanceHistory: (staffId: string, params: { from?: string; to?: string } = {}) =>
    request<StaffHistoryView>(`/attendance/staff/${staffId}${qs(params)}`),

  // Exams
  assessments: (academic_year_id: string) =>
    request<AssessmentType[]>(`/exams/assessments${qs({ academic_year_id })}`),
  createAssessment: (body: {
    academic_year_id: string;
    name: string;
    weight?: string;
    position?: number;
    term_id?: string;
    kind?: "entered" | "calculated";
    formula?: Formula;
  }) => request<AssessmentType>("/exams/assessments", { method: "POST", body }),
  defaultAssessments: (academic_year_id: string, terms = 1) =>
    request<AssessmentType[]>("/exams/assessments/defaults", {
      method: "POST",
      body: { academic_year_id, terms },
    }),
  renameAssessment: (
    id: string,
    body: { name: string; position?: number; term_id?: string; formula?: Formula },
  ) => request<AssessmentType>(`/exams/assessments/${id}`, { method: "PATCH", body }),
  examStructure: (academic_year_id: string) =>
    request<ExamStructure>(`/exams/structure${qs({ academic_year_id })}`),
  terms: (academic_year_id: string) => request<Term[]>(`/exams/terms${qs({ academic_year_id })}`),
  createTerm: (body: {
    academic_year_id: string;
    name: string;
    position?: number;
    starts_on?: string | null;
    ends_on?: string | null;
    weight?: string;
  }) => request<Term>("/exams/terms", { method: "POST", body }),
  updateTerm: (
    id: string,
    body: { name: string; position?: number; starts_on?: string | null; ends_on?: string | null },
  ) => request<Term>(`/exams/terms/${id}`, { method: "PUT", body }),
  deleteTerm: (id: string) => request<void>(`/exams/terms/${id}`, { method: "DELETE" }),
  setTermWeights: (academic_year_id: string, weights: Array<{ id: string; weight: string }>) =>
    request<Term[]>("/exams/terms/weights", { method: "PUT", body: { academic_year_id, weights } }),
  deleteAssessment: (id: string) => request<void>(`/exams/assessments/${id}`, { method: "DELETE" }),
  setWeights: (academic_year_id: string, weights: Array<{ id: string; weight: string }>) =>
    request<AssessmentType[]>("/exams/assessments/weights", {
      method: "PUT",
      body: { academic_year_id, weights },
    }),
  grading: () => request<GradingScheme>("/exams/grading"),
  setGrading: (body: GradingScheme) =>
    request<GradingScheme>("/exams/grading", { method: "PUT", body }),
  maxMarks: (assessment_id: string, class_id: string) =>
    request<MaxMarksLine[]>(`/exams/max-marks${qs({ assessment_id, class_id })}`),
  setMaxMarks: (body: {
    assessment_id: string;
    class_id: string;
    lines: Array<{ subject_id: string; max_marks: string }>;
  }) => request<MaxMarksLine[]>("/exams/max-marks", { method: "PUT", body }),
  examSheets: (assessment_id: string, section_id: string) =>
    request<SheetsView>(`/exams/sheets${qs({ assessment_id, section_id })}`),
  examSheet: (assessment_id: string, section_id: string, subject_id: string) =>
    request<MarksSheetView>(`/exams/sheet${qs({ assessment_id, section_id, subject_id })}`),
  downloadMarksTemplate: (assessment_id: string, section_id: string, subject_id: string) =>
    download(`/exams/sheet/template${qs({ assessment_id, section_id, subject_id })}`),
  uploadMarks: (body: UploadMarksRequest) =>
    request<UploadMarksResult>("/exams/sheet/upload", { method: "POST", body }),
  saveMarks: (body: SaveMarksRequest) =>
    request<MarksSheetView>("/exams/sheet", { method: "PUT", body }),
  submitSheet: (id: string) =>
    request<MarksSheetView>(`/exams/sheets/${id}/submit`, { method: "POST", body: {} }),
  approveSheet: (id: string) =>
    request<MarksSheetView>(`/exams/sheets/${id}/approve`, { method: "POST", body: {} }),
  reopenSheet: (id: string) =>
    request<MarksSheetView>(`/exams/sheets/${id}/reopen`, { method: "POST", body: {} }),
  sectionResults: (assessment_id: string, section_id: string) =>
    request<ResultSheet>(`/exams/results/section${qs({ assessment_id, section_id })}`),
  sectionReportCards: (section_id: string) =>
    request<ReportCard[]>(`/exams/results/section/report-cards${qs({ section_id })}`),
  studentReportCard: (studentId: string, academic_year_id?: string) =>
    request<ReportCard>(`/exams/results/student/${studentId}${qs({ academic_year_id })}`),

  // Backup
  backupStatus: () => request<BackupStatus>("/backup/status"),
  backupSettings: () => request<BackupSettings>("/backup/settings"),
  saveBackupSettings: (body: BackupSettings) =>
    request<BackupSettings>("/backup/settings", { method: "PUT", body }),
  runBackup: () => request<BackupRun>("/backup/run", { method: "POST", body: {} }),
  backupRuns: (limit = 20) => request<BackupRun[]>(`/backup/runs${qs({ limit })}`),
  backupFiles: () => request<BackupFile[]>("/backup/files"),
  previewRestore: (path: string) =>
    request<RestorePreview>("/backup/preview", { method: "POST", body: { path } }),
  restoreBackup: (body: { path: string; password?: string; recovery_key?: string }) =>
    request<RestoreResult>("/backup/restore", { method: "POST", body }),
  changeBackupPassword: (body: {
    current_password?: string;
    recovery_key?: string;
    new_password: string;
  }) => request<void>("/backup/password", { method: "POST", body }),
  unlockBackups: (body: { password?: string; recovery_key?: string }) =>
    request<void>("/backup/unlock", { method: "POST", body }),

  // Imports and export
  importFields: (kind: ImportKind) => request<ImportField[]>(`/imports/fields/${kind}`),
  downloadTemplate: (kind: ImportKind, format: "xlsx" | "csv") =>
    download(`/imports/templates/${kind}?format=${format}`),
  parseImport: (body: {
    kind: ImportKind;
    file_name: string;
    content_base64: string;
    sheet?: string;
  }) => request<ParsedFile>("/imports/parse", { method: "POST", body }),
  importMapping: (kind: ImportKind) => request<SavedMapping | null>(`/imports/mappings/${kind}`),
  previewImport: (body: ImportRequest) =>
    request<ImportPreview>("/imports/preview", { method: "POST", body }),
  applyImport: (body: ImportRequest) =>
    request<ImportResult>("/imports/apply", { method: "POST", body }),
  boards: () => request<ExamBoard[]>("/boards"),
  createBoard: (body: BoardRequest) => request<ExamBoard>("/boards", { method: "POST", body }),
  updateBoard: (id: string, body: BoardRequest) =>
    request<ExamBoard>(`/boards/${id}`, { method: "PUT", body }),
  deleteBoard: (id: string) => request<void>(`/boards/${id}`, { method: "DELETE" }),
  boardExams: (params: { board_id?: string; academic_year_id?: string } = {}) =>
    request<BoardExamView[]>(`/boards/exams${qs(params)}`),
  boardExam: (id: string) => request<BoardExamView>(`/boards/exams/${id}`),
  createBoardExam: (body: BoardExamRequest) =>
    request<BoardExamView>("/boards/exams", { method: "POST", body }),
  updateBoardExam: (id: string, body: BoardExamRequest) =>
    request<BoardExamView>(`/boards/exams/${id}`, { method: "PUT", body }),
  deleteBoardExam: (id: string) => request<void>(`/boards/exams/${id}`, { method: "DELETE" }),
  boardCandidates: (examId: string) =>
    request<BoardCandidate[]>(`/boards/exams/${examId}/candidates`),
  registerCandidates: (
    examId: string,
    body: { student_ids?: string[]; class_id?: string; section_id?: string; group_name?: string },
  ) =>
    request<{ added: number; candidates: BoardCandidate[] }>(`/boards/exams/${examId}/candidates`, {
      method: "POST",
      body,
    }),
  updateCandidate: (
    id: string,
    body: { roll_no?: string; registration_no?: string; group_name?: string },
  ) => request<BoardCandidate>(`/boards/candidates/${id}`, { method: "PUT", body }),
  removeCandidate: (id: string) => request<void>(`/boards/candidates/${id}`, { method: "DELETE" }),
  boardResults: (examId: string) => request<BoardResultSheet>(`/boards/exams/${examId}/results`),
  parseBoardResults: (examId: string, body: { file_name: string; content_base64: string }) =>
    request<BoardParsedFile>(`/boards/exams/${examId}/results/parse`, { method: "POST", body }),
  previewBoardResults: (examId: string, body: BoardImportRequest) =>
    request<BoardImportPreview>(`/boards/exams/${examId}/results/preview`, {
      method: "POST",
      body,
    }),
  applyBoardResults: (examId: string, body: BoardImportRequest) =>
    request<{ batch_id: string; summary: BoardImportPreview["summary"] }>(
      `/boards/exams/${examId}/results/apply`,
      { method: "POST", body },
    ),
  studentBoardResults: (studentId: string) =>
    request<StudentBoardResult[]>(`/students/${studentId}/board-results`),
  // Subject allocation (M2 stage 6)
  subjectGroups: (classId: string) => request<SubjectGroup[]>(`/classes/${classId}/subject-groups`),
  createSubjectGroup: (classId: string, body: SubjectGroupRequest) =>
    request<SubjectGroup>(`/classes/${classId}/subject-groups`, { method: "POST", body }),
  updateSubjectGroup: (id: string, body: SubjectGroupRequest) =>
    request<SubjectGroup>(`/subject-groups/${id}`, { method: "PUT", body }),
  deleteSubjectGroup: (id: string) => request<void>(`/subject-groups/${id}`, { method: "DELETE" }),
  sectionAllocations: (sectionId: string) =>
    request<SectionAllocations>(`/sections/${sectionId}/subject-allocations`),
  setSectionAllocations: (sectionId: string, rows: AllocationLine[]) =>
    request<SectionAllocations>(`/sections/${sectionId}/subject-allocations`, {
      method: "PUT",
      body: { rows },
    }),
  studentSubjects: (studentId: string, academicYearId?: string) =>
    request<StudentSubjects>(
      `/students/${studentId}/subjects${qs({ academic_year_id: academicYearId })}`,
    ),
  setStudentSubjects: (
    studentId: string,
    body: { academic_year_id?: string; group_id?: string | null; subject_ids?: string[] },
  ) => request<StudentSubjects>(`/students/${studentId}/subjects`, { method: "PUT", body }),
  downloadAllocationTemplate: (sectionId: string) =>
    download(`/subject-allocations/import/template${qs({ section_id: sectionId })}`),
  parseAllocations: (body: { file_name: string; content_base64: string }) =>
    request<AllocationParsedFile>("/subject-allocations/import/parse", { method: "POST", body }),
  previewAllocations: (body: AllocationImportRequest) =>
    request<AllocationImportPreview>("/subject-allocations/import/preview", {
      method: "POST",
      body,
    }),
  applyAllocations: (body: AllocationImportRequest) =>
    request<{ batch_id: string; summary: AllocationImportSummary }>(
      "/subject-allocations/import/apply",
      { method: "POST", body },
    ),
  // Allocation screens (M2 stage 7)
  teacherGrid: (params: { academic_year_id?: string; class_id?: string } = {}) =>
    request<TeacherGrid>(`/allocation/teachers${qs(params)}`),
  setTeacherCell: (body: { section_id: string; subject_id: string; staff_id: string | null }) =>
    request<void>("/allocation/teachers/cell", { method: "PUT", body }),
  classSections: (classId: string, academicYearId?: string) =>
    request<ClassSections>(
      `/allocation/sections${qs({ class_id: classId, academic_year_id: academicYearId })}`,
    ),
  balanceSections: (body: {
    class_id: string;
    academic_year_id?: string;
    exclude_student_ids?: string[];
  }) => request<BalancePreview>("/allocation/balance", { method: "POST", body }),
  moveStudents: (body: {
    class_id: string;
    academic_year_id?: string;
    moves: Array<{ student_id: string; to_section_id: string }>;
    roll_numbers: RollNumbersMode;
  }) => request<MovesApplied>("/allocation/moves", { method: "POST", body }),
  // Consent manager (M2 stage 8)
  consentTypes: () => request<ConsentType[]>("/consent-types"),
  createConsentType: (body: ConsentTypeRequest) =>
    request<ConsentType>("/consent-types", { method: "POST", body }),
  updateConsentType: (id: string, body: ConsentTypeRequest) =>
    request<ConsentType>(`/consent-types/${id}`, { method: "PUT", body }),
  deleteConsentType: (id: string) => request<void>(`/consent-types/${id}`, { method: "DELETE" }),
  studentConsents: (studentId: string) =>
    request<StudentConsents>(`/students/${studentId}/consents`),
  recordConsent: (studentId: string, body: ConsentRecordRequest) =>
    request<StudentConsents>(`/students/${studentId}/consents`, { method: "POST", body }),
  sectionConsents: (sectionId: string) =>
    request<SectionConsents>(`/sections/${sectionId}/consents`),
  recordSectionConsents: (sectionId: string, rows: ConsentBulkLine[], recorded_on?: string) =>
    request<SectionConsents>(`/sections/${sectionId}/consents`, {
      method: "POST",
      body: { rows, recorded_on },
    }),
  // Printing through the PDF engine (v0.2.0-alpha B3)
  voucherPdf: (id: string, lang: "en" | "ur") =>
    download(`/fees/vouchers/${id}/pdf${qs({ lang })}`),
  vouchersPdf: (ids: string[], lang: "en" | "ur") =>
    download(`/fees/vouchers/pdf${qs({ ids: ids.join(","), lang })}`),
  reportCardsPdf: (params: { section_id?: string; student_id?: string }, lang: "en" | "ur") =>
    download(`/exams/report-cards/pdf${qs({ ...params, lang })}`),
  importBatches: () => request<ImportBatch[]>("/imports/batches"),
  rollbackImport: (id: string) =>
    request<ImportBatch>(`/imports/batches/${id}/rollback`, { method: "POST", body: {} }),
  exportList: (path: string, params: Record<string, string | number | undefined>) =>
    download(`/export/${path}${qs(params)}`),
  reports: (lang: ReportLang) => request<ReportCatalogueModule[]>(`/reports${qs({ lang })}`),
  runReport: (code: string, params: Record<string, string | undefined>, lang: ReportLang) =>
    request<ReportDocument>(`/reports/${code}${qs({ ...params, lang, format: "json" })}`),
  downloadReport: (
    code: string,
    params: Record<string, string | undefined>,
    lang: ReportLang,
    format: "pdf" | "xlsx",
  ) => download(`/reports/${code}${qs({ ...params, lang, format })}`),
  builderCatalogue: (lang: ReportLang) =>
    request<BuilderCatalogue>(`/reports/custom/sources${qs({ lang })}`),
  customReports: () => request<CustomReport[]>("/reports/custom"),
  createCustomReport: (body: CustomReportRequest) =>
    request<CustomReport>("/reports/custom", { method: "POST", body }),
  updateCustomReport: (id: string, body: CustomReportRequest) =>
    request<CustomReport>(`/reports/custom/${id}`, { method: "PUT", body }),
  deleteCustomReport: (id: string) => request<void>(`/reports/custom/${id}`, { method: "DELETE" }),
  previewCustomReport: (definition: ReportDefinition, name: string, lang: ReportLang, limit = 50) =>
    request<ReportDocument>("/reports/custom/run", {
      method: "POST",
      body: { definition, name, lang, limit },
    }),
  runCustomReport: (id: string, lang: ReportLang) =>
    request<ReportDocument>(`/reports/custom/${id}${qs({ lang, format: "json" })}`),
  downloadCustomReport: (id: string, lang: ReportLang, format: "pdf" | "xlsx") =>
    download(`/reports/custom/${id}${qs({ lang, format })}`),

  // Year rollover
  rolloverPreview: () => request<RolloverPreview>("/academic-years/rollover/preview"),
  rollover: (body: { retain: string[]; graduate: string[] }) =>
    request<RolloverReport>("/academic-years/rollover", { method: "POST", body }),
};
