# Benchmarks

Adaptive load testing that finds the sustainable throughput ceiling.

The controller ramps concurrent users until latency exceeds 1.5s or error rate is too high, then stops. This finds the real ceiling, not just where things start to wobble.

## Results

| Date | Version | Setup | Concurrent Users | Peak Throughput | Latency | Error Rate | Notes |
|------|---------|-------|-----------------|-----------------|---------|------------|-------|
| 2026-03-16 | 0.7.1 | M3 Pro 18GB, PG 18 tuned, pool=30 | 8,510 | 21,728 req/s | 553ms | 0.00% | Pool saturated at ceiling, 0% errors throughout |

Config: `pool_size=30`, `test_before_acquire=false`, `log_statements=Off`, `synchronous_commit=off`, `max_connections=200`, release binary with LTO + codegen-units=1.

30% of traffic is writes that trigger PG NOTIFY for reactivity. Sustained 0.00% error rate throughout.

## Benchmark app

The `benchmarks/app/` directory contains a small app that hammers a single `counters` table with concurrent reads and writes while reactivity (PG NOTIFY) is active. It spins up a Dockerized Postgres cluster (1 primary, 2 replicas) so the test is self-contained.

```bash
# full run: builds release binary, starts PG cluster, runs adaptive load test
./benchmarks/app/run.sh 8m

# custom duration and params
./benchmarks/app/run.sh 5m --ramp-step 100 --max-vus 5000
```

## Prerequisites

- [k6](https://grafana.com/docs/k6/latest/set-up/install-k6/) for RPC load testing
- Python 3 (stdlib only, no pip installs)
- Docker (for the benchmark app's PG cluster)
