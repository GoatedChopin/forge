use std::sync::Arc;

use crate::function::AuthContext;

/// Extension point for role resolution.
///
/// The default implementation returns the flat `roles` list from the JWT
/// as parsed into [`AuthContext`]. Apps that need hierarchy expansion, group
/// membership lookups, or remote permission services can register a custom
/// resolver via `ForgeBuilder::with_role_resolver`.
///
/// The resolver is called for every request with a `require_role` constraint.
/// Keep implementations cheap — cache remote lookups externally.
///
/// # Example
///
/// ```ignore
/// struct HierarchyResolver;
///
/// impl RoleResolver for HierarchyResolver {
///     fn resolve(&self, auth: &AuthContext) -> Vec<String> {
///         let mut roles = auth.roles().to_vec();
///         if roles.contains(&"admin".to_string()) {
///             roles.extend(["editor", "viewer"].map(String::from));
///         }
///         roles
///     }
/// }
///
/// Forge::builder()
///     .with_role_resolver(Arc::new(HierarchyResolver))
///     .build()?
///     .run()
///     .await
/// ```
pub trait RoleResolver: Send + Sync + 'static {
    /// Return the effective roles for a request's auth context.
    ///
    /// Called once per `require_role` check. The returned vec is not cached
    /// between calls — if your implementation is expensive, cache internally.
    fn resolve(&self, auth: &AuthContext) -> Vec<String>;
}

/// Default resolver — returns the `roles` JWT claim as-is.
pub struct DefaultRoleResolver;

impl RoleResolver for DefaultRoleResolver {
    fn resolve(&self, auth: &AuthContext) -> Vec<String> {
        auth.roles().to_vec()
    }
}

/// Shared resolver handle used throughout the runtime.
pub type SharedRoleResolver = Arc<dyn RoleResolver>;

/// Create a shared handle to the default resolver.
pub fn default_role_resolver() -> SharedRoleResolver {
    Arc::new(DefaultRoleResolver)
}
