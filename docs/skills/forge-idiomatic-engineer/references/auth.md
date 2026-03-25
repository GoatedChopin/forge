# Auth Reference

End-to-end auth setup for Forge apps.

## Config

```toml
# HS256 (self-issued JWT) with built-in refresh tokens
[auth]
jwt_algorithm = "HS256"
jwt_secret = "${JWT_SECRET}"
access_token_ttl = "1h"    # default "1h", accepts "15m", "2h", etc.
refresh_token_ttl = "30d"  # default "30d", accepts "7d", "90d", etc.

# RS256 (external provider like Auth0, Clerk, Firebase)
[auth]
jwt_algorithm = "RS256"
jwks_url = "https://provider.com/.well-known/jwks.json"
jwt_issuer = "https://provider.com/"
jwt_audience = "my-app"
jwks_cache_ttl_secs = 3600
```

`access_token_ttl` and `refresh_token_ttl` control the lifetime of tokens issued by `ctx.issue_token_pair()`. The deprecated `token_expiry` is used as a fallback for `access_token_ttl` if not set.

Supported algorithms: HS256, HS384, HS512 (need `jwt_secret`), RS256, RS384, RS512 (need `jwks_url`). Validation: 60s clock skew leeway, requires `exp` and `sub` claims.

## Self-Issued JWT (HS256)

### Migration

