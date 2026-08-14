use std::path::Path;

use tephra::{
    SegmentConfig, SegmentSet, WriteCoordinator, WriteHandle, WriterConfig, index::IndexError,
    log::set::LogError,
};
use thiserror::Error;

pub mod command;
pub mod compile_cache;
pub mod events;
pub mod metrics;
pub mod module;
pub mod module_store;
pub mod output;
pub mod supervisor;
pub mod wit;
pub mod worker;

#[derive(Debug, Error)]
pub enum TephraError {
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error(transparent)]
    Log(#[from] LogError),
}

/// Opens the embedded Tephra event store under `<data_dir>/events`.
///
/// The returned [`WriteCoordinator`] owns the single writer thread and must be kept alive for
/// the whole process; dropping it shuts the writer down. The [`WriteHandle`] is cheaply
/// cloneable and is shared across the runtime, API, and UI.
pub fn open_event_store(data_dir: &Path) -> Result<(WriteCoordinator, WriteHandle), TephraError> {
    let set = SegmentSet::open(data_dir.join("events"), SegmentConfig::new(256 * 1024 * 1024))?;
    let (coordinator, handle) = WriteCoordinator::start(set, WriterConfig::default())?;
    Ok((coordinator, handle))
}
