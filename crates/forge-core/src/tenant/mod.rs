use std::str::FromStr;

use uuid::Uuid;

use crate::ForgeError;

/// Tenant isolation mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenantIsolationMode {
    /// No tenant isolation - global access.
    #[default]
    None,
    /// Strict isolation - only see own tenant's data.
    Strict,
    /// Read shared - can read global data, writes are tenant-scoped.
    ReadShared,
}

impl TenantIsolationMode {
    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Strict => "strict",
            Self::ReadShared => "read_shared",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTenantIsolationModeError(pub String);

impl std::fmt::Display for ParseTenantIsolationModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid tenant isolation mode: '{}'", self.0)
    }
}

impl std::error::Error for ParseTenantIsolationModeError {}

impl FromStr for TenantIsolationMode {
    type Err = ParseTenantIsolationModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "strict" => Ok(Self::Strict),
            "read_shared" => Ok(Self::ReadShared),
            _ => Err(ParseTenantIsolationModeError(s.to_string())),
        }
    }
}

/// Tenant context for multi-tenancy support.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TenantContext {
    /// Current tenant ID.
    pub tenant_id: Option<Uuid>,
    /// Isolation mode.
    pub isolation_mode: TenantIsolationMode,
}

impl TenantContext {
    /// Create an empty tenant context (no tenant).
    pub fn none() -> Self {
        Self {
            tenant_id: None,
            isolation_mode: TenantIsolationMode::None,
        }
    }

    /// Create a tenant context with a specific tenant.
    pub fn new(tenant_id: Uuid, isolation_mode: TenantIsolationMode) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            isolation_mode,
        }
    }

    /// Create a strict tenant context.
    pub fn strict(tenant_id: Uuid) -> Self {
        Self::new(tenant_id, TenantIsolationMode::Strict)
    }

    /// Check if tenant context is set.
    pub fn has_tenant(&self) -> bool {
        self.tenant_id.is_some()
    }

    /// Require a tenant ID, returning an error if not set.
    pub fn require_tenant(&self) -> crate::Result<Uuid> {
        self.tenant_id
            .ok_or_else(|| ForgeError::Unauthorized("Tenant context required".into()))
    }

    /// Check if isolation mode requires filtering.
    pub fn requires_filtering(&self) -> bool {
        self.tenant_id.is_some() && self.isolation_mode != TenantIsolationMode::None
    }

    /// Generate a SQL WHERE clause for tenant filtering with parameterized query.
    /// Returns a tuple of (SQL clause, parameter value) for safe parameterized queries.
    ///
    /// The column name is validated to contain only safe SQL identifier characters
    /// (alphanumeric and underscores) to prevent SQL injection.
    pub fn sql_filter(&self, column: &str, param_index: u32) -> Option<(String, Uuid)> {
        // Validate column name to prevent SQL injection via dynamic column names
        if column.is_empty()
            || !column
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }
        self.tenant_id
            .map(|id| (format!("\"{}\" = ${}", column, param_index), id))
    }
}

/// Trait for contexts that have tenant information.
pub trait HasTenant {
    /// Get the tenant context.
    fn tenant(&self) -> &TenantContext;

    /// Get the tenant ID if set.
    fn tenant_id(&self) -> Option<Uuid> {
        self.tenant().tenant_id
    }

    /// Require a tenant ID.
    fn require_tenant(&self) -> crate::Result<Uuid> {
        self.tenant().require_tenant()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_none() {
        let ctx = TenantContext::none();
        assert!(!ctx.has_tenant());
        assert!(!ctx.requires_filtering());
    }

    #[test]
    fn test_tenant_context_strict() {
        let tenant_id = Uuid::new_v4();
        let ctx = TenantContext::strict(tenant_id);
        assert!(ctx.has_tenant());
        assert!(ctx.requires_filtering());
        assert_eq!(ctx.tenant_id, Some(tenant_id));
    }

    #[test]
    fn test_tenant_sql_filter() {
        let tenant_id = Uuid::new_v4();
        let ctx = TenantContext::strict(tenant_id);
        let filter = ctx.sql_filter("tenant_id", 1);
        assert!(filter.is_some());
        let (clause, id) = filter.unwrap();
        assert_eq!(clause, "\"tenant_id\" = $1");
        assert_eq!(id, tenant_id);
    }

    #[test]
    fn test_require_tenant() {
        let ctx = TenantContext::none();
        assert!(ctx.require_tenant().is_err());

        let tenant_id = Uuid::new_v4();
        let ctx = TenantContext::strict(tenant_id);
        assert!(ctx.require_tenant().is_ok());
    }
}
