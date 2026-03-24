//! Built-in auth + viewer state management for forge-dioxus.
//!
//! Handles token storage, viewer persistence, refresh loops, and 401
//! recovery. Apps get viewer access for free without writing their own
//! storage layer.

use dioxus::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{ConnectionState, ForgeClient, ForgeClientConfig};

/// Persisted auth data: tokens + optional viewer.
/// Backward compatible with old format (viewer is optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAuth {
    access_token: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    viewer: Option<serde_json::Value>,
}

/// Auth state tracked by the framework.
#[derive(Debug, Clone)]
pub enum ForgeAuthState {
    Unauthenticated,
    Authenticated {
        access_token: String,
        refresh_token: String,
        viewer: Option<serde_json::Value>,
    },
}

impl ForgeAuthState {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }

    pub fn access_token(&self) -> Option<String> {
        match self {
            Self::Authenticated { access_token, .. } => Some(access_token.clone()),
            Self::Unauthenticated => None,
        }
    }

    pub fn refresh_token(&self) -> Option<String> {
        match self {
            Self::Authenticated { refresh_token, .. } => Some(refresh_token.clone()),
            Self::Unauthenticated => None,
        }
    }

    fn viewer_json(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Authenticated { viewer, .. } => viewer.as_ref(),
            Self::Unauthenticated => None,
        }
    }
}

/// Auth handle provided to components via `use_forge_auth()`.
#[derive(Clone, Copy)]
pub struct ForgeAuth {
    state: Signal<ForgeAuthState>,
    app_name: Signal<String>,
    generation: Signal<u64>,
}

impl ForgeAuth {
    pub fn is_authenticated(&self) -> bool {
        self.state.read().is_authenticated()
    }

    pub fn access_token(&self) -> Option<String> {
        self.state.read().access_token()
    }

    pub fn refresh_token(&self) -> Option<String> {
        self.state.read().refresh_token()
    }

    /// Read the stored viewer, deserialized into the app's type.
    pub fn viewer<V: DeserializeOwned>(&self) -> Option<V> {
        let state = self.state.read();
        let json = state.viewer_json()?;
        serde_json::from_value(json.clone()).ok()
    }

    /// Set tokens after login/register (no viewer).
    pub fn login(&mut self, access_token: String, refresh_token: String) {
        self.save_and_set(access_token, refresh_token, None);
    }

    /// Set tokens + viewer after login/register.
    pub fn login_with_viewer<V: Serialize>(
        &mut self,
        access_token: String,
        refresh_token: String,
        viewer: &V,
    ) {
        let viewer_json = serde_json::to_value(viewer).ok();
        self.save_and_set(access_token, refresh_token, viewer_json);
    }

    /// Update tokens (e.g., after a refresh). Preserves existing viewer.
    pub fn update_tokens(&mut self, access_token: String, refresh_token: String) {
        let existing_viewer = self.state.read().viewer_json().cloned();
        self.save_and_set(access_token, refresh_token, existing_viewer);
    }

    /// Update just the viewer without touching tokens.
    pub fn update_viewer<V: Serialize>(&mut self, viewer: &V) {
        let state = self.state.read();
        let (access_token, refresh_token) = match &*state {
            ForgeAuthState::Authenticated {
                access_token,
                refresh_token,
                ..
            } => (access_token.clone(), refresh_token.clone()),
            ForgeAuthState::Unauthenticated => return,
        };
        drop(state);
        let viewer_json = serde_json::to_value(viewer).ok();
        self.save_and_set(access_token, refresh_token, viewer_json);
    }

    /// Clear tokens, viewer, and log out.
    pub fn logout(&mut self) {
        storage::clear(&self.app_name.read());
        self.state.set(ForgeAuthState::Unauthenticated);
        self.generation.with_mut(|g| *g += 1);
    }

