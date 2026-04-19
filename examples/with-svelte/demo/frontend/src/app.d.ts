import type { ForgeSignals } from "@forge-rs/svelte";

declare global {
  interface Window {
    // Exposed in +layout.svelte so Playwright specs can drive the SDK.
    forgeSignals: ForgeSignals;
  }
}

export {};
