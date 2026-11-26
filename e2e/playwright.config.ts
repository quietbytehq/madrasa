import { defineConfig } from "@playwright/test";

// Run through scripts/e2e.sh, which starts the servers these tests talk to.
// E2E_CHROMIUM names another Chromium to use instead of the one Playwright installed.
const executablePath = process.env.E2E_CHROMIUM;

export default defineConfig({
  testDir: ".",
  outputDir: "test-results",
  // One school, one hub, shared by every test: one at a time, in file order.
  workers: 1,
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: process.env.CI
    ? [["list"], ["html", { outputFolder: "report", open: "never" }]]
    : [["list"]],
  use: {
    screenshot: "only-on-failure",
    trace: "off",
    video: "off",
    colorScheme: "light",
    locale: "en-PK",
    launchOptions: executablePath ? { executablePath } : {},
  },
  projects: [
    {
      name: "desktop",
      testDir: "desktop",
      use: { viewport: { width: 1366, height: 768 } },
    },
    {
      name: "parent",
      testDir: "parent",
      use: {
        viewport: { width: 390, height: 844 },
        isMobile: true,
        hasTouch: true,
        deviceScaleFactor: 2,
      },
    },
    {
      name: "console",
      testDir: "console",
      use: { viewport: { width: 1366, height: 768 } },
    },
  ],
});
