use super::traits::{ObjectStore, ObjectError, ObjectEntry};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    pub fn new(root_path: &str) -> Result<Self, ObjectError> {
        let root = PathBuf::from(root_path);
        if !root.exists() {
            fs::create_dir_all(&root).map_err(ObjectError::IoError)?;
        }
        Ok(Self { root })
    }

    /// Securely resolve path relative to root, preventing transversal
    fn resolve_path(&self, path: &str) -> Result<PathBuf, ObjectError> {
        // Basic sanitization: prevent '..'
        if path.contains("..") {
            return Err(ObjectError::InvalidPath("Path cannot contain '..'".to_string()));
        }
        
        // Remove leading slashes
        let clean_path = path.trim_start_matches('/');
        let full_path = self.root.join(clean_path);
        
        Ok(full_path)
    }
}

impl ObjectStore for LocalFsStore {
    fn read(&self, path: &str) -> Result<Vec<u8>, ObjectError> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(ObjectError::NotFound(path.to_string()));
        }

        fs::read(full_path).map_err(ObjectError::IoError)
    }

    fn write(&self, path: &str, data: &[u8]) -> Result<(), ObjectError> {
        let full_path = self.resolve_path(path)?;
        
        // Ensure parent dir exists
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(ObjectError::IoError)?;
            }
        }

        let mut file = fs::File::create(full_path).map_err(ObjectError::IoError)?;
        file.write_all(data).map_err(ObjectError::IoError)?;
        
        Ok(())
    }

    fn delete(&self, path: &str) -> Result<(), ObjectError> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(ObjectError::NotFound(path.to_string()));
        }

        fs::remove_file(full_path).map_err(ObjectError::IoError)
    }

    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>, ObjectError> {
        let root_str = self.root.to_string_lossy();
        let full_prefix = self.resolve_path(prefix)?;
        
        // For local FS, list is recursive walk usually, or just single dir?
        // "Folder" semantics usually imply single dir.
        // But object store list is recursive with prefix.
        // Let's implement simple recursive walk for V1 local store simplicity.
        // Warning: Performance hazard on large trees.
        
        let mut entries = Vec::new();
        
        // If prefix matches a directory, walk it
        let start_dir = if full_prefix.exists() && full_prefix.is_dir() {
            full_prefix.clone()
        } else {
             // If prefix is partial "users/ava", we might need to search parent?
             // S3 semantics: list("users/") -> items in users/
             // list("users") -> items starting with users
             // For simplicity, let's assume prefix is a directory path for now.
             self.root.clone() 
        };

        if start_dir.exists() {
             for entry in walkdir::WalkDir::new(&start_dir) {
                let entry = entry.map_err(|e| ObjectError::BackendError(e.to_string()))?;
                if entry.file_type().is_file() {
                    let path = entry.path();
                    let rel_path = path.strip_prefix(&self.root)
                        .map_err(|_| ObjectError::BackendError("Path strip error".into()))?
                        .to_string_lossy()
                        .to_string();

                    if rel_path.starts_with(prefix) {
                        let metadata = entry.metadata().map_err(|e| ObjectError::BackendError(e.to_string()))?;
                        entries.push(ObjectEntry {
                            key: rel_path,
                            size: metadata.len(),
                            created_at: metadata.created()
                                .unwrap_or(SystemTime::now())
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64,
                        });
                    }
                }
            }
        }
        
        Ok(entries)
    }
}
