use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use forge_core::workflow::{ForgeWorkflow, WorkflowContext, WorkflowInfo};
use serde_json::Value;

/// Normalize args for deserialization.
/// - Converts `null` to `{}` so both unit `()` and empty structs deserialize correctly.
/// - Unwraps `{"args": ...}` or `{"input": ...}` wrapper if present (callers may use either format).
fn normalize_args(args: Value) -> Value {
    let unwrapped = match &args {
        Value::Object(map) if map.len() == 1 => {
            if map.contains_key("args") {
                map.get("args").cloned().unwrap_or(Value::Null)
            } else if map.contains_key("input") {
                map.get("input").cloned().unwrap_or(Value::Null)
            } else {
                args
            }
        }
        _ => args,
    };

    match &unwrapped {
        Value::Null => Value::Object(serde_json::Map::new()),
        _ => unwrapped,
    }
}

/// Type alias for boxed workflow handler function.
pub type BoxedWorkflowHandler = Arc<
    dyn Fn(
            &WorkflowContext,
            serde_json::Value,
        )
            -> Pin<Box<dyn Future<Output = forge_core::Result<serde_json::Value>> + Send + '_>>
        + Send
        + Sync,
>;

/// A registered workflow entry.
pub struct WorkflowEntry {
    /// Workflow metadata.
    pub info: WorkflowInfo,
    /// Execution handler (takes serialized input, returns serialized output).
    pub handler: BoxedWorkflowHandler,
}

impl WorkflowEntry {
    /// Create a new workflow entry from a ForgeWorkflow implementor.
    pub fn new<W: ForgeWorkflow>() -> Self
    where
        W::Input: serde::de::DeserializeOwned,
        W::Output: serde::Serialize,
    {
        Self {
            info: W::info(),
            handler: Arc::new(|ctx, input| {
                Box::pin(async move {
                    let typed_input: W::Input = serde_json::from_value(normalize_args(input))
                        .map_err(|e| forge_core::ForgeError::Validation(e.to_string()))?;
                    let result = W::execute(ctx, typed_input).await?;
                    serde_json::to_value(result).map_err(forge_core::ForgeError::from)
                })
            }),
        }
    }
}

/// Composite key for versioned workflow lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkflowVersionKey {
    pub name: String,
    pub version: String,
}

/// Registry of all workflows, supporting multiple versions per workflow name.
#[derive(Default)]
pub struct WorkflowRegistry {
    /// All entries keyed by (name, version).
    entries: HashMap<WorkflowVersionKey, WorkflowEntry>,
    /// Maps workflow name to its active version string.
    active_versions: HashMap<String, String>,
}

