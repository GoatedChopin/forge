# Benchmarks

Adaptive load testing that finds the practical ceiling for real-time users.

The benchmark treats a "concurrent user" as an active SSE session with a live query subscription. Each user keeps `/_api/events` open, subscribes, and continues making RPC calls while the controller ramps slowly. Each load level is held for `60s`, then the run checks `p90` latency for that level. By default the run is unlimited and stops automatically when `p90` exceeds `2000ms` or error rate exceeds `2%`. You can optionally add a max duration.

## Default Topology

Local runs start:

- 1 PostgreSQL primary
- 2 PostgreSQL read replicas
- 3 Forge instances

The Forge instances read from replicas for query traffic, while writes still go to the primary.

## Benchmark app

The `benchmarks/app/` directory contains a small Forge app plus one Rust load generator and one shell launcher.

```bash
# full local run: 1 primary, 2 replicas, 3 Forge instances
./benchmarks/app/run.sh

# same run, but stop after 30 minutes if thresholds do not trigger first
./benchmarks/app/run.sh --max-duration 30m

# external database run, e.g. AlloyDB primary + 2 read replicas
./benchmarks/app/run.sh \
  --database-url 'postgres://primary/app?sslmode=require' \
  --replica-url 'postgres://replica-1/app?sslmode=require' \
  --replica-url 'postgres://replica-2/app?sslmode=require'

# external Forge run, e.g. a GCP load balancer or explicit Forge VM URLs
./benchmarks/app/run.sh --forge-url 'https://forge-bench.example.com'
./benchmarks/app/run.sh \
  --forge-url 'http://vm-1:9081' \
  --forge-url 'http://vm-2:9081' \
  --forge-url 'http://vm-3:9081'
```

## Prerequisites

- Rust toolchain
- Docker for local PostgreSQL runs
