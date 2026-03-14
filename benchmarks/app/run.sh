#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INFRA_DIR="$SCRIPT_DIR/infra"
BINARY="$ROOT/target/release/forge-bench"

# Defaults
DB_URL="postgres://postgres:postgres@localhost:5432/app"
REPLICA_URLS='["postgres://postgres:postgres@localhost:5433/app","postgres://postgres:postgres@localhost:5434/app"]'
DURATION="${1:-5m}"

command -v k6 >/dev/null 2>&1 || { echo "Missing: k6"; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "Missing: python3"; exit 1; }
command -v docker >/dev/null 2>&1 || { echo "Missing: docker"; exit 1; }

echo "=== Building release binary ==="
cargo build --release -p forge-bench

echo ""
echo "=== Starting PostgreSQL (1 primary + 2 replicas) ==="
docker compose -f "$INFRA_DIR/docker-compose.yml" down -v 2>/dev/null || true
docker compose -f "$INFRA_DIR/docker-compose.yml" up -d

# Wait for all PG instances
echo "Waiting for primary..."
for i in $(seq 1 30); do
  pg_isready -h localhost -p 5432 -U postgres 2>/dev/null && break
  sleep 1
done
echo "Waiting for replica1..."
for i in $(seq 1 60); do
  pg_isready -h localhost -p 5433 -U postgres 2>/dev/null && break
  sleep 2
done
echo "Waiting for replica2..."
for i in $(seq 1 60); do
  pg_isready -h localhost -p 5434 -U postgres 2>/dev/null && break
  sleep 2
done

echo "All PG instances ready"
echo ""

echo "=== Starting forge-bench app ==="
cd "$SCRIPT_DIR"
DATABASE_URL="$DB_URL" \
JWT_SECRET="bench-secret-not-for-production" \
RUST_LOG=error \
"$BINARY" 2>/dev/null &
APP_PID=$!

# Wait for app to be healthy
for i in $(seq 1 30); do
  if curl -sf http://localhost:8080/_api/health >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -sf http://localhost:8080/_api/health >/dev/null 2>&1; then
  echo "App failed to start"
  kill $APP_PID 2>/dev/null
  exit 1
fi

echo "App healthy"
echo ""

echo "=== Running adaptive benchmark ==="
python3 -u "$ROOT/benchmarks/adaptive.py" \
  --url http://localhost:8080 \
  --k6-script "$SCRIPT_DIR/bench.js" \
  --duration "$DURATION" \
  "${@:2}"

echo ""
echo "=== Cleanup ==="
kill $APP_PID 2>/dev/null || true
wait $APP_PID 2>/dev/null || true
docker compose -f "$INFRA_DIR/docker-compose.yml" down -v
