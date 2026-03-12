use events::OpenedAccount;
use serde::Deserialize;
use umari_core::prelude::*;

// Export this command as a WASM component
export_command!(OpenAccountState);

/// Events this command reads
#[derive(EventSet)]
enum Query {
    OpenedAccount(OpenedAccount),
}

/// Command payload with domain ID bindings
#[derive(CommandInput, Deserialize)]
struct Input {
    #[domain_id]
    account_id: String,
    initial_balance: f64,
}

/// Handler State
#[derive(Default)]
struct OpenAccountState {
    is_open: bool,
}

/// Implementation
impl Command for OpenAccountState {
    type Query = Query;
    type Input = Input;
    type Error = CommandError;

    fn apply(&mut self, event: Query, _meta: EventMeta) {
        match event {
            Query::OpenedAccount(OpenedAccount { .. }) => {
                self.is_open = true;
            }
        }
    }

    fn handle(&self, input: Input) -> Result<Emit, CommandError> {
        if self.is_open {
            return Err(CommandError::rejected("account already open"));
        }

        Ok(emit![OpenedAccount {
            account_id: input.account_id.clone(),
            initial_balance: input.initial_balance,
        }])
    }
}
