import { Navigate, Route, Routes, useLocation } from "react-router-dom";
import type { ReactNode } from "react";
import { AppShell } from "@/layout/AppShell";
import { AttendancePage } from "@/pages/attendance/AttendancePage";
import { BackupPage } from "@/pages/backup/BackupPage";
import { ImportPage } from "@/pages/import/ImportPage";
import { ExamsPage } from "@/pages/exams/ExamsPage";
import { ReportCardsPrintPage, ResultSheetPrintPage } from "@/pages/exams/PrintPages";
import { Dashboard } from "@/pages/Dashboard";
import { ComingSoon } from "@/pages/ComingSoon";
import { FeesPage } from "@/pages/fees/FeesPage";
import { ReceiptPrintPage, VoucherPrintPage } from "@/pages/fees/PrintPages";
import { SettingsPage } from "@/pages/SettingsPage";
import { StaffPage } from "@/pages/staff/StaffPage";
import { StudentFormPage } from "@/pages/students/StudentForm";
import { StudentProfile } from "@/pages/students/StudentProfile";
import { StudentsPage } from "@/pages/students/StudentsPage";
import { Spinner } from "@/components/ui/Spinner";
import { NAV_ITEMS } from "./navigation";
import { useFeatures } from "./features";
import { useSession } from "./session";

function FullPageSpinner() {
  return (
    <div className="flex h-full items-center justify-center">
      <Spinner size="lg" />
    </div>
  );
}

/** Redirects anonymous users to /login, remembering where they wanted to go. */
export function RequireAuth({ children }: { children: ReactNode }) {
  const { status } = useSession();
  const location = useLocation();
  if (status === "loading") return <FullPageSpinner />;
  if (status === "anonymous")
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  return <>{children}</>;
}

export interface AppRoutesProps {
  /** Pages supplied by the app (login, setup); kept as props so the shell is testable alone. */
  login: ReactNode;
  setup: ReactNode;
}

export function AppRoutes({ login, setup }: AppRoutesProps) {
  const { has } = useFeatures();
  return (
    <Routes>
      <Route path="/login" element={login} />
      <Route path="/setup" element={setup} />
      <Route
        element={
          <RequireAuth>
            <AppShell />
          </RequireAuth>
        }
      >
        <Route index element={<Dashboard />} />
        {has("students") && (
          <>
            <Route path="/students" element={<StudentsPage />} />
            <Route path="/students/new" element={<StudentFormPage />} />
            <Route path="/students/:id" element={<StudentProfile />} />
            <Route path="/students/:id/edit" element={<StudentFormPage />} />
            <Route path="/import" element={<ImportPage />} />
          </>
        )}
        {has("fees") && (
          <>
            <Route path="/fees" element={<FeesPage />} />
            <Route path="/fees/vouchers/:id/print" element={<VoucherPrintPage />} />
            <Route path="/fees/receipts/:id/print" element={<ReceiptPrintPage />} />
          </>
        )}
        {has("attendance") && <Route path="/attendance" element={<AttendancePage />} />}
        {has("examinations") && (
          <>
            <Route path="/exams" element={<ExamsPage />} />
            <Route path="/exams/report-cards/print" element={<ReportCardsPrintPage />} />
            <Route path="/exams/results/print" element={<ResultSheetPrintPage />} />
          </>
        )}
        {has("backup") && <Route path="/backup" element={<BackupPage />} />}
        {has("teachers") && <Route path="/teachers" element={<StaffPage />} />}
        {has("settings") && <Route path="/settings" element={<SettingsPage />} />}
        {NAV_ITEMS.filter((item) => item.path !== "/" && !item.available && has(item.feature)).map(
          (item) => (
            <Route
              key={item.path}
              path={item.path}
              element={<ComingSoon labelKey={item.labelKey} />}
            />
          ),
        )}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
