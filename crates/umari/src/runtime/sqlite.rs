pub use self::umari::sqlite::connection::{
    execute, execute_batch, last_insert_rowid, query_one, query_row,
};
pub use self::umari::sqlite::statement::Stmt;

pub use crate::error::{ConstraintViolation, ConstraintViolationKind, SqliteError};
pub use crate::sqlite::{Column, Row, SqliteValue};

wit_bindgen::generate!({
    path: "wit/sqlite",
    world: "sqlite",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
    with: {
        "umari:sqlite/types@0.1.0/value": crate::sqlite::SqliteValue,
        "umari:sqlite/types@0.1.0/column": crate::sqlite::Column,
        "umari:sqlite/types@0.1.0/row": crate::sqlite::Row,
        "umari:sqlite/types@0.1.0/sqlite-error": crate::error::SqliteError,
        "umari:sqlite/types@0.1.0/constraint-violation": crate::error::ConstraintViolation,
        "umari:sqlite/types@0.1.0/constraint-violation-kind": crate::error::ConstraintViolationKind,
    },
});
