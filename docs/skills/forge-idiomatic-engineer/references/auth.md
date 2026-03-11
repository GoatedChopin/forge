# Auth Scaffolding

Complete end-to-end auth setup for Forge apps with HS256 self-issued JWT. This is the single source of truth for auth configuration, backend implementation, and frontend wiring.

Frontend examples below are SvelteKit-first. For Dioxus repos, keep the same backend/auth rules and adapt the client wiring to the Dioxus runtime patterns in `references/dioxus.md`.

## forge.toml

```toml
[auth]
jwt_algorithm = "HS256"
jwt_secret = "${JWT_SECRET}"
```

## .env

```
JWT_SECRET=your-dev-secret-change-in-production
```

## Migration

```sql
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(100) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## Cargo.toml dependency

```toml
[dependencies]
bcrypt = "0.17"
```

## Schema (src/schema/user.rs)

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthInput {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
}
```

Re-export from `src/schema/mod.rs`:

```rust
pub mod user;
pub use user::*;
```

## Functions (src/functions/auth.rs)

```rust
use forge::forge_core::Claims;
use forge::prelude::*;

use crate::schema::{AuthInput, AuthResponse, User};

#[forge::mutation(public)]
pub async fn register(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let username = input.username.trim().to_string();
    if username.is_empty() || username.len() > 100 {
        return Err(ForgeError::Validation("Username must be 1-100 characters".into()));
    }
    if input.password.len() < 8 {
        return Err(ForgeError::Validation("Password must be at least 8 characters".into()));
    }

    let hash = bcrypt::hash(&input.password, 12)
        .map_err(|e| ForgeError::Internal(format!("hash error: {e}")))?;

    let user: User = ctx
        .db()
        .fetch_one(
            sqlx::query_as::<_, User>(
                "INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING *",
            )
            .bind(&username)
            .bind(&hash),
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                ForgeError::Validation("Username already taken".into())
            } else {
                ForgeError::Internal(format!("database error: {e}"))
            }
        })?;

    let claims = Claims::builder()
        .user_id(user.id)
        .duration_secs(7 * 24 * 3600)
        .build()
        .map_err(ForgeError::Internal)?;

    Ok(AuthResponse {
        token: ctx.issue_token(&claims)?,
        user_id: user.id,
        username: user.username,
    })
}

#[forge::mutation(public)]
pub async fn login(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let username = input.username.trim().to_string();

    let user: User = ctx
        .db()
        .fetch_optional(
            sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
                .bind(&username),
        )
        .await
        .map_err(|e| ForgeError::Internal(format!("database error: {e}")))?
        .ok_or_else(|| ForgeError::Unauthorized("Invalid username or password".into()))?;

    let valid = bcrypt::verify(&input.password, &user.password_hash)
        .map_err(|e| ForgeError::Internal(format!("verify error: {e}")))?;

    if !valid {
        return Err(ForgeError::Unauthorized("Invalid username or password".into()));
    }

    let claims = Claims::builder()
        .user_id(user.id)
        .duration_secs(7 * 24 * 3600)
        .build()
        .map_err(ForgeError::Internal)?;

    Ok(AuthResponse {
        token: ctx.issue_token(&claims)?,
        user_id: user.id,
        username: user.username,
    })
}
```

Re-export from `src/functions/mod.rs`:

```rust
pub mod auth;
pub use auth::*;
```

## Registration in main.rs

```rust
builder
    .register_mutation::<functions::RegisterMutation>()
    .register_mutation::<functions::LoginMutation>();
```

## Token Issuance

When HMAC auth is configured (`jwt_algorithm = "HS256"`), mutations can issue tokens via `ctx.issue_token()`. Use `Claims::builder()`:

```rust
use forge::forge_core::Claims;

let claims = Claims::builder()
    .user_id(uuid)              // sets sub claim as UUID
    .subject("custom-sub")     // sets sub as arbitrary string
    .role("admin")             // add single role
    .roles(vec!["admin", "editor"]) // set multiple roles
    .claim("org_id", json!("org-123")) // custom claim
    .tenant_id(tenant_uuid)   // sets tenant_id claim
    .duration_secs(3600)       // token expiry
    .build()?;
```

No extra dependencies needed. Forge uses the same secret from `[auth].jwt_secret` for both validation and issuance. For RSA/JWKS auth (external providers), token issuance is not available since Forge doesn't hold the private key.

## External Provider Auth (RS256/JWKS)

For apps that authenticate through Firebase, Auth0, Clerk, Supabase, or other external identity providers, use RS256 with a JWKS endpoint instead of HS256.

