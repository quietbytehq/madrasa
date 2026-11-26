import type { Browser, BrowserContext, Page } from "@playwright/test";
import { PARENT_PHONE, PASSWORD, URLS } from "./env";

export interface ParentOptions {
  signedIn?: boolean;
  lang?: "en" | "ur";
  theme?: "light" | "dark";
  viewport?: { width: number; height: number };
}

let session: { token: string; name: string } | null = null;

/** A parent's session at the hub (one sign-in for the run: sign-ins are counted). */
async function parentSession(): Promise<{ token: string; name: string }> {
  if (!session) {
    const res = await fetch(`${URLS.hub}/api/nexus/v1/parent/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ phone: PARENT_PHONE, password: PASSWORD }),
    });
    if (!res.ok) throw new Error(`parent login: ${res.status} ${await res.text()}`);
    const body = (await res.json()) as {
      token: string;
      parent?: { full_name?: string };
      full_name?: string;
    };
    session = { token: body.token, name: body.parent?.full_name ?? body.full_name ?? "" };
  }
  return session;
}

/** The parent app in a phone-sized page, against the local hub. */
export async function openParent(
  browser: Browser,
  {
    signedIn = true,
    lang = "en",
    theme = "light",
    viewport = { width: 390, height: 844 },
  }: ParentOptions = {},
): Promise<{ page: Page; context: BrowserContext }> {
  const context = await browser.newContext({
    viewport,
    isMobile: true,
    hasTouch: true,
    deviceScaleFactor: 2,
    colorScheme: theme,
    locale: lang === "ur" ? "ur-PK" : "en-PK",
  });
  const s = signedIn ? await parentSession() : null;
  await context.addInitScript(
    ({ s, lang, theme, hub, phone }) => {
      if (sessionStorage.getItem("e2e.seeded")) return;
      sessionStorage.setItem("e2e.seeded", "1");
      localStorage.setItem("madrasa.locale", lang);
      localStorage.setItem("madrasa.theme", theme);
      if (s) {
        localStorage.setItem("madrasa.parent.hub", hub);
        localStorage.setItem("madrasa.parent.session", s.token);
        localStorage.setItem("madrasa.parent.name", s.name);
        localStorage.setItem("madrasa.parent.phone", phone);
      }
    },
    { s, lang, theme, hub: URLS.hub, phone: PARENT_PHONE },
  );
  const page = await context.newPage();
  await page.goto(URLS.parent);
  return { page, context };
}
