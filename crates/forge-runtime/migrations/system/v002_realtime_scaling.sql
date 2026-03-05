-- FORGE Internal Schema v2: Realtime Scaling
-- Column-aware NOTIFY, UNLOGGED ephemeral tables, workflow event wakeup.

-- 1. Enhanced change notification with changed columns on UPDATE.
--    Compares OLD and NEW as JSONB to detect which columns changed.
--    Format: table:OP:row_id or table:OP:row_id:col1,col2,...
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

-- 2. UNLOGGED tables for ephemeral coordination state.
--    These skip WAL writes. Data is truncated on crash recovery, which is
--    acceptable because all three tables contain transient state that nodes
--    rebuild on startup.
ALTER TABLE forge_rate_limits SET UNLOGGED;
ALTER TABLE forge_nodes SET UNLOGGED;
ALTER TABLE forge_leaders SET UNLOGGED;

-- 3. Workflow event-driven wakeup via NOTIFY.
--    When a workflow event is inserted, notify the scheduler immediately
--    instead of waiting for the next poll cycle.
CREATE OR REPLACE FUNCTION forge_workflow_event_notify() RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('forge_workflow_wakeup', NEW.correlation_id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS forge_workflow_event_notify_trigger ON forge_workflow_events;
CREATE TRIGGER forge_workflow_event_notify_trigger
    AFTER INSERT ON forge_workflow_events
    FOR EACH ROW EXECUTE FUNCTION forge_workflow_event_notify();

-- 4. Index to speed up heartbeat dead-node detection queries.
CREATE INDEX IF NOT EXISTS idx_forge_nodes_status_heartbeat
    ON forge_nodes(status, last_heartbeat)
    WHERE status = 'active';
