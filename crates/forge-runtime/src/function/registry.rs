use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use forge_core::{
    ForgeMutation, ForgeQuery, FunctionInfo, FunctionKind, MutationContext, QueryContext, Result,
};
use serde_json::Value;

/// Normalize args for deserialization.
/// - Keeps `null` as-is so unit `()` deserializes correctly.
/// - Unwraps `{"args": ...}` or `{"input": ...}` wrapper if present (callers may use either format).
fn normalize_args(args: Value) -> Value {
    match &args {
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
    }
}

/// Type alias for a boxed function that executes with JSON args and returns JSON result.
pub type BoxedQueryFn = Arc<
    dyn Fn(&QueryContext, Value) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>
        + Send
        + Sync,
>;

pub type BoxedMutationFn = Arc<
    dyn Fn(&MutationContext, Value) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + '_>>
        + Send
        + Sync,
>;

/// Entry in the function registry.
pub enum FunctionEntry {
    Query {
        info: FunctionInfo,
        handler: BoxedQueryFn,
    },
    Mutation {
        info: FunctionInfo,
        handler: BoxedMutationFn,
    },
}

impl FunctionEntry {
    pub fn info(&self) -> &FunctionInfo {
        match self {
            FunctionEntry::Query { info, .. } => info,
            FunctionEntry::Mutation { info, .. } => info,
        }
    }

    pub fn kind(&self) -> FunctionKind {
        match self {
            FunctionEntry::Query { .. } => FunctionKind::Query,
            FunctionEntry::Mutation { .. } => FunctionKind::Mutation,
        }
    }
}

/// Registry of all FORGE functions.
#[derive(Clone)]
pub struct FunctionRegistry {
    functions: HashMap<String, FunctionEntry>,
}

impl Clone for FunctionEntry {
    fn clone(&self) -> Self {
        match self {
            FunctionEntry::Query { info, handler } => FunctionEntry::Query {
                info: info.clone(),
                handler: Arc::clone(handler),
            },
            FunctionEntry::Mutation { info, handler } => FunctionEntry::Mutation {
                info: info.clone(),
                handler: Arc::clone(handler),
            },
        }
    }
}

impl FunctionRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Register a query function.
    pub fn register_query<Q: ForgeQuery>(&mut self)
    where
        Q::Args: serde::de::DeserializeOwned + Send + 'static,
        Q::Output: serde::Serialize + Send + 'static,
    {
        let info = Q::info();
        let name = info.name.to_string();

        let handler: BoxedQueryFn = Arc::new(move |ctx, args| {
            Box::pin(async move {
                let parsed_args: Q::Args = serde_json::from_value(normalize_args(args))
                    .map_err(|e| forge_core::ForgeError::Validation(e.to_string()))?;
                let result = Q::execute(ctx, parsed_args).await?;
                serde_json::to_value(result)
                    .map_err(|e| forge_core::ForgeError::Internal(e.to_string()))
            })
        });

        self.functions
            .insert(name, FunctionEntry::Query { info, handler });
    }

    /// Register a mutation function.
    pub fn register_mutation<M: ForgeMutation>(&mut self)
    where
        M::Args: serde::de::DeserializeOwned + Send + 'static,
        M::Output: serde::Serialize + Send + 'static,
    {
        let info = M::info();
        let name = info.name.to_string();

        let handler: BoxedMutationFn = Arc::new(move |ctx, args| {
            Box::pin(async move {
                let parsed_args: M::Args = serde_json::from_value(normalize_args(args))
                    .map_err(|e| forge_core::ForgeError::Validation(e.to_string()))?;
                let result = M::execute(ctx, parsed_args).await?;
                serde_json::to_value(result)
                    .map_err(|e| forge_core::ForgeError::Internal(e.to_string()))
            })
        });

        self.functions
            .insert(name, FunctionEntry::Mutation { info, handler });
    }

    /// Get a function by name.
    pub fn get(&self, name: &str) -> Option<&FunctionEntry> {
        self.functions.get(name)
    }

    /// Get all function names.
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(|s| s.as_str())
    }

    /// Get all functions.
    pub fn functions(&self) -> impl Iterator<Item = (&str, &FunctionEntry)> {
        self.functions.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get the number of registered functions.
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Get all queries.
    pub fn queries(&self) -> impl Iterator<Item = (&str, &FunctionInfo)> {
        self.functions.iter().filter_map(|(name, entry)| {
            if let FunctionEntry::Query { info, .. } = entry {
                Some((name.as_str(), info))
            } else {
                None
            }
        })
    }

    /// Get all mutations.
    pub fn mutations(&self) -> impl Iterator<Item = (&str, &FunctionInfo)> {
        self.functions.iter().filter_map(|(name, entry)| {
            if let FunctionEntry::Mutation { info, .. } = entry {
                Some((name.as_str(), info))
            } else {
                None
            }
        })
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
