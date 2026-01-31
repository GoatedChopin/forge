-- FORGE Internal Schema v2
-- This migration adds tables for daemons and webhooks.

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
    idempotency_key VARCHAR(255) PRIMARY KEY,
    webhook_name VARCHAR(255) NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_forge_webhook_events_expires
    ON forge_webhook_events(expires_at);

CREATE INDEX IF NOT EXISTS idx_forge_webhook_events_webhook
    ON forge_webhook_events(webhook_name);

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
