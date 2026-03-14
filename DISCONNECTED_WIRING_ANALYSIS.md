# Disconnected Wiring Analysis

Comprehensive analysis of places where infrastructure/wiring exists but
implementation is missing or disconnected.

Generated: 2026-03-13

---

## Critical

### 1. Cluster Discovery is Completely Unimplemented

**Files:**
- `crates/forge-core/src/config/cluster.rs` (DiscoveryMethod enum, lines 58-71)
- `crates/forge/src/runtime.rs` (hardcoded defaults, line 314)
- `crates/forge-runtime/src/cluster/heartbeat.rs`
- `crates/forge-runtime/src/cluster/registry.rs`

**What exists:**
- `DiscoveryMethod` enum with 4 variants: `Postgres`, `Dns`, `Kubernetes`, `Static`
- `ClusterConfig` struct with fields: `discovery`, `seed_nodes`, `dns_name`,
  `heartbeat_interval_secs`, `dead_threshold_secs`
- TOML config parsing and serialization
- Unit tests verifying config parsing

**What's missing:**
- `config.cluster.discovery` is never read at runtime
- `config.cluster.seed_nodes` is never accessed
- `config.cluster.dns_name` is never accessed
- `config.cluster.heartbeat_interval_secs` is never used (hardcoded to 5s)
- `config.cluster.dead_threshold_secs` is never used (hardcoded to 15s)
- Discovery is hardcoded to PostgreSQL-only (`forge_nodes` table) regardless of
  the configured `DiscoveryMethod`
- No DNS-based, Kubernetes-based, or static seed node discovery exists

---

## Medium

### 2. Analytics Pool — Fully Built, Never Plugged In

**File:** `crates/forge-runtime/src/db/pool.rs`

**What exists:**
- `analytics_pool: Option<Arc<PgPool>>` field (line 30)
- Created via `create_isolated_pool()` (lines 96-98)
- Public accessor `pub fn analytics_pool()` (lines 189-191)
- Proper cleanup on shutdown (lines 257-259)
- Documented in 4 docs files as available for user code

**What's missing:**
- Zero calls to `.analytics_pool()` anywhere in the codebase
- Contrast: `jobs_pool()` is used in 6+ places, `observability_pool()` in metrics

### 3. Batch RPC — Struct Defined, Never Routed

**File:** `crates/forge-runtime/src/gateway/request.rs` (lines 23-29)

**What exists:**
- `BatchRpcRequest` struct with `#[allow(dead_code)]`
- Passing test `test_batch_request()` (lines 53-57)

**What's missing:**
- Not exported from gateway module (`mod.rs` line 16 only exports `RpcRequest`)
- No batch route in `server.rs`
- No batch handler in `rpc.rs`
- No batch response type
- No batch execution logic

### 4. WorkflowScheduler.event_store — Stored, Never Read

**File:** `crates/forge-runtime/src/workflow/scheduler.rs` (lines 42-43)

**What exists:**
- `event_store: Arc<EventStore>` field with `#[allow(dead_code)]`
- Passed into constructor and stored (line 58)

**What's missing:**
- Zero references to `self.event_store` in any scheduler method
- Workflow event emission was designed but never integrated

### 5. GracefulShutdown.leader_election — Stored, Never Accessed

**File:** `crates/forge-runtime/src/cluster/shutdown.rs` (lines 32-33)

**What exists:**
- `leader_election: Option<Arc<LeaderElection>>` with `#[allow(dead_code)]`
- Initialized in `new()` (line 50)

**What's missing:**
- Never referenced in any shutdown method
- Shutdown doesn't coordinate with leader election

---

## Low-Medium

### 6. InvalidationEngine.invalidation_rx — Channel With No Consumer

**File:** `crates/forge-runtime/src/realtime/invalidation.rs` (lines 60-61)

**What exists:**
- `invalidation_tx` IS used to send invalidation batches (line 155)
- `invalidation_rx` is stored with `#[allow(dead_code)]`

**What's missing:**
- Nothing ever reads from the receiver — messages go into a void

### 7. SecurityConfig.secret_key — Parsed, Never Used

**File:** `crates/forge-core/src/config/mod.rs` (line 334)

**What exists:**
- Field defined in `SecurityConfig`, parsed from config

**What's missing:**
- Zero references anywhere; auth uses `AuthConfig.jwt_secret` instead

### 8. coalesce_by_table Config — Defined, No Logic

**File:** `crates/forge-runtime/src/realtime/invalidation.rs` (line 24)

**What exists:**
- `coalesce_by_table: bool` in `InvalidationConfig` with default `true`

**What's missing:**
- Never referenced in any invalidation logic
- Table-level batching behavior was never implemented

---

## Low

### 9. Realtime Subscription IDs — Stored, Never Read

**File:** `crates/forge-runtime/src/realtime/reactor.rs` (lines 51-68)

Fields with `#[allow(dead_code)]`:
- `JobSubscription.subscription_id`
- `JobSubscription.job_id`
- `WorkflowSubscription.subscription_id`
- `WorkflowSubscription.workflow_id`

### 10. CodeGen output_dir Fields — Stored, Never Used

**Files:**
- `crates/forge-codegen/src/typescript/client.rs` (lines 7-8)
- `crates/forge-codegen/src/typescript/types.rs` (line 8)

### 11. Duration Deserializer — Defined, Never Called

**File:** `crates/forge-runtime/src/function/executor.rs` (lines 307-314)

The matching `serialize` IS used, but `deserialize` has `#[allow(dead_code)]`.

### 12. TestContext.query() and .mutate() — Always Error

**File:** `crates/forge-runtime/src/testing/context.rs` (lines 159-188)

Public methods that always return error "requires database connection".

### 13. Store refetch/reset Methods — Exported, No Consumers

**File:** `packages/forge-svelte/stores.ts` (lines 100-184)

`QueryStore.refetch()`, `.reset()`, `SubscriptionStore.refetch()`, `.reset()`
are exported but have zero usage in examples or consumers.

### 14. memory_bytes in SubscriptionCounts — Hardcoded to 0

**File:** `crates/forge-runtime/src/realtime/manager.rs` (line 396)

```rust
memory_bytes: 0, // TODO: calculate if needed
```

### 15. SessionEntry.connected_at — Set, Never Read

**File:** `crates/forge-runtime/src/realtime/message.rs` (lines 104-105)

Set when creating session entry but never read (`last_active` is used instead).
