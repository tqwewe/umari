use std::collections::HashMap;

use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Row, ToSql, params,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Type, ValueRef},
};
use semver::Version;
use sha2::{Digest, Sha256};

use super::{Module, ModuleStoreError, ModuleType, ModuleVersionInfo};

pub struct SqliteModuleStore {
    conn: Connection,
}

impl SqliteModuleStore {
    pub fn new(conn: Connection) -> Self {
        SqliteModuleStore { conn }
    }

    pub fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS modules (
                id INTEGER PRIMARY KEY,

                module_type TEXT NOT NULL,
                name TEXT NOT NULL,
                version TEXT NOT NULL,

                wasm_bytes BLOB NOT NULL,
                sha256 TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),

                UNIQUE(module_type, name, version)
            );

            CREATE TABLE IF NOT EXISTS active_modules (
                module_type TEXT NOT NULL,
                name TEXT NOT NULL,
                module_id INTEGER NOT NULL,
                PRIMARY KEY(module_type, name)
            );

            CREATE TABLE IF NOT EXISTS module_env_vars (
                module_type TEXT NOT NULL,
                name        TEXT NOT NULL,
                key         TEXT NOT NULL,
                value       TEXT NOT NULL,
                PRIMARY KEY (module_type, name, key)
            );
            "#,
        )?;

        Ok(())
    }
}

impl SqliteModuleStore {
    pub fn save_module(
        &self,
        module_type: ModuleType,
        name: &str,
        version: Version,
        wasm_bytes: &[u8],
    ) -> Result<(), ModuleStoreError> {
        if !is_valid_module_name(name) {
            return Err(ModuleStoreError::InvalidName(name.to_string()));
        }

        let sha256 = hex::encode(Sha256::digest(wasm_bytes));
        self.conn
            .execute(
                r#"
                INSERT INTO modules (
                    module_type,
                    name,
                    version,
                    wasm_bytes,
                    sha256
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![module_type, name, version.to_string(), wasm_bytes, sha256],
            )
            .map_err(|err| match err {
                rusqlite::Error::SqliteFailure(err, _msg)
                    if err.code == ErrorCode::ConstraintViolation =>
                {
                    ModuleStoreError::ModuleAlreadyExists
                }
                err => ModuleStoreError::Database(err),
            })?;

        Ok(())
    }

    pub fn load_module(
        &self,
        module_type: ModuleType,
        name: &str,
        version: Version,
    ) -> Result<Option<(Vec<u8>, String)>, ModuleStoreError> {
        let Some((wasm_bytes, sha256)) = self.conn.query_row(
            r#"
            SELECT wasm_bytes, sha256 FROM modules WHERE module_type = ?1 AND name = ?2 AND version = ?3
            "#,
            params![module_type, name, version.to_string()],
            |row| {
                let wasm_bytes: Vec<u8> = row.get(0)?;
                let sha256: String = row.get(1)?;
                Ok((wasm_bytes, sha256))
            },
        ).optional()? else {
            return Ok(None);
        };

        let computed_sha256 = hex::encode(Sha256::digest(&wasm_bytes));
        if sha256 != computed_sha256 {
            return Err(ModuleStoreError::Integrity(format!(
                "sha256 mismatch: expected {computed_sha256}, got {sha256}"
            )));
        }

        Ok(Some((wasm_bytes, sha256)))
    }

    pub fn activate_module(
        &mut self,
        module_type: ModuleType,
        name: &str,
        version: Version,
    ) -> Result<bool, ModuleStoreError> {
        let tx = self.conn.transaction()?;

        let module_id: i64 = tx
            .query_row(
                "SELECT id FROM modules WHERE module_type = ?1 AND name = ?2 AND version = ?3",
                params![module_type, name, version.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| ModuleStoreError::ModuleNotFound {
                module_type,
                name: name.to_string(),
                version,
            })?;

        let rows_affected = tx.execute(
            r#"
            INSERT INTO active_modules (module_type, name, module_id)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(module_type, name)
            DO UPDATE SET module_id = excluded.module_id;
            "#,
            params![module_type, name, module_id],
        )?;

        tx.commit()?;

        Ok(rows_affected > 0)
    }

    pub fn get_active_module(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<Option<(Version, Vec<u8>)>, ModuleStoreError> {
        let result = self.conn.query_row(
            r#"
            SELECT m.version, m.wasm_bytes
            FROM active_modules a
            JOIN modules m ON a.module_id = m.id
            WHERE a.module_type = ?1 AND a.name = ?2
            "#,
            params![module_type, name],
            |row| {
                let version_str: String = row.get(0)?;
                let version = version_str.parse::<Version>().map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
                })?;
                let wasm_bytes: Vec<u8> = row.get(1)?;
                Ok((version, wasm_bytes))
            },
        );

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(ModuleStoreError::Database(err)),
        }
    }