    fn save_and_set(
        &mut self,
        access_token: String,
        refresh_token: String,
        viewer: Option<serde_json::Value>,
    ) {
        let stored = StoredAuth {
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            viewer: viewer.clone(),
        };
        storage::save(&self.app_name.read(), &stored);
        let was_authenticated = self.state.read().is_authenticated();
        self.state.set(ForgeAuthState::Authenticated {
            access_token,
            refresh_token,
            viewer,
        });
        if !was_authenticated {
            self.generation.with_mut(|g| *g += 1);
        }
    }
}

/// Read the auth handle from context.
pub fn use_forge_auth() -> ForgeAuth {
    use_context::<ForgeAuth>()
}

/// Read the stored viewer, deserialized into the app's viewer type.
/// Returns `None` when unauthenticated or if the viewer hasn't been set.
pub fn use_viewer<V: DeserializeOwned + Clone + 'static>() -> Option<V> {
    use_forge_auth().viewer::<V>()
}

/// Returns a string key that changes on login/logout transitions.
/// Use this to key your router or main content area so SSE subscriptions
/// reconnect with fresh auth state.
///
/// ```ignore
/// let auth_key = use_auth_key();
/// rsx! { main { key: "{auth_key}", Router::<Route> {} } }
/// ```
pub fn use_auth_key() -> String {
    let auth = use_forge_auth();
    let generation = auth.generation.read();
    format!("forge-auth-{generation}")
}

/// Guard hook: redirects to `redirect_path` when unauthenticated.
/// Returns `true` if authenticated, `false` during redirect.
///
/// ```ignore
/// fn ProtectedPage() -> Element {
///     if !use_require_auth("/login") { return rsx! {} }
///     // ... render protected content
/// }
/// ```
#[cfg(feature = "router")]
pub fn use_require_auth(redirect_path: &str) -> bool {
    let auth = use_forge_auth();
    let navigator = use_navigator();
    let path = redirect_path.to_string();

    use_effect(move || {
        if !auth.is_authenticated() {
            navigator.replace(NavigationTarget::Internal(path.clone()));
        }
    });

    auth.is_authenticated()
}

/// Provider component that sets up auth state, ForgeClient with auto token wiring,
/// 401 detection, and periodic refresh.
///
/// ```ignore
/// ForgeAuthProvider {
///     url: "http://localhost:9081",
///     app_name: "my-app",
///     children: rsx! { Router::<Route> {} }
/// }
/// ```
/// `refresh_interval_secs`: How often to proactively refresh tokens (default: 2400 = 40 min).
/// Set to roughly 2/3 of your `access_token_ttl` from forge.toml.
#[component]
pub fn ForgeAuthProvider(
    url: String,
    #[props(default = "forge_app".to_string())] app_name: String,
    #[props(default = 2400)] refresh_interval_secs: u64,
    children: Element,
) -> Element {
    let initial = match storage::load(&app_name) {
        Some(stored) => ForgeAuthState::Authenticated {
            access_token: stored.access_token,
            refresh_token: stored.refresh_token,
            viewer: stored.viewer,
        },
        None => ForgeAuthState::Unauthenticated,
    };

    let auth_state = use_context_provider(|| Signal::new(initial));
    let app_name_signal = use_context_provider(|| Signal::new(app_name));
    let generation = use_context_provider(|| Signal::new(0_u64));
    let forge_auth = use_context_provider(|| ForgeAuth {
        state: auth_state,
        app_name: app_name_signal,
        generation,
    });

    let connection_state = use_context_provider(|| Signal::new(ConnectionState::Disconnected));
    let needs_refresh = use_signal(|| false);

    // Build ForgeClient with auto token provider and auth error handler
    let url_clone = url.clone();
    use_context_provider(move || {
        let auth_for_token = auth_state;
        let needs_refresh_clone = needs_refresh;
        let config = ForgeClientConfig::new(url_clone)
            .with_connection_state(connection_state)
            .with_token_provider(move || auth_for_token.read().access_token())
            .with_auth_error_handler(move |_err| {
                let mut sig = needs_refresh_clone;
                sig.set(true);
            });
        ForgeClient::new(config)
    });

    // Handle 401 errors by attempting token refresh
    let url_for_refresh = url.clone();
    use_effect(move || {
        if !*needs_refresh.read() {
            return;
        }
        let url = url_for_refresh.clone();
        let mut auth = forge_auth;
        spawn(async move {
            try_refresh_tokens(&url, &mut auth).await;
        });
    });

    // Periodic refresh (default every 40 minutes, configurable via refresh_interval_secs)
    let url_for_periodic = url;
    use_future(move || {
        let url = url_for_periodic.clone();
        let mut auth = forge_auth;
        async move {
            loop {
                sleep(refresh_interval_secs).await;
                if auth.is_authenticated() {
                    try_refresh_tokens(&url, &mut auth).await;
                }
            }
        }
    });

    rsx! { {children} }
}

