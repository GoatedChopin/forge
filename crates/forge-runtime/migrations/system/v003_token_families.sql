-- Auth: Add token_family for refresh token chain reuse detection.
-- When a previously-used refresh token is presented, the family ID lets us
-- revoke the entire token chain, protecting against token theft replay attacks.
ALTER TABLE forge_refresh_tokens
    ADD COLUMN token_family UUID NOT NULL DEFAULT gen_random_uuid();

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_family
    ON forge_refresh_tokens (token_family);
