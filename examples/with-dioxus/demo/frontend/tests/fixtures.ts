import { test as base, expect, type Page } from "@playwright/test";

export { expect };

export const API_URL = process.env.VITE_API_URL || "http://localhost:9081";
export const ACTION_TIMEOUT = process.env.CI ? 30_000 : 30_000;

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
  // eslint-disable-next-line no-empty-pattern
  rpc: async ({}, use) => {
    await use(async (fn: string, args: unknown = null) => {
      const res = await fetch(`${API_URL}/_api/rpc/${fn}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ args }),
      });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`RPC ${fn} failed (${res.status}): ${body}`);
      }
      return (await res.json()).data;
    });
  },

  gotoReady: async ({ page }, use) => {
    await use(async (path = "/") => {
      // Wait for the subscription registration response, not just the SSE
      // connection. This signals that reactivity is fully wired up.
      // WASM apps need extra time: download → instantiate → init → SSE → subscribe.
      // Global setup pre-compiles the WASM, but the browser still has to download
      // and instantiate the binary on each page load.
      const subscribed = page.waitForResponse(
        (res) => res.url().includes("/_api/subscribe") && res.status() === 200,
        { timeout: 90_000 },
      );
      await page.goto(path);
      await subscribed;
    });
  },
});
