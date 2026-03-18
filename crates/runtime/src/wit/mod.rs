#[cfg(debug_assertions)]
use std::thread;

use rusqlite::{Connection, Statement};
use slotmap::{DefaultKey, SlotMap};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

pub mod command;
pub mod common;
pub mod effect;
pub mod policy;
pub mod projector;
pub mod sqlite;

pub struct BasicComponentState {
    pub wasi_ctx: WasiCtx,
    pub resource_table: ResourceTable,
}

impl WasiView for BasicComponentState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

pub struct SqliteComponentState {
    wasi_ctx: WasiCtx,
    resource_table: ResourceTable,
    conn: Connection,
    last_position: Option<u64>,
    statements: SlotMap<DefaultKey, Box<Statement<'static>>>,
    #[cfg(debug_assertions)]
    thread_id: thread::ThreadId,
}

impl SqliteComponentState {
    /// Creates a new SqliteComponentState.
    /// In debug builds, captures the current thread ID for verification.
    pub fn new(
        wasi_ctx: WasiCtx,
        resource_table: ResourceTable,
        conn: Connection,
        last_position: Option<u64>,
    ) -> Self {
        Self {
            wasi_ctx,
            resource_table,
            conn,
            last_position,
            statements: SlotMap::new(),
            #[cfg(debug_assertions)]
            thread_id: std::thread::current().id(),
        }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn last_position(&self) -> Option<u64> {
        self.last_position
    }

    pub fn update_last_position(&mut self, last_position: Option<u64>) {
        self.last_position = last_position;
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
unsafe impl Send for SqliteComponentState {}

impl WasiView for SqliteComponentState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}
