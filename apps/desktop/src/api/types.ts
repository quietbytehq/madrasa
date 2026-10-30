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
