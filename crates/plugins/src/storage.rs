//! Host-provided plugin storage, mirroring `PluginStorage`
//! (canvas-plugin.ts:70) and `createPluginStorage` (canvas-event-bus.ts:32).
//!
//! The web implementation is async localforage with one store per plugin id.
//! The native trait is synchronous: the host decides whether the backing store
//! is memory, a file, or a database, and the wrapper below provides the
//! per-plugin namespace isolation the web gets from `storeName`.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::Value;
use thiserror::Error;

/// Storage failures surfaced to the host.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin storage error: {0}")]
    Storage(String),
}

/// Async in the web, synchronous here. Keys are scoped by the implementation
/// (use [`NamespacedStorage`] to scope one backend per plugin).
pub trait PluginStorage: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Value>, PluginError>;
    fn set(&self, key: &str, value: Value) -> Result<(), PluginError>;
    fn remove(&self, key: &str) -> Result<(), PluginError>;
}

/// One plugin's view of a shared storage backend. The namespace is prefixed
/// onto every key so two plugins cannot collide.
#[derive(Clone)]
pub struct NamespacedStorage {
    inner: Arc<dyn PluginStorage>,
    namespace: String,
}

impl NamespacedStorage {
    pub fn new(inner: Arc<dyn PluginStorage>, plugin_id: impl Into<String>) -> Self {
        Self {
            inner,
            namespace: plugin_id.into(),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    fn key(&self, key: &str) -> String {
        format!("{}::{key}", self.namespace)
    }
}

impl PluginStorage for NamespacedStorage {
    fn get(&self, key: &str) -> Result<Option<Value>, PluginError> {
        self.inner.get(&self.key(key))
    }

    fn set(&self, key: &str, value: Value) -> Result<(), PluginError> {
        self.inner.set(&self.key(key), value)
    }

    fn remove(&self, key: &str) -> Result<(), PluginError> {
        self.inner.remove(&self.key(key))
    }
}

/// In-memory backend for tests and for hosts that do not persist plugin state.
#[derive(Clone, Default)]
pub struct MemoryPluginStore {
    entries: Arc<RwLock<BTreeMap<String, Value>>>,
}

impl MemoryPluginStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Per-plugin view over this store (`createPluginStorage(pluginId)`).
    pub fn storage(&self, plugin_id: &str) -> NamespacedStorage {
        NamespacedStorage::new(Arc::new(self.clone()), plugin_id)
    }

    fn read(&self) -> RwLockReadGuard<'_, BTreeMap<String, Value>> {
        match self.entries.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write(&self) -> RwLockWriteGuard<'_, BTreeMap<String, Value>> {
        match self.entries.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl PluginStorage for MemoryPluginStore {
    fn get(&self, key: &str) -> Result<Option<Value>, PluginError> {
        Ok(self.read().get(key).cloned())
    }

    fn set(&self, key: &str, value: Value) -> Result<(), PluginError> {
        self.write().insert(key.to_string(), value);
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), PluginError> {
        self.write().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_store_round_trips_values() {
        let store = MemoryPluginStore::new();
        assert!(store.get("missing").is_ok_and(|value| value.is_none()));
        assert!(store.set("k", json!({ "a": 1 })).is_ok());
        assert_eq!(store.get("k").ok().flatten(), Some(json!({ "a": 1 })));
        assert!(store.remove("k").is_ok());
        assert_eq!(store.get("k").ok().flatten(), None);
    }

    #[test]
    fn namespaces_isolate_the_same_key() {
        let store = MemoryPluginStore::new();
        let alpha = store.storage("alpha");
        let beta = store.storage("beta");
        assert!(alpha.set("color", json!("red")).is_ok());
        assert!(beta.set("color", json!("blue")).is_ok());
        assert_eq!(alpha.get("color").ok().flatten(), Some(json!("red")));
        assert_eq!(beta.get("color").ok().flatten(), Some(json!("blue")));
    }
}
