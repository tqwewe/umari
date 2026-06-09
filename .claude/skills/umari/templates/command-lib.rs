// Verbatim output of `umari new command <name>` (Rust mode).
// Source: crates/cli/src/commands/new.rs (`lib_rs_content` for "command").
// Copy this as src/lib.rs in a new command crate, then fill in the TODOs.

use schemars::JsonSchema;
use serde::Deserialize;
use umari::prelude::*;

#[derive(DomainIds, JsonSchema, Deserialize)]
pub struct Input {
    // TODO: add input fields; use #[domain_id] to tag domain ID fields
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context).execute(|input| {
        // TODO: implement execute
        Ok(emit![])
    })
}
