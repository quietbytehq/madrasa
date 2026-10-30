import { request } from "./client";
import type {
  FeaturesResponse,
  HealthResponse,
  LoginRequest,
  LoginResponse,
  MeResponse,
  SchoolSummary,
  SetupRequest,
  SetupResponse,
  SetupStatus,
} from "./types";

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
};
