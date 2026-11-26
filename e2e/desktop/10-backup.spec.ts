import { test, expect } from "@playwright/test";
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { call, tokenFor } from "../support/api";
import { openDesktop } from "../support/desktop";

// Pilot stage 4: every backup is checked by restoring it into a temporary database, and a
// backup that fails is said in red where everyone sees it.
const WORK = process.env.E2E_WORK ?? "/tmp";

async function useFolder(directory: string) {
  await call("/backup/settings", {
    method: "PUT",
    token: await tokenFor("admin"),
    body: { enabled: true, time: "02:00", directory, keep: 5 },
  });
}

test.describe("Backup", () => {
  test("Back up now writes the file, then says it was checked by a restore", async ({
    browser,
  }) => {
    const folder = path.join(WORK, "backups-good");
    mkdirSync(folder, { recursive: true });
    await useFolder(folder);
    const { page, context } = await openDesktop(browser, { route: "/backup" });
    await page.getByRole("button", { name: "Back up now" }).click();
    await expect(page.getByText(/^Backup written to /)).toBeVisible({ timeout: 30_000 });
    expect(readdirSync(folder).filter((f) => f.endsWith(".mbk"))).toHaveLength(1);
    // The check follows on its own; the page asks until it is in.
    const line = page.getByTestId("backup-check");
    await expect(line).toHaveText(
      "Checked after writing: restored into a temporary database and every record counted.",
      { timeout: 90_000 },
    );
    await expect(line).toHaveAttribute("title", /all \d+ records are there/);
    await expect(page.getByTestId("backup-alert")).toHaveCount(0);
    await expect(page.getByText("Checked", { exact: true }).first()).toBeVisible();
    await context.close();
  });

  test("a backup that fails is a red alert on the dashboard, until the next good one", async ({
    browser,
  }) => {
    const token = await tokenFor("admin");
    // The folder was fine when it was chosen and is gone now (a USB drive that was pulled):
    // a file stands where it was.
    const gone = path.join(WORK, "backups-usb");
    mkdirSync(gone, { recursive: true });
    await useFolder(gone);
    rmSync(gone, { recursive: true });
    writeFileSync(gone, "not a folder");
    await expect(call("/backup/run", { method: "POST", token, body: {} })).rejects.toThrow(/409/);
    const { page, context } = await openDesktop(browser, { route: "/" });
    const alert = page.getByTestId("backup-alert");
    await expect(alert).toBeVisible();
    await expect(alert).toContainText("The last backup failed");
    await expect(alert).toContainText("The backup folder cannot be used");
    await expect(alert).toContainText(/The last good backup is from|There is no good backup yet/);
    await expect(alert).toContainText("then press Back up now");
    await alert.getByRole("link", { name: "Open backup" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Backup" })).toBeVisible();
    await expect(page.getByTestId("backup-alert")).toBeVisible();

    // Put right: the alert goes with the next good backup.
    const folder = path.join(WORK, "backups-again");
    mkdirSync(folder, { recursive: true });
    await useFolder(folder);
    await page.getByRole("button", { name: "Back up now" }).click();
    await expect(page.getByText(/^Backup written to /)).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("backup-alert")).toHaveCount(0);
    await context.close();
  });
});
