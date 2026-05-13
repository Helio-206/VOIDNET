use std::path::Path;
use thiserror::Error;

#[derive(Clone)]
pub struct VoidStore {
    db: sled::Db,
}

impl VoidStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Ok(Self {
            db: sled::open(path)?,
        })
    }

    pub fn bucket(&self, name: impl AsRef<[u8]>) -> Result<Bucket, StorageError> {
        Ok(Bucket {
            tree: self.db.open_tree(name)?,
        })
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush()?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Bucket {
    tree: sled::Tree,
}

impl Bucket {
    pub fn put(&self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<(), StorageError> {
        self.tree.insert(key, value.as_ref())?;
        Ok(())
    }

    pub fn get(&self, key: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.tree.get(key)?.map(|bytes| bytes.to_vec()))
    }

    pub fn remove(&self, key: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.tree.remove(key)?.map(|bytes| bytes.to_vec()))
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("VOID storage failed: {0}")]
    Sled(#[from] sled::Error),
}
