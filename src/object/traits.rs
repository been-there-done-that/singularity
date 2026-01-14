use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum ObjectError {
    #[error("Object not found: {0}")]
    NotFound(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Backend error: {0}")]
    BackendError(String),
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Path traversal detected")]
    PathTraversalDetected,
    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub key: String,
    pub size: u64,
    pub created_at: i64,
}

pub trait ObjectStore: Send + Sync {
    /// Read an object into memory (V1: simple vec)
    fn read(&self, path: &str) -> Result<Vec<u8>, ObjectError>;
    
    /// Write data to an object path
    fn write(&self, path: &str, data: &[u8]) -> Result<(), ObjectError>;
    
    /// Delete an object
    fn delete(&self, path: &str) -> Result<(), ObjectError>;
    
    /// List objects with prefix
    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>, ObjectError>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PresignedUrl {
    pub url: String,
    pub method: String,
    pub headers: std::collections::HashMap<String, String>,
    pub expires_at: i64,
}

pub trait ObjectStorePresign: ObjectStore {
    fn presign_put(&self, path: &str, ttl: std::time::Duration) -> Result<PresignedUrl, ObjectError>;
    fn presign_get(&self, path: &str, ttl: std::time::Duration) -> Result<PresignedUrl, ObjectError>;
}
