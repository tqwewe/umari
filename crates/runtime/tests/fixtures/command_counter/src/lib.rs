use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use umari::prelude::*;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("Incremented")]
pub struct Incremented {
    #[domain_id]
    pub counter_id: String,
    pub amount: i64,
}

#[derive(DomainIds, JsonSchema, Deserialize)]
pub struct Input {
    #[domain_id]
    pub counter_id: String,
    pub amount: i64,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context).execute(|input| {
        Ok(emit![Incremented {
            counter_id: input.counter_id,
            amount: input.amount,
        }])
    })
}
