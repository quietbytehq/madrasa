#!/usr/bin/env node
// Screenshots for the user manual (docs/manual), made with Playwright from the demo school and
// the demo network. Run through scripts/manual-screenshots.sh, which starts the servers:
//
//   node docs/manual/tools/shoot.mjs --lang en|ur [--out docs/manual/screenshots] [--only a,b]
//   node docs/manual/tools/shoot.mjs --probe /students        list what a screen offers
//
// Every screenshot is a step of the manual (docs/manual/manual.yaml names it by id). Scenes
// work in either language: they find buttons and fields by their translation keys
// (apps/desktop/src/i18n), never by English text. Images are written as WebP, light theme.

import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { scenes } from "./scenes.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
export const repo = path.resolve(here, "../../..");

export const URLS = {
  school: "http://127.0.0.1:47702",
  setup: "http://127.0.0.1:47701",
  hub: "http://127.0.0.1:47780",
  hubSetup: "http://127.0.0.1:47781",
  desktop: "http://127.0.0.1:1420",
  parent: "http://127.0.0.1:1440",
};
export const PASSWORD = "demo-password-1";
export const PARENT_PHONE = "+923001234567";
export const SETUP_TOKEN = "manual-setup-token";

// ---- arguments -----------------------------------------------------------------------------
const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const lang = opt("lang", "en");
const out = path.resolve(repo, opt("out", "docs/manual/screenshots"));
const only = (opt("only", "") || "").split(",").filter(Boolean);
const probe = opt("probe", null);

// ---- translations --------------------------------------------------------------------------
function loadDictionary(code) {
  const src = fs.readFileSync(path.join(repo, `apps/desktop/src/i18n/${code}.ts`), "utf8");
  const start = src.indexOf("{", src.indexOf(`export const ${code}`));
  const end = src.lastIndexOf("}");
  return Function(`"use strict"; return (${src.slice(start, end + 1)});`)();
}
const dictionaries = { en: loadDictionary("en"), ur: loadDictionary("ur") };

/** The text of a translation key in the current language, with {params} filled in. */
export function makeL(code) {
  return (key, params = {}) => {
    const text = dictionaries[code][key] ?? dictionaries.en[key];
    if (text === undefined) throw new Error(`no translation key ${key}`);
    return text.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m));
  };
}

