import { defineConfig, devices } from "@playwright/test";

const skipWebServer = process.env.SKIP_PLAYWRIGHT_WEBSERVER === "1";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  globalSetup: "./tests/global-setup.ts",
  use: {
    baseURL: "http://localhost:5173",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: skipWebServer
    ? undefined
    : {
        command: "bun run dev",
        url: "http://localhost:5173",
        reuseExistingServer: true,
        timeout: 120 * 1000,
      },
});