    pub fn deactivate_module(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<bool, ModuleStoreError> {
        let rows_affected = self.conn.execute(
            "DELETE FROM active_modules WHERE module_type = ?1 AND name = ?2",
            params![module_type, name],
        )?;

        Ok(rows_affected > 0)
    }

    pub fn get_all_active_modules(
        &self,
        module_type: Option<ModuleType>,
    ) -> Result<Vec<Module>, ModuleStoreError> {
        let map_fn = |row: &Row| {
            let module_type: ModuleType = row.get(0)?;
            let name: String = row.get(1)?;
            let version_str: String = row.get(2)?;
            let version = version_str.parse::<Version>().map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(2, Type::Text, Box::new(err))
            })?;
            let sha256: String = row.get(3)?;
            let wasm_bytes: Vec<u8> = row.get(4)?;
            Ok(Module {
                module_type,
                name,
                version,
                sha256,
                wasm_bytes,
            })
        };

        match module_type {
            Some(module_type) => self
                .conn
                .prepare(
                    r#"
                    SELECT a.module_type, a.name, m.version, m.sha256, m.wasm_bytes
                    FROM active_modules a
                    JOIN modules m ON a.module_id = m.id
                    WHERE a.module_type = ?1
                    "#,
                )?
                .query_map([module_type], map_fn)?
                .collect::<Result<Vec<_>, _>>(),
            None => self
                .conn
                .prepare(
                    r#"
                    SELECT a.module_type, a.name, m.version, m.sha256, m.wasm_bytes
                    FROM active_modules a
                    JOIN modules m ON a.module_id = m.id
                    "#,
                )?
                .query_map([], map_fn)?
                .collect::<Result<Vec<_>, _>>(),
        }
        .map_err(ModuleStoreError::Database)
    }

    pub fn get_module_versions(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<Vec<ModuleVersionInfo>, ModuleStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT version, sha256 FROM modules WHERE module_type = ?1 AND name = ?2 ORDER BY id ASC",
        )?;

        let rows = stmt
            .query_map(params![module_type, name], |row| {
                let version_str: String = row.get(0)?;
                let version = version_str.parse::<Version>().map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err))
                })?;
                let sha256: String = row.get(1)?;
                Ok(ModuleVersionInfo { version, sha256 })
            })?
            .collect::<Result<Vec<_>, _>>();

        rows.map_err(ModuleStoreError::Database)
    }

    pub fn get_all_module_names(
        &self,
        module_type: ModuleType,
    ) -> Result<Vec<String>, ModuleStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT name FROM modules WHERE module_type = ?1 ORDER BY name ASC",
        )?;

        let rows = stmt
            .query_map(params![module_type], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>();

        rows.map_err(ModuleStoreError::Database)
    }

    pub fn get_env_vars(
        &self,
        module_type: ModuleType,
        name: &str,
    ) -> Result<HashMap<String, String>, ModuleStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT key, value FROM module_env_vars WHERE module_type = ?1 AND name = ?2",
        )?;
        let rows = stmt
            .query_map(params![module_type, name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<HashMap<_, _>, _>>();
        rows.map_err(ModuleStoreError::Database)
    }

    pub fn set_env_var(
        &self,
        module_type: ModuleType,
        name: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ModuleStoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO module_env_vars (module_type, name, key, value) VALUES (?1, ?2, ?3, ?4)",
            params![module_type, name, key, value],
        )?;
        Ok(())
    }

    pub fn delete_env_var(
        &self,
        module_type: ModuleType,
        name: &str,
        key: &str,
    ) -> Result<bool, ModuleStoreError> {
        let rows_affected = self.conn.execute(
            "DELETE FROM module_env_vars WHERE module_type = ?1 AND name = ?2 AND key = ?3",
            params![module_type, name, key],
        )?;
        Ok(rows_affected > 0)
    }
}

fn is_valid_module_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl ToSql for ModuleType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(match self {
            ModuleType::Command => b"command",
            ModuleType::Policy => b"policy",
            ModuleType::Projector => b"projector",
            ModuleType::Effect => b"effect",
        })))
    }
}

impl FromSql for ModuleType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "command" => Ok(ModuleType::Command),
            "policy" => Ok(ModuleType::Policy),
            "projector" => Ok(ModuleType::Projector),
            "effect" => Ok(ModuleType::Effect),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[cfg(test)]
mod save_load_tests {
    use super::*;

