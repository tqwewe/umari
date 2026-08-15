use std::path::Path;

use tephra::{
    SegmentConfig, SegmentSet, WriteCoordinator, WriteHandle, WriterConfig, index::IndexError,
    log::set::LogError,
};
use thiserror::Error;

pub mod command;
pub mod compile_cache;
#[cfg(test)]
mod e2e_tests;
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
    let set = SegmentSet::open(
        data_dir.join("events"),
        SegmentConfig::new(256 * 1024 * 1024),
    )?;
    let (coordinator, handle) = WriteCoordinator::start(set, WriterConfig::default())?;
    Ok((coordinator, handle))
}

#[cfg(test)]
pub(crate) mod test_support {
    use tempfile::TempDir;
    use tephra::{Event, EventType, Tag, Tags, WriteCoordinator, WriteHandle};

    use crate::open_event_store;

    /// A temp-dir Tephra store kept alive for the duration of a test. Field drop order
    /// (handle, then coordinator, then dir) shuts the writer down before the data dir is
    /// removed.
    pub(crate) struct TestStore {
        pub handle: WriteHandle,
        _coordinator: WriteCoordinator,
        _dir: TempDir,
    }

    /// Opens a fresh embedded Tephra store backed by a temp dir.
    pub(crate) fn test_store() -> TestStore {
        let dir = TempDir::new().unwrap();
        let (coordinator, handle) = open_event_store(dir.path()).unwrap();
        TestStore {
            handle,
            _coordinator: coordinator,
            _dir: dir,
        }
    }

    /// Builds a Tephra event from a type name, tag strings, and a raw payload.
    pub(crate) fn event(ty: &str, tags: &[&str], payload: &[u8]) -> Event {
        let event_type = EventType::new(ty).unwrap();
        let tags = Tags::new(tags.iter().map(|tag| Tag::new(*tag).unwrap())).unwrap();
        Event::new(&event_type, &tags, payload).unwrap()
    }
}

#[cfg(test)]
mod store_integration_tests {
    use tephra::{Position, Query, QueryItem, Tag, Tags};

    use crate::test_support::{event, test_store};

    fn tagged_query(tag: &str) -> Query {
        Query::item(QueryItem::with_tags(
            Tags::new([Tag::new(tag).unwrap()]).unwrap(),
        ))
    }

    fn positions(events: &[(Position, tephra::Event)]) -> Vec<u64> {
        events.iter().map(|(pos, _)| pos.get()).collect()
    }

    #[test]
    fn read_is_oldest_first_and_read_back_is_newest_first() {
        let store = test_store();
        let handle = &store.handle;
        for _ in 0..5 {
            handle
                .append(vec![event("E", &["s:1"], b"{}")], None)
                .unwrap();
        }
        let query = tagged_query("s:1");

        let forward = handle
            .read(&query, Position::ZERO, None)
            .collect_owned()
            .unwrap();
        assert_eq!(positions(&forward), vec![1, 2, 3, 4, 5]);

        // From the tip, newest-first, windowed to the two most recent.
        let back = handle
            .read_back(&query, Position::MAX, Some(2))
            .collect_owned()
            .unwrap();
        assert_eq!(positions(&back), vec![5, 4]);
    }

    #[test]
    fn a_filtered_query_selects_only_its_events() {
        let store = test_store();
        let handle = &store.handle;
        handle
            .append(vec![event("E", &["s:1"], b"{}")], None)
            .unwrap();
        handle
            .append(vec![event("E", &["s:2"], b"{}")], None)
            .unwrap();
        handle
            .append(vec![event("E", &["s:1"], b"{}")], None)
            .unwrap();

        let only_s1 = handle
            .read(&tagged_query("s:1"), Position::ZERO, None)
            .collect_owned()
            .unwrap();
        assert_eq!(positions(&only_s1), vec![1, 3]);

        let only_s2 = handle
            .read(&tagged_query("s:2"), Position::ZERO, None)
            .collect_owned()
            .unwrap();
        assert_eq!(positions(&only_s2), vec![2]);
    }
}
