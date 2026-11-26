import type { Browser, BrowserContext, Page } from "@playwright/test";
import { tokenFor } from "./api";
import { ADMIN, URLS } from "./env";

export interface DesktopOptions {
  /** Signed in as this user; `null` for the login screen. */
  user?: string | null;
  /** The API the app talks to: the school server itself, or a client's proxy. */
  api?: string;
  lang?: "en" | "ur";
  theme?: "light" | "dark";
  route?: string;
  viewport?: { width: number; height: number };
}

/**
 * The desktop app's front end in a browser page, as it runs inside the desktop shell: the
 * shell's commands (which address is the API at, which mode is this PC in) are answered
 * here, everything else is the real front end against the real API.
 */
export async function openDesktop(
  browser: Browser,
  {
    user = ADMIN,
    api = URLS.school,
    lang = "en",
    theme = "light",
    route = "/",
    viewport,
  }: DesktopOptions = {},
): Promise<{ page: Page; context: BrowserContext }> {
  const context = await browser.newContext({
    viewport: viewport ?? { width: 1366, height: 768 },
    colorScheme: theme,
    locale: lang === "ur" ? "ur-PK" : "en-PK",
    acceptDownloads: true,
  });
  const token = user ? await tokenFor(user) : null;
  const client = api !== URLS.school;
  await context.addInitScript(
    ({ token, lang, theme, api, client }) => {
      // Only on the first page of the context: a reload must keep what the app stored.
      if (!sessionStorage.getItem("e2e.seeded")) {
        sessionStorage.setItem("e2e.seeded", "1");
        localStorage.setItem("madrasa.locale", lang);
        localStorage.setItem("madrasa.theme", theme);
        if (token) localStorage.setItem("madrasa.session", token);
      }
      const mode = client
        ? {
            mode: "client",
            server_url: "https://127.0.0.1:47871",
            fingerprint: "AA:BB",
            listen_port: 8471,
          }
        : { mode: "standalone", server_url: null, fingerprint: null, listen_port: 8471 };
      (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { windowLabel: "main", label: "main" },
        },
        transformCallback: () => 0,
        invoke: async (cmd: string, args: { config?: unknown }) => {
          switch (cmd) {
            case "api_base_url":
              return api;
            case "get_mode":
              return mode;
            case "set_mode":
              return args.config;
            case "network_info":
              return null;
            case "discover_servers":
              return [];
            default:
              throw new Error(`the test shell has no ${cmd}`);
          }
        },
      };
    },
    { token, lang, theme, api, client },
  );
  const page = await context.newPage();
  await page.goto(`${URLS.desktop}${route}`);
  return { page, context };
}

/** Type into the login screen and submit. */
export async function signIn(page: Page, username: string, password: string) {
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
}
