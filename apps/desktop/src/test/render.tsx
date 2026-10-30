import { render } from "@testing-library/react";
import type { RenderOptions } from "@testing-library/react";
import type { ReactElement, ReactNode } from "react";
import { MemoryRouter } from "react-router-dom";
import { I18nProvider } from "@/i18n";
import type { Locale } from "@/i18n";
import { ThemeProvider } from "@/theme";
import { ToastProvider } from "@/components/ui/Toast";

export interface AppRenderOptions extends Omit<RenderOptions, "wrapper"> {
  locale?: Locale;
  route?: string;
}

/** Render with every app-level provider (i18n, theme, toasts, router). */
export function renderWithProviders(
  ui: ReactElement,
  { locale = "en", route = "/", ...options }: AppRenderOptions = {},
) {
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <MemoryRouter initialEntries={[route]}>
        <ThemeProvider initialPreference="light">
          <I18nProvider initialLocale={locale}>
            <ToastProvider>{children}</ToastProvider>
          </I18nProvider>
        </ThemeProvider>
      </MemoryRouter>
    );
  }
  return render(ui, { wrapper: Wrapper, ...options });
}
