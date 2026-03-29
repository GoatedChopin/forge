import type { FullConfig } from "@playwright/test";

const API_URL = process.env.VITE_API_URL || "http://localhost:9081";
const FRONTEND_URL = "http://localhost:9080";

async function waitForBackend(maxRetries = 60, delayMs = 1000): Promise<void> {
  for (let i = 0; i < maxRetries; i++) {
    try {
      const response = await fetch(`${API_URL}/_api/health`);
      if (response.ok) {
        console.log("Backend is ready");
        return;
      }
    } catch {
      // Backend not ready yet
    }
    if (i % 10 === 0) {
      console.log(`Waiting for backend... (${i + 1}/${maxRetries})`);
    }
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  throw new Error(`Backend did not become ready at ${API_URL}`);
}

// Dioxus WASM apps compile on first request. Hit the frontend once to
// trigger compilation so tests don't eat the cold-build penalty.
async function warmupWasm(maxRetries = 120, delayMs = 2000): Promise<void> {
  console.log("Warming up WASM (first load triggers compilation)...");
  for (let i = 0; i < maxRetries; i++) {
    try {
      const response = await fetch(FRONTEND_URL);
      if (response.ok) {
        const html = await response.text();
        // dx serve returns the shell page with the wasm loader on first compile,
        // and the compiled app on subsequent loads. Either way, a 200 means
        // the dev server processed the build.
        if (html.includes("wasm") || html.includes("dioxus")) {
          console.log("WASM build complete, frontend is ready");
          return;
        }
      }
    } catch {
      // Dev server not ready yet
    }
    if (i % 10 === 0 && i > 0) {
      console.log(`  Still compiling WASM... (${i * 2}s elapsed)`);
    }
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  throw new Error("WASM build did not complete in time");
}

export default async function globalSetup(_config: FullConfig) {
  await waitForBackend();
  await warmupWasm();
}
