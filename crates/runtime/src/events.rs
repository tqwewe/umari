use std::sync::Arc;

use semver::Version;

use crate::store::ModuleType;

#[derive(Clone, Debug)]
pub enum ModuleEvent {
    Activated {
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
    },
    Deactivated {
        module_type: ModuleType,
        name: Arc<str>,
    },
}
