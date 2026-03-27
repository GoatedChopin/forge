-- FORGE Internal Schema v1
-- This migration creates all system tables required by the FORGE runtime.
-- It is applied automatically before any user migrations.

-- Cluster: Node registry (UNLOGGED: transient state rebuilt on startup)
CREATE UNLOGGED TABLE IF NOT EXISTS forge_nodes (
    id UUID PRIMARY KEY,
    hostname VARCHAR(255) NOT NULL,
    ip_address VARCHAR(64) NOT NULL,
    http_port INTEGER NOT NULL,
    grpc_port INTEGER NOT NULL,
    roles TEXT[] NOT NULL DEFAULT '{}',
    worker_capabilities TEXT[] NOT NULL DEFAULT '{}',
    status VARCHAR(32) NOT NULL DEFAULT 'starting',
    version VARCHAR(64),
    current_connections INTEGER NOT NULL DEFAULT 0,
    current_jobs INTEGER NOT NULL DEFAULT 0,
    cpu_usage DOUBLE PRECISION,
    memory_usage DOUBLE PRECISION,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forge_nodes_status_heartbeat
    ON forge_nodes(status, last_heartbeat)
    WHERE status = 'active';

-- Cluster: Leader election (UNLOGGED: transient state rebuilt on startup)
CREATE UNLOGGED TABLE IF NOT EXISTS forge_leaders (
    role VARCHAR(64) PRIMARY KEY,
    node_id UUID NOT NULL,
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lease_until TIMESTAMPTZ NOT NULL
);

-- Jobs: Background job queue
CREATE TABLE IF NOT EXISTS forge_jobs (
    id UUID PRIMARY KEY,
    job_type VARCHAR(255) NOT NULL,
    input JSONB NOT NULL DEFAULT '{}',
    output JSONB,
    job_context JSONB NOT NULL DEFAULT '{}',
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 50,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    last_error TEXT,
    progress_percent INTEGER DEFAULT 0,
    progress_message TEXT,
    worker_capability VARCHAR(255),
    worker_id UUID,
    idempotency_key VARCHAR(255),
    owner_subject TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    failed_at TIMESTAMPTZ,
    cancel_requested_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    cancel_reason TEXT,
    last_heartbeat TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_forge_jobs_status_scheduled
    ON forge_jobs(status, scheduled_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_forge_jobs_idempotency
    ON forge_jobs(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forge_jobs_owner_subject
    ON forge_jobs(owner_subject)
    WHERE owner_subject IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forge_jobs_expires
    ON forge_jobs(expires_at)
    WHERE expires_at IS NOT NULL;

-- Cron: Execution history
CREATE TABLE IF NOT EXISTS forge_cron_runs (
    id UUID PRIMARY KEY,
    cron_name VARCHAR(255) NOT NULL,
    scheduled_time TIMESTAMPTZ NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    node_id UUID,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error TEXT,
    UNIQUE(cron_name, scheduled_time)
);

CREATE INDEX IF NOT EXISTS idx_forge_cron_runs_name_time
    ON forge_cron_runs(cron_name, scheduled_time DESC);

-- Workflows: Definition registry (upserted on startup)
CREATE TABLE IF NOT EXISTS forge_workflow_definitions (
    workflow_name VARCHAR(255) NOT NULL,
    workflow_version VARCHAR(255) NOT NULL,
    workflow_signature VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workflow_name, workflow_version)
);

-- Workflows: Run state
CREATE TABLE IF NOT EXISTS forge_workflow_runs (
    id UUID PRIMARY KEY,
    workflow_name VARCHAR(255) NOT NULL,
    workflow_version VARCHAR(255) NOT NULL,
    workflow_signature VARCHAR(64) NOT NULL,
    owner_subject TEXT,
    input JSONB NOT NULL DEFAULT '{}',
    output JSONB,
    status VARCHAR(32) NOT NULL DEFAULT 'created',
    blocking_reason TEXT,
    resolution_reason TEXT,
    current_step VARCHAR(255),
    step_results JSONB DEFAULT '{}',
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error TEXT,
    trace_id VARCHAR(64),
    -- Durable workflow support
    suspended_at TIMESTAMPTZ,
    wake_at TIMESTAMPTZ,
    waiting_for_event TEXT,
    event_timeout_at TIMESTAMPTZ,
    tenant_id UUID
);

CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_status
    ON forge_workflow_runs(status);

CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_wake
    ON forge_workflow_runs(wake_at)
    WHERE status = 'waiting' AND wake_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_tenant
    ON forge_workflow_runs(tenant_id)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_owner_subject
    ON forge_workflow_runs(owner_subject)
    WHERE owner_subject IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_name_version
    ON forge_workflow_runs(workflow_name, workflow_version)
    WHERE status NOT IN ('completed', 'failed', 'compensated', 'retired_unresumable', 'cancelled_by_operator');

-- Workflows: Event storage for durable workflows
CREATE TABLE IF NOT EXISTS forge_workflow_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_name TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    consumed_at TIMESTAMPTZ,
    consumed_by UUID REFERENCES forge_workflow_runs(id)
);

CREATE INDEX IF NOT EXISTS idx_forge_workflow_events_lookup
    ON forge_workflow_events(event_name, correlation_id)
    WHERE consumed_at IS NULL;

-- Workflows: Step state
CREATE TABLE IF NOT EXISTS forge_workflow_steps (
    id UUID PRIMARY KEY,
    workflow_run_id UUID NOT NULL REFERENCES forge_workflow_runs(id) ON DELETE CASCADE,
    step_name VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    input JSONB,
    result JSONB,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error TEXT,
    UNIQUE(workflow_run_id, step_name)
);

-- Rate Limiting: Token bucket storage (UNLOGGED: transient state rebuilt on startup)
CREATE UNLOGGED TABLE IF NOT EXISTS forge_rate_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bucket_key TEXT NOT NULL,
    tokens DOUBLE PRECISION NOT NULL,
    last_refill TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    max_tokens INTEGER NOT NULL,
    refill_rate DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_forge_rate_limits_bucket
    ON forge_rate_limits(bucket_key);

-- Realtime: Sessions
CREATE TABLE IF NOT EXISTS forge_sessions (
    id UUID PRIMARY KEY,
    node_id UUID NOT NULL,
    user_id VARCHAR(255),
    connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(32) NOT NULL DEFAULT 'connected'
);

CREATE INDEX IF NOT EXISTS idx_forge_sessions_node
    ON forge_sessions(node_id);

-- Realtime: Subscriptions
CREATE TABLE IF NOT EXISTS forge_subscriptions (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES forge_sessions(id) ON DELETE CASCADE,
    query_name VARCHAR(255) NOT NULL,
    query_hash VARCHAR(64) NOT NULL,
    args JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forge_subscriptions_session
    ON forge_subscriptions(session_id);

CREATE INDEX IF NOT EXISTS idx_forge_subscriptions_query_hash
    ON forge_subscriptions(query_hash);

-- Realtime: Change notification function
-- Sends NOTIFY on forge_changes channel when data changes.
-- Format: table:OP:row_id or table:OP:row_id:col1,col2,... (UPDATE only)
CREATE OR REPLACE FUNCTION forge_notify_change() RETURNS TRIGGER AS $$
DECLARE
    row_id TEXT;
    payload TEXT;
    old_json JSONB;
    new_json JSONB;
    changed_cols TEXT[];
BEGIN
    IF TG_OP = 'DELETE' THEN
        row_id := COALESCE(OLD.id::TEXT, '');
    ELSE
        row_id := COALESCE(NEW.id::TEXT, '');
    END IF;

    payload := TG_TABLE_NAME || ':' || TG_OP || ':' || row_id;

    IF TG_OP = 'UPDATE' THEN
        old_json := to_jsonb(OLD);
        new_json := to_jsonb(NEW);
        changed_cols := ARRAY(
            SELECT key FROM jsonb_each(new_json)
            WHERE new_json -> key IS DISTINCT FROM old_json -> key
        );
        IF array_length(changed_cols, 1) > 0 THEN
            payload := payload || ':' || array_to_string(changed_cols, ',');
        END IF;
    END IF;

    PERFORM pg_notify('forge_changes', payload);

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    ELSE
        RETURN NEW;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Helper function to enable reactivity on a table
-- Usage: SELECT forge_enable_reactivity('my_table');
CREATE OR REPLACE FUNCTION forge_enable_reactivity(table_name TEXT) RETURNS VOID AS $$
DECLARE
    trigger_name TEXT;
BEGIN
    trigger_name := 'forge_notify_' || table_name;

    -- Drop existing trigger if any
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', trigger_name, table_name);

    -- Create new trigger
    EXECUTE format('
        CREATE TRIGGER %I
        AFTER INSERT OR UPDATE OR DELETE ON %I
        FOR EACH ROW EXECUTE FUNCTION forge_notify_change()
    ', trigger_name, table_name);
END;
$$ LANGUAGE plpgsql;

-- Helper function to disable reactivity on a table
CREATE OR REPLACE FUNCTION forge_disable_reactivity(table_name TEXT) RETURNS VOID AS $$
DECLARE
    trigger_name TEXT;
BEGIN
    trigger_name := 'forge_notify_' || table_name;
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', trigger_name, table_name);
END;
$$ LANGUAGE plpgsql;

-- GIN indexes for JSONB columns (enables efficient queries on JSON data)

-- Jobs: Enable queries on input/output JSON
CREATE INDEX IF NOT EXISTS idx_forge_jobs_input_gin
    ON forge_jobs USING GIN (input);
CREATE INDEX IF NOT EXISTS idx_forge_jobs_output_gin
    ON forge_jobs USING GIN (output)
    WHERE output IS NOT NULL;

-- Workflows: Enable queries on workflow data
CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_input_gin
    ON forge_workflow_runs USING GIN (input);
CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_output_gin
    ON forge_workflow_runs USING GIN (output)
    WHERE output IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forge_workflow_runs_step_results_gin
    ON forge_workflow_runs USING GIN (step_results);

-- Workflow events: Enable queries on event payload
CREATE INDEX IF NOT EXISTS idx_forge_workflow_events_payload_gin
    ON forge_workflow_events USING GIN (payload)
    WHERE payload IS NOT NULL;

-- Workflow steps: Enable queries on step data
CREATE INDEX IF NOT EXISTS idx_forge_workflow_steps_input_gin
    ON forge_workflow_steps USING GIN (input)
    WHERE input IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_forge_workflow_steps_result_gin
    ON forge_workflow_steps USING GIN (result)
    WHERE result IS NOT NULL;

-- Subscriptions: Enable args matching
CREATE INDEX IF NOT EXISTS idx_forge_subscriptions_args_gin
    ON forge_subscriptions USING GIN (args);

-- Enable reactivity on job/workflow tables for WebSocket subscriptions
SELECT forge_enable_reactivity('forge_jobs');
SELECT forge_enable_reactivity('forge_workflow_runs');
SELECT forge_enable_reactivity('forge_workflow_steps');

-- Daemons: Long-running singleton tasks
CREATE TABLE IF NOT EXISTS forge_daemons (
    name VARCHAR(255) PRIMARY KEY,
    node_id UUID,
    instance_id UUID NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'stopped',
    restarts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    started_at TIMESTAMPTZ,
    last_heartbeat TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_forge_daemons_status
    ON forge_daemons(status);

CREATE INDEX IF NOT EXISTS idx_forge_daemons_node
    ON forge_daemons(node_id)
    WHERE node_id IS NOT NULL;

-- Webhooks: Idempotency tracking for webhook events
CREATE TABLE IF NOT EXISTS forge_webhook_events (
    webhook_name VARCHAR(255) NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (webhook_name, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_forge_webhook_events_expires
    ON forge_webhook_events(expires_at);

CREATE INDEX IF NOT EXISTS idx_forge_webhook_events_webhook
    ON forge_webhook_events(webhook_name);

-- Workflow event-driven wakeup via NOTIFY.
-- When a workflow event is inserted, notify the scheduler immediately
-- instead of waiting for the next poll cycle.
CREATE OR REPLACE FUNCTION forge_workflow_event_notify() RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('forge_workflow_wakeup', NEW.correlation_id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER forge_workflow_event_notify_trigger
    AFTER INSERT ON forge_workflow_events
    FOR EACH ROW EXECUTE FUNCTION forge_workflow_event_notify();

-- Periodic cleanup function for expired webhook idempotency records
-- This can be called from a cron job: SELECT forge_cleanup_webhook_events();
CREATE OR REPLACE FUNCTION forge_cleanup_webhook_events() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM forge_webhook_events WHERE expires_at < NOW();
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Periodic cleanup function for expired job records
-- Deletes completed/cancelled/failed jobs past their TTL
-- This can be called from a cron job: SELECT forge_cleanup_expired_jobs();
CREATE OR REPLACE FUNCTION forge_cleanup_expired_jobs() RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM forge_jobs
    WHERE expires_at IS NOT NULL
      AND expires_at < NOW()
      AND status IN ('completed', 'cancelled', 'failed', 'dead_letter');
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Auth: Refresh token storage for built-in token rotation
CREATE TABLE IF NOT EXISTS forge_refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    client_id   TEXT,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_forge_refresh_tokens_user_id
    ON forge_refresh_tokens (user_id);

CREATE INDEX IF NOT EXISTS idx_forge_refresh_tokens_expires_at
    ON forge_refresh_tokens (expires_at);

-- Periodically purge expired tokens to prevent table bloat.
-- Runs every hour, deleting tokens that expired more than 24 hours ago
-- (keeps recently-expired tokens for audit/error-reporting purposes).
CREATE OR REPLACE FUNCTION forge_purge_expired_refresh_tokens()
RETURNS void LANGUAGE sql AS $$
    DELETE FROM forge_refresh_tokens
    WHERE expires_at < now() - interval '24 hours';
$$;

-- OAuth: Dynamic client registrations (MCP clients self-register via RFC 7591)
CREATE TABLE IF NOT EXISTS forge_oauth_clients (
    client_id                  TEXT PRIMARY KEY DEFAULT gen_random_uuid()::TEXT,
    client_name                TEXT,
    redirect_uris              TEXT[] NOT NULL DEFAULT '{}',
    token_endpoint_auth_method TEXT NOT NULL DEFAULT 'none',
    created_at                 TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- OAuth: Authorization codes (short-lived, PKCE-bound)
CREATE TABLE IF NOT EXISTS forge_oauth_codes (
    code                   TEXT PRIMARY KEY,
    client_id              TEXT NOT NULL REFERENCES forge_oauth_clients(client_id) ON DELETE CASCADE,
    user_id                UUID NOT NULL,
    redirect_uri           TEXT NOT NULL,
    code_challenge         TEXT NOT NULL,
    code_challenge_method  TEXT NOT NULL DEFAULT 'S256',
    scopes                 TEXT[] NOT NULL DEFAULT '{}',
    expires_at             TIMESTAMPTZ NOT NULL,
    used_at                TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_forge_oauth_codes_expires
    ON forge_oauth_codes(expires_at);

-- Purge expired authorization codes (called by cron or manually)
CREATE OR REPLACE FUNCTION forge_purge_expired_oauth_codes()
RETURNS void LANGUAGE sql AS $$
    DELETE FROM forge_oauth_codes
    WHERE expires_at < now() - interval '1 hour';
$$;

-- Cluster-aware cache invalidation tracking.
-- Used by the Reactor to propagate invalidation events across nodes
-- when a write occurs on one node and subscriptions exist on another.
CREATE TABLE IF NOT EXISTS forge_invalidations (
    id              BIGSERIAL PRIMARY KEY,
    table_name      TEXT NOT NULL,
    row_id          TEXT,
    operation       TEXT NOT NULL,          -- INSERT, UPDATE, DELETE
    changed_columns TEXT[],
    node_id         UUID,                   -- originating node
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for efficient polling by other nodes
CREATE INDEX IF NOT EXISTS idx_forge_invalidations_created
    ON forge_invalidations (created_at);

-- Auto-purge old invalidation records (keep only last hour)
CREATE OR REPLACE FUNCTION forge_purge_expired_invalidations()
RETURNS void LANGUAGE sql AS $$
    DELETE FROM forge_invalidations
    WHERE created_at < now() - interval '1 hour';
$$;
