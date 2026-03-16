use std::sync::Arc;

use rusqlite::Connection;
use semver::Version;
use thiserror::Error;
use tracing::info;
use umadb_dcb::{DCBQuery, DCBQueryItem};
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, ResourceAny},
};
use wasmtime_wasi::{ResourceTable, WasiCtx};

use crate::{module_store::ModuleType, wit};

#[derive(Debug, Error)]
pub enum ModuleError<E> {
    #[error("concurrent modification")]
    ConcurrentModification,
    #[error("database error: {0}")]
    Database(#[from] umari_core::error::SqliteError),
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
    #[error(transparent)]
    Wit(E),
}

impl<E> From<rusqlite::Error> for ModuleError<E> {
    fn from(err: rusqlite::Error) -> Self {
        let wit_err = wit::sqlite::SqliteError::from(err);
        ModuleError::Database(wit_err.into())
    }
}

pub trait Module: Sized {
    type State: 'static;
    type Error;

    fn instantiate_async(
        store: &mut Store<Self::State>,
        component: &Component,
        linker: &Linker<Self::State>,
    ) -> impl Future<Output = wasmtime::Result<Self>>;
}

pub trait SqliteModule: Module<State = wit::SqliteComponentState> {
    fn construct(
        &self,
        store: &mut Store<wit::SqliteComponentState>,
    ) -> impl Future<Output = wasmtime::Result<Result<ResourceAny, Self::Error>>>;

    fn query(
        &self,
        store: &mut Store<wit::SqliteComponentState>,
        handler: ResourceAny,
    ) -> impl Future<Output = wasmtime::Result<wit::common::DcbQuery>>;
}

pub struct InstantiatedModule<M: Module> {
    pub store: Store<M::State>,
    pub instance: M,
    pub handler: ResourceAny,
    pub name: Arc<str>,
    pub version: Version,
}

impl<M: SqliteModule> InstantiatedModule<M> {
    pub async fn new_sqlite(
        engine: &Engine,
        linker: &Linker<wit::SqliteComponentState>,
        component: &Component,
        module_type: ModuleType,
        name: Arc<str>,
        version: Version,
    ) -> Result<Self, ModuleError<M::Error>> {
        let conn = Connection::open(format!("{name}-{module_type}.sqlite"))?;
        let meta_table_name = format!("{module_type}_meta");

        conn.execute_batch(&format!(
            "
            CREATE TABLE IF NOT EXISTS {meta_table_name} (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                last_position INTEGER
            );

            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL; -- Don't fsync too often
            PRAGMA temp_store = MEMORY;
            PRAGMA foreign_keys = ON;
            PRAGMA wal_autocheckpoint = 1000;
            "
        ))?;
        conn.execute(
            r#"
            INSERT INTO projection_meta (id, name, version) VALUES (?1, ?2, ?3)
            ON CONFLICT(id) DO UPDATE SET version = excluded.version
            "#,
            (1, &name, version.to_string()),
        )?;
        let last_position = conn
            .query_one(
                r#"
                SELECT last_position FROM projection_meta WHERE id = 1
                "#,
                (),
                |row| row.get::<_, Option<i64>>(0),
            )?
            .map(|n| n as u64);

        let wasi_ctx = WasiCtx::builder().inherit_stdio().inherit_args().build();
        let state =
            wit::SqliteComponentState::new(wasi_ctx, ResourceTable::new(), conn, last_position);
        let mut store = Store::new(engine, state);

        let instance = M::instantiate_async(&mut store, component, linker).await?;

        store.data().conn().execute("BEGIN", [])?;

        let handler = instance
            .construct(&mut store)
            .await?
            .map_err(ModuleError::Wit)?;

        store.data().conn().execute_batch("COMMIT; BEGIN")?;

        Ok(InstantiatedModule {
            store,
            instance,
            handler,
            name,
            version,
        })
    }

    pub async fn query(&mut self) -> Result<DCBQuery, ModuleError<M::Error>> {
        let query = self.instance.query(&mut self.store, self.handler).await?;

        Ok(DCBQuery {
            items: query
                .items
                .into_iter()
                .map(|item| DCBQueryItem {
                    types: item.types,
                    tags: item.tags,
                })
                .collect(),
        })
    }

    pub async fn update_last_position(
        &mut self,
        new_position: u64,
    ) -> Result<(), ModuleError<M::Error>> {
        let data = self.store.data_mut();

        let expected_position = data.last_position().map(|n| n as i64);
        let rows = data.conn().execute(
            "
            UPDATE projection_meta
            SET last_position = ?1
            WHERE id = 1
            AND last_position IS NOT DISTINCT FROM ?2
            ",
            (new_position as i64, expected_position),
        )?;

        if rows == 0 {
            return Err(ModuleError::ConcurrentModification);
        }

        data.conn().execute_batch("COMMIT; BEGIN")?;
        data.update_last_position(Some(new_position));
        info!(
            name = %self.name,
            version = %self.version,
            last_position = ?expected_position,
            new_position,
            "projection committed batch"
        );

        Ok(())
    }

    pub fn last_position(&self) -> Option<u64> {
        self.store.data().last_position()
    }
}