// ---- the session ---------------------------------------------------------------------------
async function postJson(url, body, token) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${url}: ${res.status} ${await res.text()}`);
  return res.json();
}

async function getJson(url, token) {
  const res = await fetch(url, { headers: token ? { authorization: `Bearer ${token}` } : {} });
  if (!res.ok) throw new Error(`${url}: ${res.status} ${await res.text()}`);
  return res.json();
}

export class Shots {
  constructor(browser, code) {
    this.browser = browser;
    this.lang = code;
    this.L = makeL(code);
    this.written = [];
    this.tokens = new Map();
  }

  /** A session token for a demo user of the school API at `api`. */
  async token(username, api = URLS.school) {
    const key = `${api}|${username}`;
    if (!this.tokens.has(key)) {
      const r = await postJson(`${api}/api/v1/auth/login`, { username, password: PASSWORD });
      this.tokens.set(key, r.token);
    }
    return this.tokens.get(key);
  }

  /** Demo usernames by role (the demo data is generated, so they are looked up). */
  async users() {
    if (!this.userList) {
      const admin = await this.token("demo.khadija.iqbal").catch(() => null);
      this.userList = admin ? await getJson(`${URLS.school}/api/v1/users`, admin) : [];
    }
    return this.userList;
  }

  async userWithRole(role) {
    const list = await this.users();
    const items = Array.isArray(list) ? list : (list.items ?? []);
    const u = items.find((x) => (x.roles ?? []).includes(role) && x.is_active !== false);
    if (!u) throw new Error(`no demo user with role ${role}`);
    return u.username;
  }

  /** A browser page of the desktop app as it runs in the Tauri shell. */
  async desktop({
    user = "demo.khadija.iqbal",
    api = URLS.school,
    route = "/",
    mode = { mode: "standalone", server_url: null, fingerprint: null, listen_port: 8471 },
    network = null,
    signedIn = true,
    viewport = { width: 1280, height: 760 },
  } = {}) {
    const context = await this.browser.newContext({ viewport, deviceScaleFactor: 1, colorScheme: "light", locale: this.lang === "ur" ? "ur-PK" : "en-PK" });
    const token = signedIn ? await this.token(user, api) : null;
    const shell = {
      api,
      mode,
      network,
      found: [
        {
          host: "192.168.1.20",
          url: "https://192.168.1.20:8471",
          info: { name: "DEMO Gulshan Model School", version: "0.4.2", port: 8471, fingerprint: "3F:A2:9C:41:7B:D0:5E:88:12:C4:9A:6F:E1:03:B7:52:4D:9E:20:AF:68:C1:73:0B:9D:E4:51:26:8F:3A:C0:77" },
        },
      ],
    };
    await context.addInitScript(
      ({ token, lang, shell }) => {
        localStorage.setItem("madrasa.locale", lang);
        localStorage.setItem("madrasa.theme", "light");
        if (token) localStorage.setItem("madrasa.session", token);
        // The desktop shell's commands (apps/desktop/src-tauri), answered here.
        window.__TAURI_INTERNALS__ = {
          metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
          transformCallback: () => 0,
          invoke: async (cmd, args) => {
            switch (cmd) {
              case "api_base_url":
                return shell.api;
              case "get_mode":
                return shell.mode;
              case "set_mode":
                return args.config;
              case "network_info":
                return shell.network;
              case "discover_servers":
                await new Promise((r) => setTimeout(r, 300));
                return shell.found;
              case "probe_server":
                return { url: shell.found[0].url, fingerprint: shell.found[0].info.fingerprint, version: "0.4.2", needs_setup: false };
              default:
                throw new Error(`no ${cmd} in the manual's shell`);
            }
          },
        };
      },
      { token, lang: this.lang, shell },
    );
    const page = await context.newPage();
    await page.goto(`${URLS.desktop}${route}`);
    await settle(page);
    return page;
  }

  /** The Nexus console (served by the hub at `hub`), signed in unless `signedIn` is false. */
  async console({ hub = URLS.hub, route = "/", signedIn = true } = {}) {
    const context = await this.browser.newContext({ viewport: { width: 1280, height: 760 }, deviceScaleFactor: 1, colorScheme: "light" });
    const token = signedIn
      ? (await postJson(`${hub}/api/nexus/v1/auth/login`, { username: "demo.admin", password: PASSWORD })).token
      : null;
    await context.addInitScript(
      ({ token, lang }) => {
        localStorage.setItem("madrasa.locale", lang);
        localStorage.setItem("madrasa.theme", "light");
        if (token) sessionStorage.setItem("madrasa.nexus.session", token);
      },
      { token, lang: this.lang },
    );
    const page = await context.newPage();
    await page.goto(`${hub}${route}`);
    await settle(page);
    return page;
  }

  /** The parent app on a phone-sized screen. */
  async parent({ route = "/", signedIn = true, hub = URLS.hub } = {}) {
    const context = await this.browser.newContext({
      viewport: { width: 390, height: 780 },
      deviceScaleFactor: 2,
      isMobile: true,
      hasTouch: true,
      colorScheme: "light",
    });
    const session = signedIn
      ? await postJson(`${hub}/api/nexus/v1/parent/login`, { phone: PARENT_PHONE, password: PASSWORD })
      : null;
    await context.addInitScript(
      ({ session, lang, hub }) => {
        localStorage.setItem("madrasa.locale", lang);
        localStorage.setItem("madrasa.theme", "light");
        localStorage.setItem("madrasa.parent.hub", hub);
        if (session) {
          localStorage.setItem("madrasa.parent.session", session.token);
          localStorage.setItem("madrasa.parent.name", session.parent.full_name);
          localStorage.setItem("madrasa.parent.phone", session.parent.phone);
        }
      },
      { session, lang: this.lang, hub },
    );
    const page = await context.newPage();
    await page.goto(`${URLS.parent}/#${route}`);
    await settle(page);
    return page;
  }

  /**
   * Save what the page shows as screenshot `id`: the whole window, or one element (`target`,
   * a locator) with some room around it.
   */
  async snap(page, id, { target = null, around = null, margin = 12, fullPage = false, maxHeight = 640 } = {}) {
    if (only.length && !only.includes(id)) return;
    await settle(page);
    let png;
    if (around) {
      // The panel an element sits in: its widest ancestor narrower than the window.
      const box = await around.first().evaluate((el) => {
        const vw = window.innerWidth;
        let node = el;
        while (node.parentElement && node.parentElement.getBoundingClientRect().width < vw * 0.96)
          node = node.parentElement;
        const r = node.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      });
      const vp = page.viewportSize();
      const x = Math.max(0, box.x - margin);
      const y = Math.max(0, box.y - margin);
      png = await page.screenshot({
        clip: { x, y, width: Math.min(vp.width - x, box.width + 2 * margin), height: Math.min(vp.height - y, box.height + 2 * margin) },
      });
    } else if (target) {
      // The element from its top, at most `maxHeight` tall (long lists show their start).
      await target.first().evaluate((el) => el.scrollIntoView({ block: "start" })).catch(() => {});
      await page.waitForTimeout(150);
      const box = await target.first().boundingBox();
      if (!box) throw new Error(`${id}: the element to photograph is not on screen`);
      const vp = page.viewportSize();
      const x = Math.max(0, box.x - margin);
      const y = Math.max(0, box.y - margin);
      png = await page.screenshot({
        clip: {
          x,
          y,
          width: Math.min(vp.width - x, box.width + 2 * margin),
          height: Math.min(vp.height - y, box.height + 2 * margin, maxHeight),
        },
      });
    } else {
      png = await page.screenshot({ fullPage });
    }
    const webp = await toWebp(this.browser, png);
    const file = path.join(out, this.lang, `${id}.webp`);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, webp);
    this.written.push(id);
    console.log(`  ${this.lang}/${id}.webp (${Math.round(webp.length / 1024)} KB)`);
  }
}