### forge.toml

```toml
[auth]
jwt_algorithm = "RS256"
jwks_url = "https://your-provider/.well-known/jwks.json"
jwt_issuer = "https://your-provider"     # validates iss claim
jwt_audience = "your-app-id"             # validates aud claim
```

Forge fetches and caches the provider's public keys from the JWKS URL automatically (`jwks_cache_ttl_secs` defaults to 3600).

### Provider JWKS URLs

| Provider | JWKS URL |
|----------|----------|
| Firebase | `https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com` |
| Auth0 | `https://YOUR_DOMAIN.auth0.com/.well-known/jwks.json` |
| Clerk | `https://YOUR_DOMAIN.clerk.accounts.dev/.well-known/jwks.json` |
| Supabase | `https://YOUR_PROJECT.supabase.co/auth/v1/jwks` |

### Firebase example

```toml
[auth]
jwt_algorithm = "RS256"
jwks_url = "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com"
jwt_issuer = "https://securetoken.google.com/YOUR_PROJECT_ID"
jwt_audience = "YOUR_PROJECT_ID"
```

When `jwt_issuer` is set, tokens with a different `iss` claim are rejected. When `jwt_audience` is set, tokens with a different `aud` claim are rejected. Both are optional but recommended for external providers to prevent token confusion across services.

Token issuance via `ctx.issue_token()` is not available in RS256 mode since Forge only holds public keys, not the provider's private signing key.

## Identity Scope Enforcement

Authenticated (non-public) functions with an input parameter must include at least one identity or tenant scope argument. Without it, Forge returns a runtime error: `"Function '...' must include identity or tenant scope arguments"`.

Functions with no input args are exempt. These still require authentication and can access the verified user via `ctx.require_user_id()`.

Recognized identity keys: `user_id`, `userId`, `owner_id`, `ownerId`, `owner_subject`, `ownerSubject`, `subject`, `sub`, `principal_id`, `principalId`.
Recognized tenant keys: `tenant_id`, `tenantId`.

Forge validates that the provided value matches the authenticated JWT subject at the router level. Do not add manual identity comparison checks in handlers.

If a handler has any other business input, keep the scope field in the Rust input type. The no-input exemption only applies when there is no input parameter at all.

Generated TypeScript bindings may omit the scope field from client-facing call signatures because the Forge client can inject it automatically. Treat the generated frontend API as authoritative for the browser call shape.

Do not confuse scope enforcement with trusting frontend data. The backend must still derive the acting user from `ctx.require_user_id()?` and validate any other payload fields before using them.

**Anti-pattern:**
```rust
// WRONG: redundant check, the router already enforces this
let uid = ctx.require_user_id()?;
if input.user_id != uid {
    return Err(ForgeError::Forbidden("Access denied".into()));
}
```

## Frontend Auth Store (src/lib/auth.svelte.ts)

```typescript
import { getForgeClient } from "@forge-rs/svelte"
import { getContext, setContext } from "svelte"

const AUTH_KEY = Symbol("auth")

export class AuthStore {
  token = $state<string | null>(null)
  userId = $state<string | null>(null)
  username = $state<string | null>(null)

  isAuthenticated = $derived(this.token !== null)

  getToken(): string | null {
    return this.token
  }

  setAuth(data: { token: string; user_id: string; username: string }) {
    this.token = data.token
    this.userId = data.user_id
    this.username = data.username

    if (typeof localStorage === "undefined") return
    localStorage.setItem("auth_token", data.token)
    localStorage.setItem("auth_user_id", data.user_id)
    localStorage.setItem("auth_username", data.username)

    getForgeClient()?.reconnect()
  }

  logout() {
    this.token = null
    this.userId = null
    this.username = null

    if (typeof localStorage === "undefined") return
    localStorage.removeItem("auth_token")
    localStorage.removeItem("auth_user_id")
    localStorage.removeItem("auth_username")

    getForgeClient()?.reconnect()
  }

  hydrate() {
    if (typeof localStorage === "undefined") return
    this.token = localStorage.getItem("auth_token")
    this.userId = localStorage.getItem("auth_user_id")
    this.username = localStorage.getItem("auth_username")
  }
}

export function createAuthStore(): AuthStore {
  const store = new AuthStore()
  setContext(AUTH_KEY, store)
  return store
}

export function getAuthStore(): AuthStore {
  return getContext<AuthStore>(AUTH_KEY)
}
```

## Frontend Layout Wiring

If the auth store touches `localStorage`, disable SSR for the route tree:

