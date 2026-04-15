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
