import { BrowserRouter } from "react-router-dom";
import { I18nProvider } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { ToastProvider } from "@/components/ui/Toast";
import { FeaturesProvider } from "@/app/features";
import { SessionProvider } from "@/app/session";
import { AppRoutes } from "@/app/router";
import { Login } from "@/pages/Login";
import { SetupGate } from "@/app/SetupGate";
import { Setup } from "@/pages/Setup";

export default function App() {
  return (
    <BrowserRouter>
      <ThemeProvider>
        <I18nProvider>
          <ToastProvider>
            <FeaturesProvider>
              <SessionProvider>
                <SetupGate>
                  <AppRoutes login={<Login />} setup={<Setup />} />
                </SetupGate>
              </SessionProvider>
            </FeaturesProvider>
          </ToastProvider>
        </I18nProvider>
      </ThemeProvider>
    </BrowserRouter>
  );
}
