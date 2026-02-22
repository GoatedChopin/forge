import { defineConfig, devices } from "@playwright/test";

const FRONTEND_PORT = 5173;

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  globalSetup: "./tests/global-setup.ts",
  timeout: 60_000,
  expect: {
    timeout: process.env.CI ? 15_000 : 10_000,
  },
  use: {
    baseURL: `http://localhost:${FRONTEND_PORT}`,
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `docker compose up --build`,
    url: `http://localhost:${FRONTEND_PORT}`,
    cwd: "..",
    reuseExistingServer: !!process.env.CI,
    timeout: 10 * 60 * 1000,
  },
});
