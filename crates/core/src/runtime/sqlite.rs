
pub use self::umari::sqlite::connection::{
    execute, execute_batch, last_insert_rowid, query_one, query_row,
};
pub use self::umari::sqlite::statement::Stmt;
pub use self::umari::sqlite::types::*;

wit_bindgen::generate!({
    path: "../../wit/sqlite",
    world: "sqlite",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
});
