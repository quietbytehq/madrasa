import { BrowserRouter } from "react-router-dom";
import { I18nProvider } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { ToastProvider } from "@/components/ui/Toast";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import { AppRoutes } from "@/app/router";
import { Login } from "@/pages/Login";
import { SetupGate } from "@/app/SetupGate";

export default function App() {
  return (
    <BrowserRouter>
      <ThemeProvider>
        <I18nProvider>
          <ToastProvider>
            <FeaturesProvider>
              <SessionProvider>
                <SetupGate>
                  <AppRoutes login={<Login />} setup={<SetupPlaceholder />} />
                </SetupGate>
              </SessionProvider>
            </FeaturesProvider>
          </ToastProvider>
        </I18nProvider>
      </ThemeProvider>
    </BrowserRouter>
  );
}

/** Replaced by the real wizard in task 8. */
function SetupPlaceholder() {
  return null;
}
