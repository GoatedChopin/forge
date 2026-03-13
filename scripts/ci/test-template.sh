#!/usr/bin/env bash
# Usage: test-template.sh <template> <forge-binary> <workspace-dir>
# Scaffolds a project, validates with forge check, then runs forge test.
set -euo pipefail

TEMPLATE="$1"
FORGE="$2"
WORKSPACE="$3"
SLUG=$(echo "$TEMPLATE" | tr '/' '-')
DIR="/tmp/test-project"

BACKEND_PID=""
FRONTEND_PID=""
cleanup() {
  [ -n "$FRONTEND_PID" ] && kill "$FRONTEND_PID" 2>/dev/null || true
  [ -n "$BACKEND_PID" ] && kill "$BACKEND_PID" 2>/dev/null || true
  docker stop forge-test-pg 2>/dev/null || true
}
trap cleanup EXIT

# rust-cache may restore target/ which creates this directory
rm -rf "$DIR"

echo "=== Scaffold ==="
"$FORGE" new "test-$SLUG" --template "$TEMPLATE" --output "$DIR" --no-lock --include-skill

# Patch npm packages to local source (Rust patches are handled by debug build)
if [ -f "$DIR/frontend/package.json" ] && grep -q '@forge-rs/svelte' "$DIR/frontend/package.json"; then
  jq --arg p "file:$WORKSPACE/packages/forge-svelte" '
    if .dependencies["@forge-rs/svelte"] then .dependencies["@forge-rs/svelte"] = $p
    elif .devDependencies["@forge-rs/svelte"] then .devDependencies["@forge-rs/svelte"] = $p
    else . end
  ' "$DIR/frontend/package.json" > "$DIR/frontend/package.json.tmp"
  mv "$DIR/frontend/package.json.tmp" "$DIR/frontend/package.json"
fi

cd "$DIR"

echo "=== Forge check ==="
"$FORGE" check

echo "=== Install Playwright (with system deps for CI) ==="
if [ -d "$DIR/frontend" ]; then
  cd "$DIR/frontend" && bun install && bunx playwright install chromium --with-deps && cd "$DIR"
fi

# Dioxus WASM apps: start dx serve early so the WASM compiles in parallel
# with the backend build. Without this, the first Playwright test times out
# waiting for WASM (60-90s compile on CI vs 30s test timeout).
IS_DIOXUS=false
if [ -f "$DIR/frontend/Dioxus.toml" ] || [ -f "$DIR/frontend/dioxus.toml" ]; then
  IS_DIOXUS=true
  echo "=== Start Dioxus frontend (WASM pre-compile) ==="
  cd "$DIR/frontend"
  [ -f .env.example ] && [ ! -f .env ] && cp .env.example .env
  bun run dev &
  FRONTEND_PID=$!
  cd "$DIR"
fi

echo "=== Start PostgreSQL ==="
DB_NAME=$(grep '^POSTGRES_DB=' .env | cut -d= -f2- || echo "test_db")
docker run -d --name forge-test-pg \
  -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=forge -e POSTGRES_DB="$DB_NAME" \
  -p 5432:5432 postgres:18
for i in $(seq 1 30); do
  docker exec forge-test-pg pg_isready -U postgres 2>/dev/null && break
  sleep 1
done

echo "=== Build & start backend ==="
cargo build --release
BINARY=$(find target/release -maxdepth 1 -name 'test-*' -type f -executable | head -1)

DATABASE_URL="postgres://postgres:forge@localhost:5432/$DB_NAME" \
  HOST=0.0.0.0 PORT=8080 RUST_LOG=warn \
  JWT_SECRET=test-secret-for-ci WEBHOOK_SECRET=test-webhook-secret \
  "$BINARY" &
BACKEND_PID=$!

for i in $(seq 1 180); do
  curl -sf http://localhost:8080/_api/health | grep -q "healthy" && break
  [ "$i" -eq 180 ] && { echo "Backend failed to start"; exit 1; }
  sleep 1
done

# Wait for Dioxus WASM to finish compiling. dx serve responds to HTTP
# immediately with a 404 placeholder while compiling. The real page with
# .wasm references only appears after the build completes (60-90s on CI).
if [ "$IS_DIOXUS" = true ]; then
  echo "=== Wait for Dioxus WASM build ==="
  for i in $(seq 1 300); do
    if curl -s http://localhost:5173 2>/dev/null | command grep -q 'wasm'; then
      echo "Dioxus WASM build complete"
      break
    fi
    [ "$i" -eq 300 ] && { echo "Dioxus WASM build timed out after 300s"; exit 1; }
    sleep 1
  done
fi

echo "=== Run forge test ==="
"$FORGE" test --skip-backend -- --fail-on-flaky-tests
