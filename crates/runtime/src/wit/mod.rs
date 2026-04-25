use std::sync::Arc;
#[cfg(debug_assertions)]
use std::thread;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, Statement};
use serde::Serialize;
use slotmap::{DefaultKey, SlotMap};
use umadb_client::AsyncUmaDbClient;
use umadb_dcb::{DcbQuery, DcbReadResponseAsync};
use uuid::Uuid;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_http::{
    WasiHttpCtx,
    p2::{WasiHttpCtxView, WasiHttpView},
};

use crate::effect_journal::EffectJournal;

pub mod command;
pub mod common;
pub mod effect;
pub mod projector;
pub mod sqlite;

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ExecuteResult {
    /// Event store position after command execution
    pub position: Option<u64>,
    /// Events emitted by the command
    pub events: Vec<EmittedEvent>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EmittedEvent {
    /// Event unique identifier
    pub id: Uuid,
    /// Event type identifier
    pub event_type: String,
    /// Domain ID tags for event categorization
    pub tags: Vec<String>,
}

pub struct CommandComponentState {
    pub wasi_ctx: WasiCtx,
    pub wasi_http_ctx: WasiHttpCtx,
    pub resource_table: ResourceTable,
    pub event_store: Arc<AsyncUmaDbClient>,
    pub timestamp: DateTime<Utc>,
    pub transactions: SlotMap<
        DefaultKey,
        (
            DcbQuery,
            Option<Box<dyn DcbReadResponseAsync + Send + 'static>>,
        ),
    >,
    pub emitted_events: Vec<EmittedEvent>,
}

impl WasiView for CommandComponentState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

impl WasiHttpView for CommandComponentState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.wasi_http_ctx,
            table: &mut self.resource_table,
            hooks: Default::default(),
        }
    }
}

pub struct EventHandlerComponentState {
    wasi_ctx: WasiCtx,
    wasi_http_ctx: WasiHttpCtx,
    resource_table: ResourceTable,
    event_store: Arc<AsyncUmaDbClient>,
    conn: Connection,
    current_event_id: Uuid,
    current_correlation_id: Uuid,
    current_event_position: u64,
    last_position: Option<u64>,
    statements: SlotMap<DefaultKey, Box<Statement<'static>>>,
    transactions: SlotMap<
        DefaultKey,
        (
            DcbQuery,
            Option<Box<dyn DcbReadResponseAsync + Send + 'static>>,
        ),
    >,
    effect_journal: Option<Box<EffectJournal>>,
    #[cfg(debug_assertions)]
    thread_id: thread::ThreadId,
}

impl EventHandlerComponentState {
    /// Creates a new SqliteComponentState.
    /// In debug builds, captures the current thread ID for verification.
    pub fn new(
        wasi_ctx: WasiCtx,
        resource_table: ResourceTable,
        event_store: Arc<AsyncUmaDbClient>,
        conn: Connection,
        last_position: Option<u64>,
    ) -> Self {
        Self {
            wasi_ctx,
            wasi_http_ctx: WasiHttpCtx::new(),
            resource_table,
            event_store,
            conn,
            current_event_id: Uuid::nil(),
            current_correlation_id: Uuid::nil(),
            current_event_position: 0,
            last_position,
            statements: SlotMap::new(),
            transactions: SlotMap::new(),
            effect_journal: None,
            #[cfg(debug_assertions)]
            thread_id: std::thread::current().id(),
        }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn current_event_id(&self) -> Uuid {
        self.current_event_id
    }

    pub fn update_current_event_id(&mut self, event_id: Uuid) {
        self.current_event_id = event_id;
    }

    pub fn current_correlation_id(&self) -> Uuid {
        self.current_correlation_id
    }

    pub fn update_current_correlation_id(&mut self, correlation_id: Uuid) {
        self.current_correlation_id = correlation_id;
    }

    pub fn last_position(&self) -> Option<u64> {
        self.last_position
    }

    pub fn update_last_position(&mut self, last_position: Option<u64>) {
        self.last_position = last_position;
    }

    pub fn current_event_position(&self) -> u64 {
        self.current_event_position
    }

    pub fn update_current_event_position(&mut self, position: u64) {
        self.current_event_position = position;
    }

    pub fn set_effect_journal(&mut self, journal: Box<EffectJournal>) {
        self.effect_journal = Some(journal);
    }

    pub fn take_effect_journal(&mut self) -> Option<Box<EffectJournal>> {
        self.effect_journal.take()
    }

    /// Checks that we're on the correct thread (debug builds only).
    /// Panics if called from a different thread than where SqliteComponentState was created.
    #[cfg(debug_assertions)]
    fn check_thread(&self) {
        let current = std::thread::current().id();
        assert_eq!(
            self.thread_id, current,
            "SqliteComponentState accessed from wrong thread! \
             Created on {:?}, accessed from {:?}. \
             This violates SQLite thread safety requirements.",
            self.thread_id, current
        );
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    fn check_thread(&self) {}
}

/// SAFETY: This type is NOT actually safe to send between threads due to the
/// SQLite connection and prepared statements having thread affinity. SQLite
/// connections and statements MUST be accessed only from the thread they were
/// created on.
///
/// This unsafe impl is ONLY sound when SqliteComponentState is used with:
/// - Actors spawned with `.spawn_in_thread()` (NOT `.spawn()`)
/// - The kameo runtime which uses `block_on()` on a dedicated OS thread
/// - No usage with `tokio::spawn()` or other multi-threaded executors
///
/// The current usage is sound because:
/// 1. ProjectorActor is spawned with `.spawn_in_thread()` which creates a
///    dedicated OS thread
/// 2. The actor runs via `Handle::block_on()` which executes all async code
///    (including wasmtime operations) on that specific thread without migrating
/// 3. Debug builds include runtime thread affinity checks that panic if this
///    type is accessed from the wrong thread
///
/// DO NOT use this type with `tokio::spawn` or change from `.spawn_in_thread()`
/// to `.spawn()`. Doing so will cause undefined behavior, data corruption, or crashes.
///
/// See: crates/runtime/src/projector/supervisor.rs lines 126 and 214
unsafe impl Send for EventHandlerComponentState {}

impl WasiView for EventHandlerComponentState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

impl WasiHttpView for EventHandlerComponentState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.wasi_http_ctx,
            table: &mut self.resource_table,
            hooks: match &mut self.effect_journal {
                Some(journal) => journal.as_mut() as &mut dyn wasmtime_wasi_http::p2::WasiHttpHooks,
                None => Default::default(),
            },
        }
    }
}
