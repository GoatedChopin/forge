
import { getContext, setContext } from "svelte";
import type { ForgeSignals } from "./signals.js";

const FORGE_SIGNALS_KEY = Symbol("forge-signals");
let globalSignals: ForgeSignals | null = null;

export function getForgeSignals(): ForgeSignals {
  try {
    const signals = getContext<ForgeSignals>(FORGE_SIGNALS_KEY);
    if (signals) return signals;
  } catch {}
  if (globalSignals) return globalSignals;
  throw new Error(
    "FORGE signals not found. Wrap your component with ForgeProvider.",
  );
}

export function setForgeSignals(signals: ForgeSignals): void {
  setContext(FORGE_SIGNALS_KEY, signals);
  globalSignals = signals;
}