    #[test]
    fn test_save_load_module() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = SqliteModuleStore::new(conn);
        store.init().unwrap();
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 69, 255],
            )
            .unwrap();
        let (bytes, _sha256) = store
            .load_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(bytes, vec![1, 2, 69, 255]);
    }

    #[test]
    fn test_load_module_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = SqliteModuleStore::new(conn);
        store.init().unwrap();
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 69, 255],
            )
            .unwrap();
        let bytes = store
            .load_module(ModuleType::Command, "hello", "0.1.3".parse().unwrap())
            .unwrap();
        assert_eq!(bytes, None);
    }

    #[test]
    fn test_load_module_integrity() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = SqliteModuleStore::new(conn);
        store.init().unwrap();
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 69, 255],
            )
            .unwrap();

        store
            .conn
            .execute("UPDATE modules SET wasm_bytes = ?1", [[1, 2, 68, 255]])
            .unwrap();

        let result = store.load_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap());
        assert!(matches!(result, Err(ModuleStoreError::Integrity(_))));
    }
}

#[cfg(test)]
mod active_tests {
    use super::*;

    #[test]
    fn test_activate_module_success() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save a module
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 69, 255],
            )
            .unwrap();

        // Activate it
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        // Check active_modules table
        let active_module_id: i64 = store
            .conn
            .query_row(
                "SELECT module_id FROM active_modules WHERE module_type = ?1 AND name = ?2",
                params![ModuleType::Command, "hello"],
                |row| row.get(0),
            )
            .unwrap();

        // Should match the saved module's id
        let saved_module_id: i64 = store
            .conn
            .query_row(
                "SELECT id FROM modules WHERE module_type = ?1 AND name = ?2 AND version = ?3",
                params![ModuleType::Command, "hello", "0.1.2"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(active_module_id, saved_module_id);
    }

    #[test]
    fn test_activate_module_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Attempt to activate a module that doesn't exist
        let result = store.activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap());

        assert!(matches!(
            result,
            Err(ModuleStoreError::ModuleNotFound { .. })
        ));
    }

    #[test]
    fn test_activate_module_switch_version() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save two versions of the same module
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 69, 255],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.2.0".parse().unwrap(),
                &[3, 4, 5, 6],
            )
            .unwrap();

        // Activate the first version
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        // Switch to the second version
        store
            .activate_module(ModuleType::Command, "hello", "0.2.0".parse().unwrap())
            .unwrap();

        // Check that active_modules points to the new version
        let active_module_id: i64 = store
            .conn
            .query_row(
                "SELECT module_id FROM active_modules WHERE module_type = ?1 AND name = ?2",
                params![ModuleType::Command, "hello"],
                |row| row.get(0),
            )
            .unwrap();

        let new_module_id: i64 = store
            .conn
            .query_row(
                "SELECT id FROM modules WHERE module_type = ?1 AND name = ?2 AND version = ?3",
                params![ModuleType::Command, "hello", "0.2.0"],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(active_module_id, new_module_id);
    }

    #[test]
    fn test_get_active_module() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        let active = store
            .get_active_module(ModuleType::Command, "hello")
            .unwrap()
            .unwrap();
        assert_eq!(active.0.to_string(), "0.1.2");
        assert_eq!(active.1, vec![1, 2, 3]);
    }

    #[test]
    fn test_deactivate_module() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        store
            .deactivate_module(ModuleType::Command, "hello")
            .unwrap();

        let active = store
            .get_active_module(ModuleType::Command, "hello")
            .unwrap();
        assert!(active.is_none());
    }
}

#[cfg(test)]
mod all_active_tests {
    use super::*;

