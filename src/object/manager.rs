use super::traits::{ObjectStore, ObjectStorePresign, ObjectError};
use super::local::LocalFsStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct ObjectManager {
    // Config: Map Namespace ID -> Backend impl
    // For now, hardcode or dynamic?
    // In V1, we likely construct backends on demand from DB config or cache them.
    // Let's cache instantiated backends.
    stores: Mutex<HashMap<String, Arc<dyn ObjectStorePresign>>>,
}

impl ObjectManager {
    pub fn new() -> Self {
        Self {
            stores: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve an ObjectStore for a given namespace configuration.
    /// In a real system, we'd look up the namespace in DB to get backend config.
    /// For this V1, let's assume the caller passes the backend config or we mock lookup.
    /// Actually, execution layer will look up `__object_namespaces`. 
    /// So `get_store` should take the backend config details.
    pub fn get_store(
        &self, 
        namespace_id: &str, 
        backend_type: &str, 
        root_path: &str
    ) -> Result<Arc<dyn ObjectStorePresign>, ObjectError> {
        let mut cache = self.stores.lock().unwrap();
        
        if let Some(store) = cache.get(namespace_id) {
            return Ok(store.clone());
        }

        let store: Arc<dyn ObjectStorePresign> = match backend_type {
            "local" => Arc::new(LocalFsStore::new(root_path)?),
             // "s3" => ...
            _ => return Err(ObjectError::BackendError(format!("Unknown backend type: {}", backend_type))),
        };

        cache.insert(namespace_id.to_string(), store.clone());
        Ok(store)
    }
}
