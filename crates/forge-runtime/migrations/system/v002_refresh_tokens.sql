CREATE TABLE IF NOT EXISTS forge_refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_forge_refresh_tokens_user_id
    ON forge_refresh_tokens (user_id);

CREATE INDEX IF NOT EXISTS idx_forge_refresh_tokens_expires_at
    ON forge_refresh_tokens (expires_at);
