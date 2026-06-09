// Verbatim output of `umari new effect <name>` (Rust mode).
// Source: crates/cli/src/commands/new.rs (`lib_rs_content` for "effect").
// `{type_name}` is the kebab-case → PascalCase form of the crate name.
// Copy this as src/lib.rs in a new effect crate and fill in the TODOs.

use umari::prelude::*;

export_effect!({type_name});

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct {type_name} {}

impl Effect for {type_name} {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        Ok({type_name} {})
    }

    fn partition_key(&self, _event: StoredEvent<Query>) -> Option<String> {
        None
    }

    fn handle(&mut self, event: StoredEvent<Query>) -> anyhow::Result<()> {
        Ok(())
    }
}
