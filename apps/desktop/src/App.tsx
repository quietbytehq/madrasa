import { BrowserRouter } from "react-router-dom";
import { I18nProvider } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { ToastProvider } from "@/components/ui/Toast";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider, useSession } from "@/app/session";
import { ConnectionProvider } from "@/app/connection";
import { FormDrafts } from "@/components/drafts/FormDrafts";
import { AppRoutes } from "@/app/router";
import { Login } from "@/pages/Login";
import { SetupGate } from "@/app/SetupGate";
import { ModeGate } from "@/app/ModeGate";
import { Setup } from "@/pages/Setup";

export default function App() {
  return (
    <BrowserRouter>
      <ThemeProvider>
        <I18nProvider>
          <ToastProvider>
            <ModeGate>
              <FeaturesProvider>
                <SessionProvider>
                  <ConnectionProvider>
                    <SetupGate>
                      <AppRoutes login={<Login />} setup={<Setup />} />
                    </SetupGate>
                    <Drafts />
                  </ConnectionProvider>
                </SessionProvider>
              </FeaturesProvider>
            </ModeGate>
          </ToastProvider>
        </I18nProvider>
      </ThemeProvider>
    </BrowserRouter>
  );
}

/** Autosave of every form, for the signed-in user (ADR 0009). */
function Drafts() {
  const { user } = useSession();
  return <FormDrafts user={user?.id ?? null} />;
}
