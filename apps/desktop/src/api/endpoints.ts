import { request } from "./client";
import type {
  AcademicYear,
  Collections,
  Defaulter,
  FeeConcession,
  FeeHead,
  FeePolicy,
  FeeStructureLine,
  FeeVoucherRow,
  GenerationReport,
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
  setup: (body: SetupRequest) =>
    request<SetupResponse>("/setup", { method: "POST", body, auth: false }),
  login: (body: LoginRequest) =>
    request<LoginResponse>("/auth/login", { method: "POST", body, auth: false }),
  logout: () => request<void>("/auth/logout", { method: "POST" }),
  me: () => request<MeResponse>("/auth/me"),
  currentSchool: () => request<SchoolSummary>("/schools/current"),

  // Academic structure
  academicYears: () => request<AcademicYear[]>("/academic-years"),
  createAcademicYear: (body: {
    start_year: number;
    start_month: 4 | 8;
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
  classSubjects: (id: string) => request<{ subject_ids: string[] }>(`/classes/${id}/subjects`),
  setClassSubjects: (id: string, subject_ids: string[]) =>
    request<{ subject_ids: string[] }>(`/classes/${id}/subjects`, {
      method: "PUT",
      body: { subject_ids },
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
};
