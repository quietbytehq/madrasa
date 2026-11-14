import { Navigate, Route, Routes } from "react-router-dom";
import { ToastProvider } from "@/components/ui/Toast";
import { I18nProvider } from "@/i18n";
import type { Locale } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { SessionProvider, useSession } from "./session";
import { FormDrafts } from "@/components/drafts/FormDrafts";
import { AttendanceScreen } from "./screens/Attendance";
import { ChildLayout, OverviewScreen } from "./screens/Child";
import { Children } from "./screens/Children";
import { FeesScreen } from "./screens/Fees";
import { MessagesScreen } from "./screens/Messages";
import { NoticesScreen } from "./screens/NoticesScreen";
import { ResultsScreen } from "./screens/ResultsScreen";
import { Login, UseCode, Welcome } from "./screens/Start";

/**
 * Signed out: the welcome screen, the access code and sign-in. Signed in: the children,
 * and for one child the six sections. Any other address leads to the right start.
 */
export function AppRoutes() {
  const { signedIn, hubUrl } = useSession();
  if (!signedIn) {
    return (
      <Routes>
        <Route path="/welcome" element={<Welcome />} />
        <Route path="/code" element={<UseCode />} />
        <Route path="/login" element={<Login />} />
        {/* A phone that has been set up before goes straight to sign-in. */}
        <Route path="*" element={<Navigate to={hubUrl ? "/login" : "/welcome"} replace />} />
      </Routes>
    );
  }
  return (
    <Routes>
      <Route path="/" element={<Children />} />
      <Route path="/code" element={<UseCode />} />
      <Route path="/child/:school/:student" element={<ChildLayout />}>
        <Route index element={<OverviewScreen />} />
        <Route path="attendance" element={<AttendanceScreen />} />
        <Route path="results" element={<ResultsScreen />} />
        <Route path="fees" element={<FeesScreen />} />
        <Route path="notices" element={<NoticesScreen />} />
        <Route path="messages" element={<MessagesScreen />} />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}

/** Every provider the screens need, around the routes. The router comes from outside. */
export function ParentApp({ initialLocale }: { initialLocale?: Locale }) {
  return (
    <ThemeProvider>
      <I18nProvider initialLocale={initialLocale}>
        <ToastProvider>
          <SessionProvider>
            <AppRoutes />
            <Drafts />
          </SessionProvider>
        </ToastProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}

/** Autosave of every form, kept encrypted on the phone for the signed-in parent (ADR 0009). */
function Drafts() {
  const { signedIn, phone } = useSession();
  return <FormDrafts user={signedIn && phone ? `parent:${phone}` : null} />;
}
