-- Workflows: Persist compensation metadata so it survives process restarts.
-- Stores step names that registered compensation handlers and their
-- completion order, allowing the executor to reconstruct the reverse-order
-- compensation sequence after resume without in-memory state.
ALTER TABLE forge_workflow_runs
    ADD COLUMN compensation_state JSONB;
