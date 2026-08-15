use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use sha2::{Digest, Sha256};
use tracing::warn;
use wasmtime::{Engine, component::Component};

pub struct CompileCache {
    dir: PathBuf,
}

impl CompileCache {
    pub fn new(data_dir: &Path) -> Arc<Self> {
        Arc::new(CompileCache {
            dir: data_dir.join("cache"),
        })
    }

    pub fn load(&self, sha256: &str) -> Option<Vec<u8>> {
        fs::read(self.path(sha256)).ok()
    }

    pub fn store(&self, sha256: &str, bytes: &[u8]) {
        let _ = fs::create_dir_all(&self.dir);
        let _ = fs::write(self.path(sha256), bytes);
    }

    fn path(&self, sha256: &str) -> PathBuf {
        self.dir.join(format!("{sha256}.cwasm"))
    }

    pub fn load_component(
        &self,
        engine: &Engine,
        wasm_bytes: &[u8],
    ) -> Result<Component, wasmtime::Error> {
        let sha256 = hex::encode(Sha256::digest(wasm_bytes));
        if let Some(cached) = self.load(&sha256) {
            match unsafe { Component::deserialize(engine, &cached) } {
                Ok(component) => return Ok(component),
                Err(err) => {
                    warn!("failed to deserialize cached component, recompiling: {err}");
                }
            }
        }
        let component = Component::new(engine, wasm_bytes)?;
        if let Ok(serialized) = component.serialize() {
            self.store(&sha256, &serialized);
        }
        Ok(component)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::CompileCache;

    #[test]
    fn store_then_load_round_trips() {
        let dir = TempDir::new().unwrap();
        let cache = CompileCache::new(dir.path());
        assert!(cache.load("deadbeef").is_none());
        cache.store("deadbeef", b"compiled-bytes");
        assert_eq!(
            cache.load("deadbeef").as_deref(),
            Some(&b"compiled-bytes"[..])
        );
    }

    #[test]
    fn store_overwrites_existing_entry() {
        let dir = TempDir::new().unwrap();
        let cache = CompileCache::new(dir.path());
        cache.store("k", b"first");
        cache.store("k", b"second");
        assert_eq!(cache.load("k").as_deref(), Some(&b"second"[..]));
    }

    #[test]
    fn distinct_keys_are_independent() {
        let dir = TempDir::new().unwrap();
        let cache = CompileCache::new(dir.path());
        cache.store("a", b"aaa");
        cache.store("b", b"bbb");
        assert_eq!(cache.load("a").as_deref(), Some(&b"aaa"[..]));
        assert_eq!(cache.load("b").as_deref(), Some(&b"bbb"[..]));
        assert!(cache.load("c").is_none());
    }
}
