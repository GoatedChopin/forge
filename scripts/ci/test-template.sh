#!/usr/bin/env bash
# Usage: test-template.sh <template> <forge-binary> <workspace-dir>
# Scaffolds a project, validates with forge check, then runs forge test.
set -euo pipefail

TEMPLATE="$1"
FORGE="$2"
WORKSPACE="$3"
SLUG=$(echo "$TEMPLATE" | tr '/' '-')
DIR="/tmp/test-project-$SLUG-$(date +%s)-$$"
CONTAINER_NAME="forge-test-pg-$SLUG"
BACKEND_PORT=$((18080 + ($(printf '%s' "$SLUG" | cksum | awk '{print $1}') % 1000)))
API_URL="http://localhost:$BACKEND_PORT"

BACKEND_PID=""
FRONTEND_PID=""
stop_pid() {
  local pid="${1:-}"
  [ -n "$pid" ] || return 0
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}
cleanup() {
  stop_pid "$FRONTEND_PID"
  stop_pid "$BACKEND_PID"
  docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
  rm -rf "$DIR" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== Scaffold ==="
"$FORGE" new "test-$SLUG" --template "$TEMPLATE" --output "$DIR" --no-lock --include-skill

perl -0pi -e "s/\\[gateway\\]\\nport = 8080/[gateway]\\nport = $BACKEND_PORT/" "$DIR/forge.toml"

# Patch npm packages to local source (Rust patches are handled by debug build)
if [ -f "$DIR/frontend/package.json" ] && grep -q '@forge-rs/svelte' "$DIR/frontend/package.json"; then
  jq --arg p "file:$WORKSPACE/packages/forge-svelte" '
    if .dependencies["@forge-rs/svelte"] then .dependencies["@forge-rs/svelte"] = $p
    elif .devDependencies["@forge-rs/svelte"] then .devDependencies["@forge-rs/svelte"] = $p
    else . end
  ' "$DIR/frontend/package.json" > "$DIR/frontend/package.json.tmp"
  mv "$DIR/frontend/package.json.tmp" "$DIR/frontend/package.json"
fi

if [ -d "$DIR/frontend" ]; then
  printf 'PUBLIC_API_URL=%s\nVITE_API_URL=%s\n' "$API_URL" "$API_URL" > "$DIR/frontend/.env"
fi

cd "$DIR"

echo "=== Forge check ==="
"$FORGE" check

echo "=== Install Playwright (with system deps for CI) ==="
if [ -d "$DIR/frontend" ]; then
  cd "$DIR/frontend" && bun install && bunx playwright install chromium --with-deps && cd "$DIR"
fi

# Dioxus WASM apps: on Linux CI, start dx serve early so the WASM compiles
# in parallel with the backend build. On macOS this contends on the Cargo
# cache and is slower than letting Playwright manage the dev server.
IS_DIOXUS=false
PRESTART_DIOXUS=false
if [ -f "$DIR/frontend/Dioxus.toml" ] || [ -f "$DIR/frontend/dioxus.toml" ]; then
  IS_DIOXUS=true
  if [ "$(uname -s)" = "Linux" ]; then
    PRESTART_DIOXUS=true
  fi
fi

if [ "$PRESTART_DIOXUS" = true ]; then
  echo "=== Start Dioxus frontend (WASM pre-compile) ==="
  cd "$DIR/frontend"
  [ -f .env.example ] && [ ! -f .env ] && cp .env.example .env
  FORGE_API_URL="$API_URL" bun run dev &
  FRONTEND_PID=$!
  cd "$DIR"
fi

echo "=== Start PostgreSQL ==="
if [ -f .env ]; then
  DB_NAME=$(grep '^POSTGRES_DB=' .env | cut -d= -f2- || true)
fi
DB_NAME=${DB_NAME:-test_db}
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true
docker run -d --name "$CONTAINER_NAME" \
  -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=forge -e POSTGRES_DB="$DB_NAME" \
  -p 5432:5432 postgres:18
for i in $(seq 1 30); do
  docker exec "$CONTAINER_NAME" pg_isready -U postgres 2>/dev/null && break
  sleep 1
done

echo "=== Build & start backend ==="
cargo build --release
BINARY=""
for candidate in target/release/test-*; do
  if [ -f "$candidate" ] && [ -x "$candidate" ]; then
    BINARY="$candidate"
    break
  fi
done
[ -n "$BINARY" ] || { echo "Built backend binary not found"; exit 1; }

DATABASE_URL="postgres://postgres:forge@localhost:5432/$DB_NAME" \
  HOST=0.0.0.0 PORT="$BACKEND_PORT" RUST_LOG=warn \
  JWT_SECRET=test-secret-for-ci WEBHOOK_SECRET=test-webhook-secret \
  "$BINARY" &
BACKEND_PID=$!

for i in $(seq 1 180); do
  curl -sf "$API_URL/_api/health" | grep -q "healthy" && break
  [ "$i" -eq 180 ] && { echo "Backend failed to start"; exit 1; }
  sleep 1
done

# Wait for Dioxus to finish compiling. dx serve responds to HTTP
# immediately with a placeholder page while compiling. Once the real app
# is ready, that placeholder disappears.
if [ "$PRESTART_DIOXUS" = true ]; then
  echo "=== Wait for Dioxus WASM build ==="
  for i in $(seq 1 300); do
    PAGE=$(curl -sf http://localhost:5173 2>/dev/null || true)
    if [ -n "$PAGE" ] \
      && ! printf '%s' "$PAGE" | command grep -q 'Forge Dioxus Dev Placeholder' \
      && ! printf '%s' "$PAGE" | command grep -q 'Err 404 - dx is not serving a web app' \
      && ! printf '%s' "$PAGE" | command grep -q "One sec! We're building your app now."; then
      echo "Dioxus WASM build complete"
      break
    fi
    [ "$i" -eq 300 ] && { echo "Dioxus WASM build timed out after 300s"; exit 1; }
    sleep 1
  done
fi

echo "=== Run forge test ==="
CI=true FORGE_API_URL="$API_URL" VITE_API_URL="$API_URL" PUBLIC_API_URL="$API_URL" \
  "$FORGE" test --skip-backend -- --fail-on-flaky-tests
