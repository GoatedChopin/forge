-- Add owner_subject to cron runs for per-tenant audit trails.
-- Mirrors forge_jobs.owner_subject. NULL for system-scheduled runs.
ALTER TABLE forge_cron_runs
    ADD COLUMN IF NOT EXISTS owner_subject TEXT;