```typescript
// +layout.ts
export const ssr = false
```

```svelte
<!-- +layout.svelte -->
<script lang="ts">
  import { resolve } from "$app/paths"
  import { ForgeProvider } from "$lib/forge"
  import { PUBLIC_API_URL } from "$env/static/public"
  import { createAuthStore } from "$lib/auth.svelte"

  let { children } = $props()

  const auth = createAuthStore()
  auth.hydrate()
</script>

<ForgeProvider url={PUBLIC_API_URL} getToken={() => auth.getToken()}>
  <nav>
    <a href={resolve("/")}>Home</a>
    {#if auth.isAuthenticated}
      <button type="button" onclick={() => auth.logout()}>Log out</button>
    {:else}
      <a href={resolve("/login")}>Log in</a>
      <a href={resolve("/register")}>Register</a>
    {/if}
  </nav>

  {@render children()}
</ForgeProvider>
```

## Login Page (+page.svelte)

```svelte
<script lang="ts">
  import { goto } from "$app/navigation"
  import { resolve } from "$app/paths"
  import { getAuthStore } from "$lib/auth.svelte"
  import { ForgeClientError, login } from "$lib/forge"

  const auth = getAuthStore()

  let username = $state("")
  let password = $state("")
  let error = $state("")
  let loading = $state(false)

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault()
    error = ""
    loading = true

    try {
      const result = await login({ username, password })
      auth.setAuth(result)
      goto(resolve("/"))
    } catch (err) {
      if (err instanceof ForgeClientError) {
        error = err.message
      } else {
        error = "Something went wrong"
      }
    } finally {
      loading = false
    }
  }
</script>

<main>
  <h1>Log in</h1>

  {#if error}
    <p role="alert">{error}</p>
  {/if}

  <form onsubmit={handleSubmit}>
    <label>
      Username
      <input type="text" bind:value={username} required autocomplete="username" />
    </label>

    <label>
      Password
      <input type="password" bind:value={password} required autocomplete="current-password" />
    </label>

    <button type="submit" disabled={loading}>
      {loading ? "Logging in..." : "Log in"}
    </button>
  </form>

  <p>Don't have an account? <a href={resolve("/register")}>Register</a></p>
</main>
```

## Register Page (+page.svelte)

```svelte
<script lang="ts">
  import { goto } from "$app/navigation"
  import { resolve } from "$app/paths"
  import { getAuthStore } from "$lib/auth.svelte"
  import { ForgeClientError, register } from "$lib/forge"

  const auth = getAuthStore()

  let username = $state("")
  let password = $state("")
  let error = $state("")
  let loading = $state(false)

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault()
    error = ""
    loading = true

    try {
      const result = await register({ username, password })
      auth.setAuth(result)
      goto(resolve("/"))
    } catch (err) {
      if (err instanceof ForgeClientError) {
        error = err.message
      } else {
        error = "Something went wrong"
      }
    } finally {
      loading = false
    }
  }
</script>

<main>
  <h1>Register</h1>

  {#if error}
    <p role="alert">{error}</p>
  {/if}

  <form onsubmit={handleSubmit}>
    <label>
      Username
      <input type="text" bind:value={username} required autocomplete="username" />
    </label>

    <label>
      Password (minimum 8 characters)
      <input type="password" bind:value={password} required minlength="8" autocomplete="new-password" />
    </label>

    <button type="submit" disabled={loading}>
      {loading ? "Creating account..." : "Register"}
    </button>
  </form>

  <p>Already have an account? <a href={resolve("/login")}>Log in</a></p>
</main>
```

## Redirect Pattern (authenticated pages)

```svelte
<script lang="ts">
  import { goto } from "$app/navigation"
  import { resolve } from "$app/paths"
  import { getAuthStore } from "$lib/auth.svelte"

  const auth = getAuthStore()

  $effect(() => {
    if (!auth.isAuthenticated) {
      goto(resolve("/login"))
    }
  })
</script>
```

## curl Verification

After the backend is running, verify the full flow:

```bash
# Register
curl -s -X POST http://localhost:8080/_api/rpc/register \
  -H "Content-Type: application/json" \
  -d '{"args": {"username": "'"$(whoami)"'", "password": "password123"}}'

# Login
curl -s -X POST http://localhost:8080/_api/rpc/login \
  -H "Content-Type: application/json" \
  -d '{"args": {"username": "'"$(whoami)"'", "password": "password123"}}'
```

Both should return `{ "token": "...", "user_id": "...", "username": "..." }`. Use the token in subsequent requests as `Authorization: Bearer <token>`.
