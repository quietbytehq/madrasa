import { execFileSync } from "node:child_process";
import type { Download, Page } from "@playwright/test";

/** The text of a PDF, as a reader can select it (poppler's pdftotext, layout kept). */
export function pdfText(path: string): string {
  return execFileSync("pdftotext", ["-layout", "-enc", "UTF-8", path, "-"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
}

/** Number of pages in a PDF. */
export function pdfPages(path: string): number {
  const info = execFileSync("pdfinfo", [path], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return Number(/Pages:\s+(\d+)/.exec(info)?.[1] ?? 0);
}

/** Click what starts a download and return the saved file. */
export async function downloadFrom(
  page: Page,
  click: () => Promise<void>,
): Promise<{ download: Download; path: string }> {
  const [download] = await Promise.all([page.waitForEvent("download"), click()]);
  const path = await download.path();
  if (!path) throw new Error("the download was not saved");
  return { download, path };
}
