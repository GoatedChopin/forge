import type { FullConfig } from "@playwright/test";

const API_URL = process.env.PUBLIC_API_URL || "http://localhost:8080";

async function waitForBackend(maxRetries = 60, delayMs = 1000): Promise<void> {
  for (let i = 0; i < maxRetries; i += 1) {
    try {
      const response = await fetch(`${API_URL}/_api/ready`);
      if (response.ok) {
        return;
      }
    } catch {
      // Backend not ready yet.
    }
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  throw new Error(`Backend did not become ready in time (${API_URL})`);
}

export default async function globalSetup(_config: FullConfig) {
  await waitForBackend();
}
