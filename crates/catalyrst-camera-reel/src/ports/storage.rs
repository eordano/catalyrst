use bytes::Bytes;

use catalyrst_hashing::hash_bytes_v1;
use catalyrst_storage::{ContentStorage, StorageError};

pub struct ImageStore {
    storage: ContentStorage,
}

impl ImageStore {
    pub async fn new(base_path: impl Into<std::path::PathBuf>) -> Result<Self, StorageError> {
        Ok(Self {
            storage: ContentStorage::new(base_path).await?,
        })
    }

    pub fn hash(data: &[u8]) -> String {
        hash_bytes_v1(data)
    }

    pub async fn store(&self, data: Bytes) -> Result<String, StorageError> {
        let hash = Self::hash(&data);
        self.store_as(&hash, data).await?;
        Ok(hash)
    }

    /// `hash` must be `Self::hash(&data)`; an existing blob with that hash is the same bytes.
    pub async fn store_as(&self, hash: &str, data: Bytes) -> Result<(), StorageError> {
        match self.storage.store(hash, data).await {
            Ok(()) => Ok(()),
            Err(StorageError::Io(e)) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn retrieve(&self, hash: &str) -> Result<Option<Bytes>, StorageError> {
        self.storage.retrieve(hash).await
    }

    pub async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        self.storage.delete(hash).await
    }
}
