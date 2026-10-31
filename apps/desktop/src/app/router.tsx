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
import { NoAccess } from "@/pages/NoAccess";
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

/** Renders the page only for a role that holds the permission; otherwise "You don't have access". */
export function RequirePermission({
  permission,
  children,
}: {
  permission: string;
  children: ReactNode;
}) {
  const { can } = useSession();
  return can(permission) ? <>{children}</> : <NoAccess />;
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
            <Route
              path="/import"
              element={
                <RequirePermission permission="students.manage">
                  <ImportPage />
                </RequirePermission>
              }
            />
          </>
        )}
        {has("fees") && (
          <>
            <Route
              path="/fees"
              element={
                <RequirePermission permission="fees.read">
                  <FeesPage />
                </RequirePermission>
              }
            />
            <Route
              path="/fees/vouchers/:id/print"
              element={
                <RequirePermission permission="fees.read">
                  <VoucherPrintPage />
                </RequirePermission>
              }
            />
            <Route
              path="/fees/receipts/:id/print"
              element={
                <RequirePermission permission="fees.read">
                  <ReceiptPrintPage />
                </RequirePermission>
              }
            />
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
        {has("backup") && (
          <Route
            path="/backup"
            element={
              <RequirePermission permission="backup.manage">
                <BackupPage />
              </RequirePermission>
            }
          />
        )}
        {has("teachers") && (
          <Route
            path="/teachers"
            element={
              <RequirePermission permission="teachers.read">
                <StaffPage />
              </RequirePermission>
            }
          />
        )}
        {has("settings") && (
          <Route
            path="/settings"
            element={
              <RequirePermission permission="academics.manage">
                <SettingsPage />
              </RequirePermission>
            }
          />
        )}
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
