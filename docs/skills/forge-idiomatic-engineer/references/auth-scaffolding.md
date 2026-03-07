# Auth Scaffolding Reference

Complete end-to-end auth setup for Forge apps with HS256 self-issued JWT. This is the single source of truth for auth configuration, backend implementation, and frontend wiring.

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

Remember to re-export from `src/schema/mod.rs`:

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
        .map_err(|e| ForgeError::Internal(e))?;

    Ok(AuthResponse {
        token: ctx.issue_token(&claims)?,
        user_id: user.id,
        username: user.username,
    })
}

#[forge::mutation(public)]
pub async fn login(ctx: &MutationContext, input: AuthInput) -> Result<AuthResponse> {
    let username = input.username.trim().to_string();

    let user: User = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE username = $1",
    )
    .bind(&username)
    .fetch_optional(ctx.db_pool())
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
        .map_err(|e| ForgeError::Internal(e))?;

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

## Frontend Auth Store (src/lib/auth.svelte.ts)

```typescript
import { getContext, setContext } from "svelte"

const AUTH_KEY = Symbol("auth")

export class AuthStore {
  token = $state<string | null>(null)
  userId = $state<string | null>(null)
  username = $state<string | null>(null)

  isAuthenticated = $derived(this.token !== null)

  // called by ForgeProvider to get current token
  getToken(): string | null {
    return this.token
  }

  setAuth(data: { token: string; user_id: string; username: string }) {
    this.token = data.token
    this.userId = data.user_id
    this.username = data.username

    localStorage.setItem("auth_token", data.token)
    localStorage.setItem("auth_user_id", data.user_id)
    localStorage.setItem("auth_username", data.username)

    // reconnect SSE with new credentials
    window.dispatchEvent(new Event("forge:reconnect"))
  }

  logout() {
    this.token = null
    this.userId = null
    this.username = null

    localStorage.removeItem("auth_token")
    localStorage.removeItem("auth_user_id")
    localStorage.removeItem("auth_username")

    // reconnect SSE without credentials
    window.dispatchEvent(new Event("forge:reconnect"))
  }

  hydrate() {
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

## Frontend Layout Wiring (+layout.svelte)

```svelte
<script lang="ts">
  import { ForgeProvider } from "$lib/forge"
  import { PUBLIC_API_URL } from "$env/static/public"
  import { createAuthStore } from "$lib/auth.svelte"

  let { children } = $props()

  const auth = createAuthStore()
  auth.hydrate()
</script>

<ForgeProvider url={PUBLIC_API_URL} getToken={() => auth.getToken()}>
  {@render children()}
</ForgeProvider>
```

## Login Page (+page.svelte)

```svelte
<script lang="ts">
  import { goto } from "$app/navigation"
  import { resolve } from "$app/paths"
  import { getAuthStore } from "$lib/auth.svelte"
  import { useForge, ForgeClientError } from "$lib/forge"

  const auth = getAuthStore()
  const { rpc } = useForge()

  let username = $state("")
  let password = $state("")
  let error = $state("")
  let loading = $state(false)

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault()
    error = ""
    loading = true

    try {
      const result = await rpc.login({ username, password })
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
  import { useForge, ForgeClientError } from "$lib/forge"

  const auth = getAuthStore()
  const { rpc } = useForge()

  let username = $state("")
  let password = $state("")
  let error = $state("")
  let loading = $state(false)

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault()
    error = ""
    loading = true

    try {
      const result = await rpc.register({ username, password })
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

## Redirect Pattern (authenticated main page)

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
