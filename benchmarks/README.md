# Benchmarks

Adaptive load test that ramps concurrent SSE users until the system breaks. Stops when p90 latency exceeds 2,000ms or error rate exceeds 2%.

Each user holds a live SSE subscription, authenticates with JWT, and makes 10 RPC calls per second (50% scoped reads, 20% paginated list, 30% atomic writes). Writes trigger the full reactivity pipeline: `NOTIFY`, invalidation debouncing, query re-execution, hash comparison, and SSE fan-out.

See the [performance docs](https://tryforge.dev/docs/scale/performance) for scaling analysis and cloud projections.

## Results

<!-- Add new rows at the bottom. Keep postgresql.conf tuning consistent for valid comparison. -->

| Date | Hardware | PG | Forge instances | Pool size | Peak req/s | Peak concurrent | p90 at peak | Stop reason |
|------|----------|----|-----------------|-----------|------------|-----------------|-------------|-------------|
| 2026-04-11 | 12-core laptop, 18GB, Docker | 18 | 2 | 40/instance | 12,535 | 2,250 | 46ms | errors 8.46% at 2,450 |

<details>
<summary>2026-04-11 full output</summary>

```
  users | active |    req/s |    p50    p90    p99 |  err %
──────────────────────────────────────────────────────────
    250 |    250 |     2373 |     2ms     9ms    41ms |  0.00%
    450 |    450 |     4324 |     1ms     4ms    11ms |  0.00%
    650 |    650 |     6251 |     1ms     4ms    11ms |  0.00%
    850 |    850 |     8155 |     1ms     3ms    33ms |  0.00%
   1050 |   1050 |    10022 |     1ms     3ms    69ms |  0.00%
   1250 |   1250 |    11659 |     1ms    14ms    74ms |  0.00%
   1450 |   1450 |    12535 |     2ms    46ms   110ms |  0.00%
   1650 |   1650 |    10980 |    27ms   128ms   265ms |  0.00%
   1850 |   1850 |    11533 |    35ms   151ms   290ms |  0.00%
   2050 |   2050 |    12203 |    47ms   165ms   266ms |  0.00%
   2250 |   2250 |    10962 |    85ms   248ms   404ms |  0.00%
   2450 |   2450 |     9065 |    85ms   282ms  2128ms |  8.46%
```

</details>

## How it works

The load generator (`src/bin/loadgen.rs`) is a Rust binary that simulates users. Each user:

1. Registers and gets a JWT token
2. Opens an SSE connection to `/_api/events`
3. Subscribes to a specific counter via `/_api/subscribe`
4. Loops: makes an RPC call every 100ms, records latency

The controller starts with 50 warmup users (results discarded), then adds 200 users per level. Each level is held for 60 seconds while latency and errors are measured. Latency samples are collected across 16 shards to avoid mutex contention, and percentiles use `select_nth_unstable` (O(n) instead of sorting).

The benchmark app itself (`src/main.rs`) registers five handlers against two tables:

| Handler | Type | SQL |
|---------|------|-----|
| `register` | mutation (public) | INSERT into users, return JWT |
| `create_counter` | mutation (public) | INSERT into counters |
| `increment` | mutation | UPDATE counter value + 1, row lock |
| `get_counter` | query | SELECT by primary key |
| `list_counters` | query | SELECT with LIMIT 20 |

The counters table has reactivity enabled (`forge_enable_reactivity('counters')`), so every write triggers `NOTIFY` and the full invalidation pipeline.

## Running it

```bash
# full local run (starts Docker PG + 2 Forge instances)
./benchmarks/app/run.sh

# stop after 30 minutes if thresholds don't trigger first
./benchmarks/app/run.sh --max-duration 30m
```

Needs Rust 1.92+ and Docker. The script builds release binaries, starts PostgreSQL (1 primary + 1 read replica), starts 2 Forge instances, warms up, ramps, and tears everything down on exit.

### External infrastructure

```bash
# your own database
./benchmarks/app/run.sh \
  --database-url 'postgres://user:pass@primary/app' \
  --replica-url 'postgres://user:pass@replica/app'

# your own Forge instances (skip local DB and Forge startup)
./benchmarks/app/run.sh \
  --forge-url 'http://10.0.1.10:9081' \
  --forge-url 'http://10.0.1.11:9081'
```

### Comparing Postgres versions

Change the image tag in `infra/docker-compose.yml`:

```yaml
image: postgres:16  # or 17, 18
```

Keep `infra/primary/postgresql.conf` the same across runs.

## Configuration

| Parameter | Default | Where |
|-----------|---------|-------|
| `FORGE_INSTANCES` | 2 | run.sh |
| `POOL_SIZE` | 40 | run.sh |
| `GATEWAY_MAX_CONNECTIONS` | 16000 | run.sh |
| `SSE_MAX_SESSIONS` | 12000 | run.sh |
| Warmup users | 50 | loadgen |
| Ramp step | 200 | loadgen |
| Level hold | 60s | loadgen |
| Action interval | 100ms | loadgen |
| Counter count | 128 | loadgen |
| P90 limit | 2000ms | loadgen |
| Error threshold | 2% | loadgen |
