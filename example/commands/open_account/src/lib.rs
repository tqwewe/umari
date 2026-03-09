use rivo_core::prelude::*;
use serde::Deserialize;

use events::OpenedAccount;

#[unsafe(no_mangle)]
pub extern "C" fn query(ptr: i32, len: i32) -> i64 {
    rivo_core::runtime::query_command::<OpenAccount>(ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn execute(ptr: i32, len: i32) -> i64 {
    rivo_core::runtime::execute_command::<OpenAccount>(ptr, len)
}

/// Events this command reads
#[derive(EventSet)]
pub enum Query {
    OpenedAccount(OpenedAccount),
}

/// Command payload with domain ID bindings
#[derive(CommandInput, Deserialize)]
pub struct OpenAccountInput {
    #[domain_id]
    pub account_id: String,
    pub initial_balance: f64,
}

/// Handler State
#[derive(Default)]
pub struct OpenAccount {
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
