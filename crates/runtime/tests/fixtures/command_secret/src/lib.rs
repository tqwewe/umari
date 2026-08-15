use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use umari::prelude::*;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("SecretStored")]
pub struct SecretStored {
    #[domain_id]
    #[crypto_scope]
    pub user_id: String,
    pub secret: String,
}

#[derive(DomainIds, JsonSchema, Deserialize)]
pub struct Input {
    #[domain_id]
    pub user_id: String,
    pub secret: String,
}

#[export_command]
pub fn execute(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context).execute(|input| {
        Ok(emit![SecretStored {
            user_id: input.user_id,
            secret: input.secret,
        }])
    })
}
