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
        let root = root.canonicalize().map_err(ObjectError::IoError)?;
        Ok(Self { root })
    }

    /// Securely resolve path relative to root, preventing traversal.
    ///
    /// The ObjectStore MUST NEVER trust a path string.
    /// All paths must be:
    /// 1. Normalized
    /// 2. Resolved against a fixed root
    /// 3. Rejected if they escape that root
    fn resolve_path(&self, path: &str) -> Result<PathBuf, ObjectError> {
        // 1. Reject absolute paths early
        if Path::new(path).is_absolute() {
             return Err(ObjectError::InvalidPath("Absolute paths not allowed".to_string()));
        }
        
        // 2. Reject '..' components for simplicity and security 
        // (S3 keys are flat strings, '..' usually indicates attack or mistake)
        if path.split('/').any(|c| c == "..") {
             return Err(ObjectError::InvalidPath("Path cannot contain '..'".to_string()));
        }

        // 3. Join and normalize
        // We use component-based resolution to avoid checking file existence for writes
        let mut full_path = self.root.clone();
        for component in Path::new(path).components() {
            match component {
                std::path::Component::Normal(c) => full_path.push(c),
                std::path::Component::CurDir => {}, // skip .
                std::path::Component::ParentDir => return Err(ObjectError::PathTraversalDetected),
                _ => return Err(ObjectError::InvalidPath("Invalid path component".to_string())),
            }
        }
        
        // 4. Enforce root containment (double check)
        // Note: fs::canonicalize requires file existence, so we use it only if verified
        // For strictness, if the path exists, we verify canonical match.
        if full_path.exists() {
             let canonical = full_path.canonicalize().map_err(ObjectError::IoError)?;
             let root_canonical = self.root.canonicalize().map_err(ObjectError::IoError)?;
             if !canonical.starts_with(&root_canonical) {
                 return Err(ObjectError::PathTraversalDetected);
             }
             Ok(canonical)
        } else {
             // If it doesn't exist (e.g. new file write), we trust the component logic + check starts_with
             if !full_path.starts_with(&self.root) {
                 return Err(ObjectError::PathTraversalDetected);
             }
             Ok(full_path)
        }
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

impl super::traits::ObjectStorePresign for LocalFsStore {
    fn presign_put(&self, _path: &str, _ttl: std::time::Duration) -> Result<super::traits::PresignedUrl, ObjectError> {
        Err(ObjectError::NotSupported("Local backend does not support presigned URLs".to_string()))
    }
    
    fn presign_get(&self, _path: &str, _ttl: std::time::Duration) -> Result<super::traits::PresignedUrl, ObjectError> {
        Err(ObjectError::NotSupported("Local backend does not support presigned URLs".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_path_security() {
        let temp_dir = TempDir::new().unwrap();
        let store = LocalFsStore::new(temp_dir.path().to_str().unwrap()).unwrap();

        // Valid paths
        assert!(store.resolve_path("file.txt").is_ok());
        assert!(store.resolve_path("folder/file.txt").is_ok());

        // Traversal attempts
        // Standard ..
        match store.resolve_path("../secret.txt") {
            Err(ObjectError::InvalidPath(_)) => {},
            res => panic!("Expected InvalidPath, got {:?}", res),
        }

        // Nested ..
        match store.resolve_path("folder/../secret.txt") {
             Err(ObjectError::InvalidPath(_)) | Err(ObjectError::PathTraversalDetected) => {},
             res => panic!("Expected PathTraversalDetected or InvalidPath, got {:?}", res),
        }

        // Absolute path
        match store.resolve_path("/etc/passwd") {
            Err(ObjectError::InvalidPath(_)) => {},
            res => panic!("Expected InvalidPath for absolute path, got {:?}", res),
        }
    }
}