/// Attempt to refresh tokens using an anonymous client.
///
/// Only logs out on definitive auth failures (401/403). Network errors
/// are silently ignored so transient connectivity issues in hospital
/// networks don't force unnecessary logouts.
async fn try_refresh_tokens(api_url: &str, auth: &mut ForgeAuth) {
    let refresh_token = match auth.refresh_token() {
        Some(t) => t,
        None => return,
    };

    let anon_client = ForgeClient::new(ForgeClientConfig::new(api_url.to_string()));

    #[derive(Serialize)]
    struct RefreshArgs {
        refresh_token: String,
    }

    #[derive(Deserialize)]
    struct RefreshResponse {
        access_token: String,
        refresh_token: String,
    }

    match anon_client
        .call::<_, RefreshResponse>(
            "refresh",
            RefreshArgs {
                refresh_token,
            },
        )
        .await
    {
        Ok(resp) => {
            auth.update_tokens(resp.access_token, resp.refresh_token);
        }
        Err(ref e)
            if e.code == "UNAUTHORIZED"
                || e.code == "FORBIDDEN"
                || e.code == "NOT_FOUND" =>
        {
            // Definitive auth failure: token is invalid/expired/revoked.
            auth.logout();
        }
        Err(_) => {
            // Network or transient error. Keep current tokens and retry
            // on the next refresh cycle rather than forcing a logout.
        }
    }
}

/// Platform-specific sleep (works on both WASM and native).
async fn sleep(secs: u64) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new((secs * 1000) as u32).await;

    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
}

// Platform-specific auth storage
#[cfg(target_arch = "wasm32")]
mod storage {
    use super::StoredAuth;

    fn key(app_name: &str) -> String {
        format!("{app_name}_auth")
    }

    pub fn save(app_name: &str, auth: &StoredAuth) {
        if let Ok(json) = serde_json::to_string(auth) {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
            {
                let _ = storage.set_item(&key(app_name), &json);
            }
        }
    }

    pub fn load(app_name: &str) -> Option<StoredAuth> {
        let storage = web_sys::window()?.local_storage().ok()??;
        let json = storage.get_item(&key(app_name)).ok()??;
        serde_json::from_str(&json).ok()
    }

    pub fn clear(app_name: &str) {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            let _ = storage.remove_item(&key(app_name));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use super::StoredAuth;
    use std::fs;
    use std::path::PathBuf;

    fn storage_path(app_name: &str) -> Option<PathBuf> {
        dirs::data_local_dir().map(|base| base.join(app_name).join("auth.json"))
    }

    pub fn save(app_name: &str, auth: &StoredAuth) {
        let Some(path) = storage_path(app_name) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(auth) {
            let tmp = path.with_extension("tmp");
            let _ = fs::write(&tmp, json).and_then(|()| fs::rename(tmp, path));
        }
    }

    pub fn load(app_name: &str) -> Option<StoredAuth> {
        let path = storage_path(app_name)?;
        let json = fs::read_to_string(path).ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn clear(app_name: &str) {
        if let Some(path) = storage_path(app_name) {
            let _ = fs::remove_file(path);
        }
    }
}