    #[test]
    fn test_get_all_active_modules_empty() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        let active = store.get_all_active_modules(None).unwrap();
        assert!(active.is_empty());
    }

    #[test]
    fn test_get_all_active_modules_single() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        let active = store.get_all_active_modules(None).unwrap();
        assert_eq!(active.len(), 1);

        let module = &active[0];
        assert_eq!(module.module_type, ModuleType::Command);
        assert_eq!(module.name, "hello");
        assert_eq!(module.version.to_string(), "0.1.2");
        assert_eq!(module.wasm_bytes, [1, 2, 3]);
    }

    #[test]
    fn test_get_all_active_modules_multiple() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save multiple modules
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Command,
                "world",
                "0.2.0".parse().unwrap(),
                &[4, 5, 6],
            )
            .unwrap();

        // Activate them
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();
        store
            .activate_module(ModuleType::Command, "world", "0.2.0".parse().unwrap())
            .unwrap();

        let mut active = store.get_all_active_modules(None).unwrap();
        active.sort_by(|a, b| a.name.cmp(&b.name)); // sort by name for deterministic test

        assert_eq!(active.len(), 2);

        let module = &active[0];
        assert_eq!(module.module_type, ModuleType::Command);
        assert_eq!(module.name, "hello");
        assert_eq!(module.version.to_string(), "0.1.2");
        assert_eq!(module.wasm_bytes, [1, 2, 3]);

        let module = &active[1];
        assert_eq!(module.module_type, ModuleType::Command);
        assert_eq!(module.name, "world");
        assert_eq!(module.version.to_string(), "0.2.0");
        assert_eq!(module.wasm_bytes, [4, 5, 6]);
    }

    #[test]
    fn test_get_all_active_modules_switch_version() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save two versions
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.1.2".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Command,
                "hello",
                "0.2.0".parse().unwrap(),
                &[7, 8, 9],
            )
            .unwrap();

        // Activate first version
        store
            .activate_module(ModuleType::Command, "hello", "0.1.2".parse().unwrap())
            .unwrap();

        // Switch to second version
        store
            .activate_module(ModuleType::Command, "hello", "0.2.0".parse().unwrap())
            .unwrap();

        let active = store.get_all_active_modules(None).unwrap();
        assert_eq!(active.len(), 1);
        let module = &active[0];
        assert_eq!(module.version.to_string(), "0.2.0");
        assert_eq!(module.wasm_bytes, [7, 8, 9]);
    }

    #[test]
    fn test_get_all_active_modules_filter_by_type() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save and activate modules of different types
        store
            .save_module(
                ModuleType::Command,
                "cmd1",
                "0.1.0".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Command,
                "cmd2",
                "0.2.0".parse().unwrap(),
                &[4, 5, 6],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Projector,
                "proj1",
                "0.1.0".parse().unwrap(),
                &[7, 8, 9],
            )
            .unwrap();
        store
            .save_module(
                ModuleType::Effect,
                "effect1",
                "0.1.0".parse().unwrap(),
                &[10, 11, 12],
            )
            .unwrap();

        store
            .activate_module(ModuleType::Command, "cmd1", "0.1.0".parse().unwrap())
            .unwrap();
        store
            .activate_module(ModuleType::Command, "cmd2", "0.2.0".parse().unwrap())
            .unwrap();
        store
            .activate_module(ModuleType::Projector, "proj1", "0.1.0".parse().unwrap())
            .unwrap();
        store
            .activate_module(ModuleType::Effect, "effect1", "0.1.0".parse().unwrap())
            .unwrap();

        // Filter by Command type
        let commands = store
            .get_all_active_modules(Some(ModuleType::Command))
            .unwrap();
        assert_eq!(commands.len(), 2);
        assert!(
            commands
                .iter()
                .all(|m| m.module_type == ModuleType::Command)
        );

        // Filter by Projector type
        let projectors = store
            .get_all_active_modules(Some(ModuleType::Projector))
            .unwrap();
        assert_eq!(projectors.len(), 1);
        assert_eq!(projectors[0].module_type, ModuleType::Projector);
        assert_eq!(projectors[0].name, "proj1");

        // Filter by Effect type
        let effects = store
            .get_all_active_modules(Some(ModuleType::Effect))
            .unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].module_type, ModuleType::Effect);
        assert_eq!(effects[0].name, "effect1");
    }

    #[test]
    fn test_get_all_active_modules_filter_empty_type() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        // Save and activate only Command modules
        store
            .save_module(
                ModuleType::Command,
                "cmd1",
                "0.1.0".parse().unwrap(),
                &[1, 2, 3],
            )
            .unwrap();
        store
            .activate_module(ModuleType::Command, "cmd1", "0.1.0".parse().unwrap())
            .unwrap();

        // Filter by Projector type (should return empty)
        let projectors = store
            .get_all_active_modules(Some(ModuleType::Projector))
            .unwrap();
        assert!(projectors.is_empty());

        // Filter by Effect type (should return empty)
        let effects = store
            .get_all_active_modules(Some(ModuleType::Effect))
            .unwrap();
        assert!(effects.is_empty());
    }
}

#[cfg(test)]
mod module_versions_tests {
    use super::*;

    #[test]
    fn test_get_module_versions() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let store = SqliteModuleStore::new(conn);
        store.init().unwrap();

        store
            .save_module(ModuleType::Command, "hello", "0.1.0".parse().unwrap(), &[1])
            .unwrap();
        store
            .save_module(ModuleType::Command, "hello", "0.2.0".parse().unwrap(), &[2])
            .unwrap();
        store
            .save_module(ModuleType::Command, "hello", "0.3.0".parse().unwrap(), &[3])
            .unwrap();

        let versions = store
            .get_module_versions(ModuleType::Command, "hello")
            .unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version.to_string(), "0.1.0");
        assert_eq!(versions[1].version.to_string(), "0.2.0");
        assert_eq!(versions[2].version.to_string(), "0.3.0");
    }
}
