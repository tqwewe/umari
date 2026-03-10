use umari_core::prelude::*;
use serde::Deserialize;

use events::OpenedAccount;

// Export this command as a WASM component
export_command!(OpenAccount);

/// Events this command reads
#[derive(EventSet)]
enum Query {
    OpenedAccount(OpenedAccount),
}

/// Command payload with domain ID bindings
#[derive(CommandInput, Deserialize)]
struct OpenAccountInput {
    #[domain_id]
    account_id: String,
    initial_balance: f64,
}

/// Handler State
#[derive(Default)]
struct OpenAccount {
    is_open: bool,
}

/// Implementation
impl Command for OpenAccount {
    type Query = Query;
    type Input = OpenAccountInput;
    type Error = CommandError;

    fn apply(&mut self, event: Query, _meta: EventMeta) {
        match event {
            Query::OpenedAccount(OpenedAccount { .. }) => {
                self.is_open = true;
            }
        }
    }

    fn handle(&self, input: OpenAccountInput) -> Result<Emit, CommandError> {
        if self.is_open {
            return Err(CommandError::rejected("account already open"));
        }

        Ok(emit![OpenedAccount {
            account_id: input.account_id.clone(),
            initial_balance: input.initial_balance,
        }])
    }
}
