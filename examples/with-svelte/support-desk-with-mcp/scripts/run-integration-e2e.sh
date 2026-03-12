#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORGE_BIN_DEFAULT="$ROOT_DIR/../../target/debug/forge"
FORGE_BIN="${FORGE_BIN:-$FORGE_BIN_DEFAULT}"
DEV_LOG="$ROOT_DIR/.forge-dev-integration.log"

if [[ ! -x "$FORGE_BIN" ]]; then
  echo "Forge binary not found at $FORGE_BIN"
  exit 1
fi

cleanup() {
  if [[ -n "${DEV_PID:-}" ]]; then
    kill -TERM "$DEV_PID" >/dev/null 2>&1 || true
    pkill -TERM -P "$DEV_PID" >/dev/null 2>&1 || true
    wait "$DEV_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

wait_for_url() {
  local url="$1"
  local label="$2"
  local retries="${3:-120}"

  for _ in $(seq 1 "$retries"); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "$label is ready: $url"
      return 0
    fi
    sleep 1
  done

  echo "Timed out waiting for $label at $url"
  return 1
}

echo "Starting forge dev (logs: $DEV_LOG)"
cd "$ROOT_DIR"
"$FORGE_BIN" dev down --clear >/dev/null 2>&1 || true
"$FORGE_BIN" dev --takeover-ports >"$DEV_LOG" 2>&1 &
DEV_PID=$!

wait_for_url "http://localhost:8080/_api/health" "Backend"
wait_for_url "http://localhost:5173" "Frontend"

echo "Running Playwright support inbox test"
cd "$ROOT_DIR/frontend"
SKIP_PLAYWRIGHT_WEBSERVER=1 bun run test tests/support-inbox.spec.ts

echo "Running MCP integration script"
cd "$ROOT_DIR"
bun scripts/test-mcp.ts

echo "Integration suite passed"
