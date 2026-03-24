import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  globalSetup: "./tests/global-setup.ts",
  // WASM cold start on CI can take 60-90s (download + compile + init)
  timeout: process.env.CI ? 120_000 : 30_000,
  use: {
    baseURL: "http://localhost:9080",
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "bun run dev",
    url: "http://localhost:9080",
    reuseExistingServer: true,
    timeout: 120 * 1000,
  },
});
