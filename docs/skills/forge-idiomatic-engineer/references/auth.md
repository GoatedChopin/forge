# Auth Reference

End-to-end auth setup for Forge apps.

## Config

```toml
# HS256 (self-issued JWT)
[auth]
jwt_algorithm = "HS256"
jwt_secret = "${JWT_SECRET}"
token_expiry = "15m"

# RS256 (external provider like Auth0, Clerk, Firebase)
[auth]
jwt_algorithm = "RS256"
jwks_url = "https://provider.com/.well-known/jwks.json"
jwt_issuer = "https://provider.com/"
jwt_audience = "my-app"
jwks_cache_ttl_secs = 3600
```

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
#[forge::model]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthInput { pub email: String, pub password: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse { pub token: String, pub user: User }
```

### Handlers

```rust
#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let hash = bcrypt::hash(&input.password, bcrypt::DEFAULT_COST)
        .map_err(|e| ForgeError::Internal(e.to_string()))?;

    let user: User = ctx.db().fetch_one(
        sqlx::query_as("INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING *")
            .bind(&input.email).bind(&hash)
    ).await.map_err(|e| {
        if e.to_string().contains("unique constraint") {
            ForgeError::Validation("Email already registered".into())
        } else { e.into() }
    })?;

    let claims = Claims::builder()
        .subject(user.id)
        .role("user")
        .build();
    let token = ctx.issue_token(&claims)?;

    Ok(AuthResponse { token, user })
}

#[forge::mutation(public)]
pub async fn login(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let user: User = ctx.db().fetch_optional(
        sqlx::query_as("SELECT * FROM users WHERE email = $1").bind(&input.email)
    ).await?
    .ok_or_else(|| ForgeError::Unauthorized("Invalid credentials".into()))?;

    if !bcrypt::verify(&input.password, &user.password_hash)
        .unwrap_or(false) {
        return Err(ForgeError::Unauthorized("Invalid credentials".into()));
    }

    let claims = Claims::builder()
        .subject(user.id)
        .role("user")
        .build();
    let token = ctx.issue_token(&claims)?;

    Ok(AuthResponse { token, user })
}
```

### Registration

```rust
Forge::builder()
    .register_mutation::<functions::RegisterMutation>()
    .register_mutation::<functions::LoginMutation>()
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

See `frontend/svelte.md` for auth store details. Key points:
- `auth.setAuth(token, user)` after login (persists to localStorage, reconnects SSE)
- `auth.clearAuth()` on logout
- `getToken()` passed to `ForgeProvider` for automatic header injection

## Frontend Auth (Dioxus)

Pass `get_token` callback to `ForgeProvider`. Store token in a signal or persistent storage. On auth change, the client reconnects automatically.

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
