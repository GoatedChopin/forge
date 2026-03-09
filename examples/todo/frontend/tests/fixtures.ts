import { test as base, expect, type Page } from "@playwright/test";

export { expect };

export const API_URL = process.env.VITE_API_URL || "http://localhost:8080";
export const ACTION_TIMEOUT = process.env.CI ? 10_000 : 5_000;

export function uniqueId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function trackConsoleErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      const text = msg.text();
      if (
        !text.includes("net::ERR") &&
        !text.includes("favicon") &&
        !text.includes("EventSource")
      ) {
        errors.push(text);
      }
    }
  });
  return errors;
}

type ForgeFixtures = {
  rpc: (fn: string, args?: unknown) => Promise<unknown>;
  gotoReady: (path?: string) => Promise<void>;
};

export const test = base.extend<ForgeFixtures>({
  rpc: async ({}, use) => {
    await use(async (fn: string, args: unknown = null) => {
      const res = await fetch(`${API_URL}/_api/rpc/${fn}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ args }),
      });
      return (await res.json()).data;
    });
  },

  gotoReady: async ({ page }, use) => {
    await use(async (path = "/") => {
      // Wait for the first subscription registration response, not just the
      // SSE connection. This is the actual signal that reactivity is wired up.
      const subscribed = page.waitForResponse(
        (res) =>
          res.url().includes("/_api/subscribe") && res.status() === 200,
        { timeout: 15_000 },
      );
      await page.goto(path);
      await subscribed;
    });
  },
});
