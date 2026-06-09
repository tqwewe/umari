// Verbatim output of `umari new projector <name>` (Rust mode).
// Source: crates/cli/src/commands/new.rs (`lib_rs_content` for "projector").
// `{type_name}` is the kebab-case → PascalCase form of the crate name
// (e.g. `plans` → `Plans`, `warranty-plans` → `WarrantyPlans`).
// Copy this as src/lib.rs in a new projector crate and fill in the TODOs.

use umari::prelude::*;

export_projector!({type_name});

#[derive(EventSet)]
enum Query {
    // TODO: add event variants, e.g.: MyEvent(MyEvent),
}

struct {type_name} {}

impl Projector for {type_name} {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        // TODO: run CREATE TABLE IF NOT EXISTS statements here
        Ok({type_name} {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {}
    }
}
