use std::sync::Arc;

use semver::Version;

use crate::module_store::ModuleType;

#[derive(Clone, Debug)]
pub enum ModuleEvent {
    Activated {
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
        wasm_bytes: Arc<[u8]>,
    },
    Deactivated {
        module_type: ModuleType,
        name: Arc<str>,
    },
}
