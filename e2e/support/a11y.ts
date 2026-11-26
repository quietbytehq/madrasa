import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";

/**
 * What a screen reader or a keyboard user would trip over on this screen, by the WCAG 2.1
 * A and AA rules that a machine can check: controls without a name, fields without a
 * label, wrong roles, broken ARIA, lists and tables that are not what they say. Colour
 * contrast is left to `lowContrast` in checks.ts, which also sees text on tinted cards.
 */
export async function a11yProblems(page: Page, where: string): Promise<string[]> {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
    .disableRules(["color-contrast"])
    .analyze();
  return result.violations.flatMap((v) =>
    v.nodes
      .slice(0, 8)
      .map((n) => `${where}: ${v.id} (${v.impact}): ${v.help}: ${n.target.join(" ")}`),
  );
}

/** Wait until the screen has stopped loading: no spinner, and the heading is there. */
export async function settled(page: Page) {
  await page.getByRole("heading", { level: 1 }).first().waitFor();
  await page.waitForFunction(() => !document.querySelector(".animate-spin"), null, {
    timeout: 15_000,
  });
}

export interface FocusStop {
  what: string;
  /** Position in the form's own order of controls (-1: not one of them). */
  index: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Is there a ring, outline or shadow that shows where the keyboard is? */
  shown: boolean;
  /** Does a screen reader have a name for it? */
  named: boolean;
}

export interface FormWalk {
  /** How many controls the form has that a person can use. */
  controls: number;
  stops: FocusStop[];
}

/**
 * Walk one form with the Tab key, from its first control until focus leaves it, and note
 * for each stop what it is, where it is, whether the focus can be seen and whether it has
 * a name. `form` is a selector for the form.
 */
export async function walkForm(page: Page, form: string): Promise<FormWalk> {
  const USABLE =
    'input:not([type="hidden"]):not([disabled]), select:not([disabled]), textarea:not([disabled]), button:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';
  const controls = await page.evaluate(
    ({ form, USABLE }) => {
      const root = document.querySelector(form)!;
      const list = Array.from(root.querySelectorAll<HTMLElement>(USABLE)).filter((el) => {
        const r = el.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
      });
      // Radio buttons of one group are one stop for the keyboard.
      const seen = new Set<string>();
      const stops = list.filter((el) => {
        if (el instanceof HTMLInputElement && el.type === "radio" && el.name) {
          if (seen.has(el.name)) return false;
          seen.add(el.name);
        }
        return true;
      });
      stops.forEach((el, i) => el.setAttribute("data-walk", String(i)));
      // Radios: every one of the group answers for the group's stop.
      list.forEach((el) => {
        if (el instanceof HTMLInputElement && el.type === "radio" && el.name && !el.dataset.walk) {
          const first = stops.find(
            (s) => s instanceof HTMLInputElement && s.type === "radio" && s.name === el.name,
          );
          if (first) el.setAttribute("data-walk", first.getAttribute("data-walk")!);
        }
      });
      stops[0]?.focus();
      return stops.length;
    },
    { form, USABLE },
  );
  const read = () =>
    page.evaluate((form) => {
      const el = document.activeElement as HTMLElement | null;
      const root = document.querySelector(form);
      if (!el || !root || !root.contains(el)) return null;
      // Measured from the form's own corner: the page scrolls as the focus moves on.
      const box = el.getBoundingClientRect();
      const origin = root.getBoundingClientRect();
      const r = { x: box.x - origin.x, y: box.y - origin.y, width: box.width, height: box.height };
      const s = getComputedStyle(el);
      const outline = s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0;
      const labels = (el as HTMLInputElement).labels;
      const named =
        (labels && labels.length > 0 && (labels[0]!.textContent ?? "").trim() !== "") ||
        !!el.getAttribute("aria-label") ||
        !!el.getAttribute("aria-labelledby") ||
        !!el.getAttribute("title") ||
        ((el.tagName === "BUTTON" || el.tagName === "A") && (el.textContent ?? "").trim() !== "");
      return {
        what: `${el.tagName.toLowerCase()}${el.getAttribute("name") ? `[name=${el.getAttribute("name")}]` : ""} "${(
          el.getAttribute("aria-label") ??
          labels?.[0]?.textContent ??
          el.textContent ??
          ""
        )
          .trim()
          .slice(0, 40)}"`,
        index: Number(el.getAttribute("data-walk") ?? -1),
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
        shown: outline || s.boxShadow !== "none",
        named,
      };
    }, form);
  const stops: FocusStop[] = [];
  for (let i = 0; i < controls * 4 + 8; i++) {
    const stop = await read();
    if (!stop) break;
    const last = stops[stops.length - 1];
    // A date field is several stops for the keyboard (day, month, year) and one control.
    if (!last || last.index !== stop.index || stop.index < 0) {
      if (stops.some((s) => s.index === stop.index && stop.index >= 0)) break;
      stops.push(stop);
    }
    await page.keyboard.press("Tab");
  }
  return { controls, stops };
}

/** What is wrong with a walk, in words; empty when the form is fine from the keyboard. */
export function walkProblems(where: string, walk: FormWalk, rtl = false): string[] {
  const out: string[] = [];
  const { stops, controls } = walk;
  if (stops.length !== controls)
    out.push(`${where}: Tab reached ${stops.length} of the form's ${controls} controls`);
  stops.forEach((s, i) => {
    if (!s.shown) out.push(`${where}: no visible focus on ${s.what}`);
    if (!s.named) out.push(`${where}: no label or name for ${s.what}`);
    if (s.index !== i) out.push(`${where}: Tab reached ${s.what} out of the form's order`);
    const prev = stops[i - 1];
    if (!prev) return;
    // Focus never goes backwards on the page: not up without moving on to the next column.
    const up = s.y + s.height <= prev.y + 2;
    const onward = rtl ? s.x + s.width < prev.x - 4 : s.x > prev.x + prev.width + 4;
    if (up && !onward) out.push(`${where}: focus jumps back up from ${prev.what} to ${s.what}`);
  });
  return out;
}
