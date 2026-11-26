import type { Page } from "@playwright/test";

export interface Clipped {
  what: string;
  text: string;
  why: string;
}

/**
 * Text a reader cannot see in full: cut by a box that hides its overflow, or running off the
 * side of the window. Text shortened on purpose with an ellipsis is allowed in table cells and
 * lists (the full text is one click away), but never on the controls people act on: buttons,
 * tabs, menu items, headings, field labels and column headings.
 */
export async function clippedText(page: Page): Promise<Clipped[]> {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    const out: { what: string; text: string; why: string }[] = [];
    const control = "button,[role=tab],[role=menuitem],h1,h2,h3,label,th,nav a,legend";
    const measure = document.createElement("canvas").getContext("2d")!;
    /** Where the drawn letters of a text node start and end, top to bottom. */
    const ink = (node: Text, el: Element): [number, number] => {
      const s = getComputedStyle(el);
      measure.font = `${s.fontStyle} ${s.fontWeight} ${s.fontSize} ${s.fontFamily}`;
      const m = measure.measureText(node.textContent ?? "");
      const range = document.createRange();
      range.selectNodeContents(node);
      const lines = [...range.getClientRects()].filter((r) => r.width > 0);
      if (!lines.length) return [0, 0];
      // A line's box starts one font-ascent above its baseline.
      const first = Math.min(...lines.map((r) => r.top)) + m.fontBoundingBoxAscent;
      const last = Math.max(...lines.map((r) => r.top)) + m.fontBoundingBoxAscent;
      return [first - m.actualBoundingBoxAscent, last + m.actualBoundingBoxDescent];
    };
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const seen = new Set<Element>();
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = (node.textContent ?? "").trim();
      const el = node.parentElement;
      if (!text || !el || seen.has(el)) continue;
      const style = getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") continue;
      // Hidden from sight on purpose (labels for screen readers).
      if (el.closest(".sr-only") || style.clip === "rect(0px, 0px, 0px, 0px)") continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const box = range.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      seen.add(el);
      const what = `${el.tagName.toLowerCase()}${el.closest(control) ? " (control)" : ""}`;
      const short = text.slice(0, 60);

      // The nearest ancestor that hides what does not fit.
      let clip: Element | null = el;
      let scrolls = false;
      for (; clip && clip !== document.documentElement; clip = clip.parentElement) {
        const s = getComputedStyle(clip);
        const x = s.overflowX;
        const y = s.overflowY;
        if (x === "auto" || x === "scroll" || y === "auto" || y === "scroll") {
          scrolls = true; // the reader can scroll to it
          break;
        }
        if (x === "hidden" || x === "clip" || y === "hidden" || y === "clip") break;
      }
      if (clip && clip !== document.documentElement && !scrolls) {
        const s = getComputedStyle(clip);
        const c = clip.getBoundingClientRect();
        const wide =
          (s.overflowX === "hidden" || s.overflowX === "clip") &&
          (box.left < c.left - 1 || box.right > c.right + 1);
        // Top and bottom are judged by the ink, not the line box: Nastaliq's line box is far
        // taller than its letters, and a short box around it cuts nothing.
        const [top, bottom] = ink(node as Text, el);
        const tall =
          (s.overflowY === "hidden" || s.overflowY === "clip") &&
          (top < c.top - 1 || bottom > c.bottom + 1);
        if (wide || tall) {
          const ellipsis =
            s.textOverflow === "ellipsis" || getComputedStyle(el).textOverflow === "ellipsis";
          if (!ellipsis || el.closest(control)) {
            out.push({
              what,
              text: short,
              why: wide
                ? ellipsis
                  ? "shortened with …"
                  : "cut at the side"
                : "cut at the top or bottom",
            });
            continue;
          }
        }
      }
      // Off the side of the window, with nothing to scroll.
      if (!scrolls && (box.right > window.innerWidth + 1 || box.left < -1)) {
        out.push({ what, text: short, why: "runs off the window" });
      }
    }
    if (document.documentElement.scrollWidth > window.innerWidth + 1) {
      out.push({
        what: "page",
        text: "",
        why: `the page scrolls sideways (${document.documentElement.scrollWidth}px wide)`,
      });
    }
    return out;
  });
}

export interface LowContrast {
  text: string;
  ratio: number;
  needs: number;
  color: string;
  on: string;
}

/**
 * Text whose contrast with what is behind it is below WCAG AA (4.5:1, or 3:1 for large text).
 * Disabled controls and placeholders are exempt, as in WCAG.
 */
export async function lowContrast(page: Page): Promise<LowContrast[]> {
  await page.evaluate(() => document.fonts.ready);
  return page.evaluate(() => {
    type Rgba = [number, number, number, number];
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    const parse = (css: string): Rgba => {
      // Through a canvas, so every colour syntax the browser accepts is read the same way.
      ctx.clearRect(0, 0, 1, 1);
      ctx.fillStyle = "#000";
      ctx.fillStyle = css;
      ctx.fillRect(0, 0, 1, 1);
      const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
      return [r, g, b, a / 255];
    };
    const over = (top: Rgba, under: Rgba): Rgba => {
      const a = top[3] + under[3] * (1 - top[3]);
      if (a === 0) return [0, 0, 0, 0];
      const mix = (i: 0 | 1 | 2) => (top[i] * top[3] + under[i] * under[3] * (1 - top[3])) / a;
      return [mix(0), mix(1), mix(2), a];
    };
    const luminance = ([r, g, b]: Rgba) => {
      const f = (v: number) => {
        const c = v / 255;
        return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
    };
    const background = (el: Element): Rgba | null => {
      let colour: Rgba = [0, 0, 0, 0];
      for (let e: Element | null = el; e; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.backgroundImage !== "none") return null; // a picture or gradient: not measured
        colour = over(colour, parse(s.backgroundColor));
        if (colour[3] >= 0.999) return colour;
      }
      return over(colour, [255, 255, 255, 1]);
    };
    const hex = (c: Rgba) =>
      "#" +
      c
        .slice(0, 3)
        .map((v) => Math.round(v).toString(16).padStart(2, "0"))
        .join("");

    const found = new Map<
      string,
      { text: string; ratio: number; needs: number; color: string; on: string }
    >();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = (node.textContent ?? "").trim();
      const el = node.parentElement;
      if (!text || !el) continue;
      const s = getComputedStyle(el);
      if (s.visibility === "hidden" || s.display === "none" || Number(s.opacity) === 0) continue;
      if (el.closest(".sr-only,[aria-hidden=true],:disabled,[aria-disabled=true]")) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const box = range.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) continue;
      const bg = background(el);
      if (!bg) continue;
      // Opacity on the way up fades the text toward the background.
      let alpha = 1;
      for (let e: Element | null = el; e; e = e.parentElement)
        alpha *= Number(getComputedStyle(e).opacity);
      const fgRaw = parse(s.color);
      const fg = over([fgRaw[0], fgRaw[1], fgRaw[2], fgRaw[3] * alpha], bg);
      const [hi, lo] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
      const ratio = (hi + 0.05) / (lo + 0.05);
      const px = parseFloat(s.fontSize);
      const bold = Number(s.fontWeight) >= 700;
      const needs = px >= 24 || (bold && px >= 18.66) ? 3 : 4.5;
      if (ratio + 0.005 < needs) {
        const key = `${hex(fg)} on ${hex(bg)}`;
        if (!found.has(key)) {
          found.set(key, {
            text: text.slice(0, 50),
            ratio: Math.round(ratio * 100) / 100,
            needs,
            color: hex(fg),
            on: hex(bg),
          });
        }
      }
    }
    return [...found.values()];
  });
}