```sql
-- @up
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Schema

```rust
// Internal: used for DB queries. NOT in handler return types.
// #[serde(skip)] does NOT hide fields from forge generate.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Public: used in API responses. This is what forge generate sees.
// Omit sensitive fields entirely instead of using serde skip.
#[forge::model]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<User> for PublicUser {
    fn from(u: User) -> Self {
        Self { id: u.id, email: u.email, display_name: u.display_name, created_at: u.created_at }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthInput { pub email: String, pub password: String }
```

### Handlers (with built-in refresh tokens)

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: PublicUser,  // Never expose User with password_hash
}

#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let hash = bcrypt::hash(&input.password, bcrypt::DEFAULT_COST)
        .map_err(|e| ForgeError::Internal(e.to_string()))?;

    let user: User = sqlx::query_as!(User, "INSERT INTO users ...")
        .fetch_one(ctx.pool()).await?;

    // Framework handles token generation, hashing, storage, and expiry.
    // TTLs come from [auth] in forge.toml (access_token_ttl, refresh_token_ttl).
    let pair = ctx.issue_token_pair(user.id, &["user"]).await?;
    Ok(AuthResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        user: user.into(),  // Convert User → PublicUser
    })
}

#[forge::mutation(public)]
pub async fn login(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let user: User = /* verify credentials */;
    let pair = ctx.issue_token_pair(user.id, &["user"]).await?;
    Ok(AuthResponse { access_token: pair.access_token, refresh_token: pair.refresh_token, user: user.into() })
}

// Refresh: framework rotates the token atomically (delete old, insert new)
#[forge::mutation(public)]
pub async fn refresh(ctx: &MutationContext, input: RefreshInput) -> Result<RefreshResponse> {
    let pair = ctx.rotate_refresh_token(&input.refresh_token).await?;
    Ok(RefreshResponse { access_token: pair.access_token, refresh_token: pair.refresh_token })
}

// Logout: framework revokes the refresh token
#[forge::mutation]
pub async fn logout(ctx: &MutationContext, input: LogoutInput) -> Result<()> {
    ctx.revoke_refresh_token(&input.refresh_token).await
}
```

### Registration

```rust
// auto_register() discovers all #[forge::query], #[forge::mutation], etc. automatically
Forge::builder()
    .config(config)
    .auto_register()
    .build()?
    .run()
    .await
```

## Claims Builder

```rust
use forge::forge_core::Claims;

let claims = Claims::builder()
    .user_id(uuid)                          // sets sub as UUID string
    .subject("custom-sub")                  // sets sub as arbitrary string
    .role("admin")                          // add single role
    .roles(vec!["admin", "editor"])         // set multiple roles
    .claim("org_id", json!("org-123"))      // custom claim
    .tenant_id(tenant_uuid)                 // sets tenant_id claim
    .duration_secs(3600)                    // token expiry in seconds
    .build()?;                              // Result<Claims, String>
```

Forge uses the same secret from `[auth].jwt_secret` for both validation and issuance. Token issuance via `ctx.issue_token()` is only available in HMAC mode (HS256/384/512). For RS256/JWKS (external providers), token issuance is not available since Forge only holds public keys.

## Identity Scope Enforcement

Authenticated handlers with input args containing scope keys are validated at runtime. The router's `check_identity_args()` checks that the scope value matches the authenticated principal. Admins bypass all scope checks.

Recognized identity keys: `user_id`, `userId`, `owner_id`, `ownerId`, `owner_subject`, `ownerSubject`, `subject`, `sub`, `principal_id`, `principalId`.
Recognized tenant keys: `tenant_id`, `tenantId`.

Functions with no input args are exempt from scope enforcement. They still require auth and can access the user via `ctx.require_user_id()`.

Generated TypeScript bindings may omit the scope field from client-facing call signatures because the Forge client injects it automatically. Treat the generated frontend API as authoritative for the browser call shape.

Do not confuse scope enforcement with trusting frontend data. The backend must still derive the acting user from auth context and validate any other payload fields.

Anti-pattern:
```rust
// WRONG: redundant, the router already enforces this
let uid = ctx.require_user_id()?;
if input.user_id != uid { return Err(ForgeError::Forbidden(...)); }
```

## AuthContext Methods

```rust
ctx.auth.is_authenticated() -> bool
ctx.auth.user_id() -> Option<Uuid>
ctx.auth.require_user_id() -> Result<Uuid>
ctx.auth.subject() -> Option<&str>           // raw sub claim, any format
ctx.auth.require_subject() -> Result<&str>
ctx.auth.has_role("admin") -> bool
ctx.auth.require_role("admin") -> Result<()> // returns Forbidden
ctx.auth.claim("org_id") -> Option<&Value>
ctx.auth.claims() -> &HashMap<String, Value>
ctx.auth.roles() -> &[String]
ctx.auth.principal_id() -> Option<String>    // prefers sub, falls back to UUID
ctx.auth.is_admin() -> bool                  // has "admin" role
```

## Frontend Auth (SvelteKit)

The generated `auth.svelte.ts` store provides built-in refresh token support:

```typescript
import { auth, getToken } from "./forge/auth.svelte";

// After login/register (stores tokens + user to localStorage, reconnects SSE):
auth.setAuth(response.access_token, response.refresh_token, response.user);

// Update just tokens after refresh (preserves user):
auth.updateTokens(response.access_token, response.refresh_token);

// Start periodic refresh (call once in root layout):
auth.startRefreshLoop("http://localhost:9081", 40 * 60 * 1000);

// Logout (clears localStorage, stops refresh loop, reconnects SSE):
auth.clearAuth();

// Read state:
auth.isAuthenticated  // boolean
auth.token            // string | null
auth.user             // User | null

// Pass to ForgeProvider:
// <ForgeProvider url="..." getToken={getToken}>
```

The store handles localStorage persistence, periodic token refresh, and 401 recovery via `auth.handleAuthError()`. SSE reconnects automatically on `setAuth` and `clearAuth`.

## Frontend Auth (Dioxus)

Use `ForgeAuthProvider` instead of `ForgeProvider` for built-in token + viewer storage, refresh loops, and 401 recovery:

```rust
rsx! {
    ForgeAuthProvider {
        url: "http://localhost:9081",
        app_name: "my-app",
        refresh_interval_secs: 2400,   // default 40min, ~2/3 of access_token_ttl
        Router::<Route> {}
    }
}
```

Access auth state and viewer in components:
```rust
let mut auth = use_forge_auth();

// After login/register (stores tokens + viewer, persists across sessions):
auth.login_with_viewer(response.access_token, response.refresh_token, &response.viewer);

// Read the viewer anywhere (typed to your app's user struct):
let viewer: Option<Viewer> = use_viewer::<Viewer>();

// Update just the viewer (e.g. after profile edit):
auth.update_viewer(&updated_viewer);

// Logout (clears tokens + viewer):
auth.logout();

// Route guard (redirects unauthenticated users):
if !use_require_auth("/login") { return rsx! {} }

// Keyed remount for SSE reconnection on auth change:
let auth_key = use_auth_key();
rsx! { main { key: "{auth_key}", Router::<Route> {} } }
```

`ForgeAuthProvider` automatically wires the token provider, handles 401 errors with refresh, runs periodic refresh, and persists the viewer alongside tokens. See `references/frontend/dioxus.md` for the full auth pattern.

## Refresh Tokens (Built-in)

Forge provides built-in refresh token management via `forge_refresh_tokens` system table. No app-level migration needed.

### MutationContext Methods

```rust
// Issue access + refresh token pair (TTLs from forge.toml)
let pair = ctx.issue_token_pair(user_id, &["user"]).await?;
// pair.access_token, pair.refresh_token

// Rotate: delete old token, issue new pair atomically
let pair = ctx.rotate_refresh_token(&old_refresh_token).await?;

// Revoke a specific token (logout)
ctx.revoke_refresh_token(&refresh_token).await?;

// Revoke all tokens for a user (password change, account deletion)
ctx.revoke_all_refresh_tokens(user_id).await?;
```

### How it works

- Refresh tokens are random opaque strings, SHA-256 hashed before storage
- Single-use rotation: each refresh consumes the old token and issues a new one
- Expired tokens are rejected; a cleanup function `forge_cleanup_refresh_tokens()` is available for cron

### Claims Builder (low-level)

For manual token issuance without refresh tokens:

```rust
let claims = Claims::builder()
    .subject(user_id)
    .role("user")
    .duration_secs(900)
    .build()
    .map_err(ForgeError::Internal)?;
let token = ctx.issue_token(&claims)?;
```

`issue_token()` and `issue_token_pair()` are only available on `MutationContext` and only with HMAC algorithms (HS256/384/512).

## External Provider (JWKS)

No backend auth handlers needed. Configure `jwks_url` + `jwt_issuer` + `jwt_audience` in `forge.toml`. Forge validates tokens from the provider directly. Frontend gets tokens from the provider SDK and passes via `Authorization: Bearer <token>`.

### Common Provider JWKS URLs

| Provider | JWKS URL |
|---|---|
| Firebase | `https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com` |
| Auth0 | `https://YOUR_DOMAIN.auth0.com/.well-known/jwks.json` |
| Clerk | `https://YOUR_DOMAIN.clerk.accounts.dev/.well-known/jwks.json` |
| Supabase | `https://YOUR_PROJECT.supabase.co/auth/v1/jwks` |

When `jwt_issuer` is set, tokens with a different `iss` claim are rejected. When `jwt_audience` is set, tokens with a different `aud` claim are rejected. Both optional but recommended to prevent token confusion across services.

## OAuth 2.1 for MCP

Enable OAuth so MCP clients (Claude Code, etc.) can auto-authenticate:

```toml
[auth]
jwt_secret = "${JWT_SECRET}"

[mcp]
enabled = true
oauth = true
```

Forge acts as an OAuth 2.1 Authorization Server with PKCE. Works with both HMAC and JWKS modes. The authorize page detects existing sessions from localStorage (same origin).

Endpoints created: `/.well-known/oauth-authorization-server`, `/_api/oauth/authorize`, `/_api/oauth/token`, `/_api/oauth/register`.

OAuth-issued tokens include `aud: "forge:mcp"` to scope access to MCP endpoints. Refresh tokens are bound to the OAuth client that created them.

HMAC mode: built-in login form queries `users` table (email + password_hash). JWKS mode: user must be logged into the app first (token read from localStorage).

## Testing Auth

```rust
// UUID auth
let ctx = TestQueryContext::builder()
    .as_user(Uuid::new_v4())
    .with_role("admin")
    .with_claim("org_id", json!("org-123"))
    .build();

// Non-UUID auth (Firebase, Clerk)
let ctx = TestMutationContext::builder()
    .as_subject("firebase-uid-abc123")
    .with_role("user")
    .build();

// Unauthenticated
let ctx = TestQueryContext::minimal();
assert!(!ctx.auth.is_authenticated());
```