impl WorkflowRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            active_versions: HashMap::new(),
        }
    }

    /// Register a workflow handler.
    pub fn register<W: ForgeWorkflow>(&mut self)
    where
        W::Input: serde::de::DeserializeOwned,
        W::Output: serde::Serialize,
    {
        let entry = WorkflowEntry::new::<W>();
        let info = &entry.info;

        if info.is_active {
            self.active_versions
                .insert(info.name.to_string(), info.version.to_string());
        }

        let key = WorkflowVersionKey {
            name: info.name.to_string(),
            version: info.version.to_string(),
        };
        self.entries.insert(key, entry);
    }

    /// Get the active version entry for a workflow by name.
    /// Used when starting new runs.
    pub fn get_active(&self, name: &str) -> Option<&WorkflowEntry> {
        let version = self.active_versions.get(name)?;
        let key = WorkflowVersionKey {
            name: name.to_string(),
            version: version.clone(),
        };
        self.entries.get(&key)
    }

    /// Get a specific workflow version.
    /// Used when resuming runs pinned to a specific version.
    pub fn get_version(&self, name: &str, version: &str) -> Option<&WorkflowEntry> {
        let key = WorkflowVersionKey {
            name: name.to_string(),
            version: version.to_string(),
        };
        self.entries.get(&key)
    }

    /// Get a workflow entry by name (returns the active version).
    /// Backward-compatible with code that only knows the workflow name.
    pub fn get(&self, name: &str) -> Option<&WorkflowEntry> {
        self.get_active(name)
    }

    /// Check if a specific version+signature combination is available.
    pub fn has_version_with_signature(&self, name: &str, version: &str, signature: &str) -> bool {
        self.get_version(name, version)
            .is_some_and(|entry| entry.info.signature == signature)
    }

    /// Validate that a run can be safely resumed.
    /// Returns the matching entry, or a blocking reason.
    pub fn validate_resume(
        &self,
        name: &str,
        version: &str,
        signature: &str,
    ) -> Result<&WorkflowEntry, ResumeBlockReason> {
        // Check if any version of this workflow is registered
        let has_any = self.entries.keys().any(|k| k.name == name);
        if !has_any {
            return Err(ResumeBlockReason::MissingHandler);
        }

        let entry = self
            .get_version(name, version)
            .ok_or(ResumeBlockReason::MissingVersion)?;

        if entry.info.signature != signature {
            return Err(ResumeBlockReason::SignatureMismatch {
                expected: signature.to_string(),
                actual: entry.info.signature.to_string(),
            });
        }

        Ok(entry)
    }

    /// List all registered workflow entries.
    pub fn list(&self) -> impl Iterator<Item = &WorkflowEntry> {
        self.entries.values()
    }

    /// Get the number of registered workflow entries (all versions).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all workflow names (deduplicated).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.active_versions.keys().map(|s| s.as_str())
    }

    /// Get all registered definitions for startup persistence.
    pub fn definitions(&self) -> Vec<&WorkflowInfo> {
        self.entries.values().map(|e| &e.info).collect()
    }
}

/// Reason a workflow run cannot be resumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeBlockReason {
    /// No handler registered for this workflow name at all.
    MissingHandler,
    /// The specific version is not present in the current binary.
    MissingVersion,
    /// The version exists but its signature does not match.
    SignatureMismatch { expected: String, actual: String },
}

impl ResumeBlockReason {
    /// Convert to the corresponding WorkflowStatus.
    pub fn to_status(&self) -> forge_core::workflow::WorkflowStatus {
        match self {
            Self::MissingHandler => forge_core::workflow::WorkflowStatus::BlockedMissingHandler,
            Self::MissingVersion => forge_core::workflow::WorkflowStatus::BlockedMissingVersion,
            Self::SignatureMismatch { .. } => {
                forge_core::workflow::WorkflowStatus::BlockedSignatureMismatch
            }
        }
    }

    /// Human-readable description for the blocking_reason column.
    pub fn description(&self) -> String {
        match self {
            Self::MissingHandler => "No handler registered for this workflow".to_string(),
            Self::MissingVersion => "Workflow version not present in current binary".to_string(),
            Self::SignatureMismatch { expected, actual } => {
                format!("Signature mismatch: run expects {expected}, binary has {actual}")
            }
        }
    }
}

impl Clone for WorkflowRegistry {
    fn clone(&self) -> Self {
        Self {
            entries: self
                .entries
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        WorkflowEntry {
                            info: v.info.clone(),
                            handler: v.handler.clone(),
                        },
                    )
                })
                .collect(),
            active_versions: self.active_versions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resume_block_reasons() {
        let reason = ResumeBlockReason::MissingHandler;
        assert_eq!(
            reason.to_status(),
            forge_core::workflow::WorkflowStatus::BlockedMissingHandler
        );

        let reason = ResumeBlockReason::SignatureMismatch {
            expected: "abc".to_string(),
            actual: "def".to_string(),
        };
        assert_eq!(
            reason.to_status(),
            forge_core::workflow::WorkflowStatus::BlockedSignatureMismatch
        );
        assert!(reason.description().contains("abc"));
        assert!(reason.description().contains("def"));
    }
}