/** Let the page finish loading and animating. */
export async function settle(page, ms = 350) {
  await page.waitForLoadState("networkidle").catch(() => {});
  await page.evaluate(() => document.fonts?.ready).catch(() => {});
  await page.waitForTimeout(ms);
}

let converter = null;
/** PNG to WebP with the browser's own encoder: no image tools needed. */
async function toWebp(browser, png) {
  if (!converter) {
    const ctx = await browser.newContext();
    converter = await ctx.newPage();
  }
  const dataUrl = await converter.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    c.getContext("2d").drawImage(img, 0, 0);
    return c.toDataURL("image/webp", 0.85);
  }, png.toString("base64"));
  return Buffer.from(dataUrl.split(",")[1], "base64");
}

// ---- probing a screen while writing scenes -------------------------------------------------
async function probeScreen(s, route) {
  const page = route.startsWith("setup:")
    ? await s.desktop({ api: URLS.setup, signedIn: false, route: route.slice(6) })
    : route.startsWith("mode:")
      ? await s.desktop({ mode: null, signedIn: false, route: route.slice(5) })
      : route.startsWith("hub:")
    ? await s.console({ route: route.slice(4) })
    : route.startsWith("parent:")
      ? await s.parent({ route: route.slice(7) })
      : await s.desktop({ route });
  const outline = await page.evaluate(() => {
    const seen = [];
    const text = (el) => (el.innerText || el.getAttribute("aria-label") || el.value || "").trim().replace(/\s+/g, " ").slice(0, 70);
    for (const el of document.querySelectorAll("h1,h2,h3,button,a[href],[role=tab],input,select,textarea,[data-testid]")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const tid = el.getAttribute("data-testid");
      const label = el.labels?.[0]?.innerText?.trim();
      seen.push(`${el.tagName.toLowerCase()}${el.getAttribute("role") ? `[${el.getAttribute("role")}]` : ""}${tid ? `#${tid}` : ""}${label ? ` <${label}>` : ""} ${text(el)}`);
    }
    return seen.join("\n");
  });
  console.log(outline);
  await page.screenshot({ path: path.join(process.env.PROBE_DIR ?? "/tmp", "probe.png") });
}

// ---- main ----------------------------------------------------------------------------------
const browser = await chromium.launch();
const s = new Shots(browser, lang);
try {
  if (probe) {
    await probeScreen(s, probe);
  } else {
    for (const scene of scenes) {
      if (only.length && !scene.shots.some((id) => only.includes(id))) continue;
      console.log(`${scene.name}`);
      await scene.run(s);
    }
    const missing = scenes
      .flatMap((sc) => sc.shots)
      .filter((id) => (!only.length || only.includes(id)) && !s.written.includes(id));
    if (missing.length) throw new Error(`these screenshots were not made: ${missing.join(", ")}`);
    console.log(`${s.written.length} screenshots in ${lang}`);
  }
} finally {
  await browser.close();
}
