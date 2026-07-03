use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::provider::IOProvider;

/// File system operation request.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub enum FsRequest {
    Read { path: PathBuf },
    Write { path: PathBuf, content: String },
    Exists { path: PathBuf },
    Remove { path: PathBuf },
    ListDir { path: PathBuf },
}

/// File system operation result.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum FsResult {
    Content(String),
    Written,
    Exists(bool),
    Removed,
    Entries(Vec<PathBuf>),
}

/// In-memory mock file system.
///
/// Stores files in a HashMap. Supports read, write, exists, remove, and list operations.
///
/// ```
/// use servyi_ioprovider::{MockFileSystem, IOProvider, filesystem::{FsRequest, FsResult}};
/// use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let mut fs = MockFileSystem::new();
/// fs.insert("/test/file.txt", "hello world");
///
/// let req = FsRequest::Read { path: PathBuf::from("/test/file.txt") };
/// let result = fs.invoke(req).await.unwrap();
/// assert_eq!(result, FsResult::Content("hello world".into()));
/// # });
/// ```
pub struct MockFileSystem {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl MockFileSystem {
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.lock().unwrap().insert(path.into(), content.into());
    }

    pub fn get(&self, path: &PathBuf) -> Option<String> {
        self.files.lock().unwrap().get(path).cloned()
    }
}

impl Default for MockFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IOProvider<FsRequest, FsResult> for MockFileSystem {
    async fn invoke(&self, input: FsRequest) -> Result<FsResult> {
        let mut files = self.files.lock().unwrap();
        match input {
            FsRequest::Read { path } => files
                .get(&path)
                .map(|c| FsResult::Content(c.clone()))
                .ok_or_else(|| anyhow!("file not found: {}", path.display())),
            FsRequest::Write { path, content } => {
                files.insert(path, content);
                Ok(FsResult::Written)
            }
            FsRequest::Exists { path } => Ok(FsResult::Exists(files.contains_key(&path))),
            FsRequest::Remove { path } => {
                files.remove(&path);
                Ok(FsResult::Removed)
            }
            FsRequest::ListDir { path } => {
                let entries: Vec<PathBuf> = files
                    .keys()
                    .filter(|p| p.starts_with(&path))
                    .cloned()
                    .collect();
                Ok(FsResult::Entries(entries))
            }
        }
    }
}
