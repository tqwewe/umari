//! # ESRuntime SDK
//!
//! SDK for building event-sourced command handlers as WASM modules.
//!
//! ## Overview
//!
//! This crate provides the traits and types needed to write command handlers
//! that run in the ESRuntime. Command handlers:
//!
//! 1. Declare which events they need to read (via `EventSet`)
//! 2. Declare which domain IDs to query (via `CommandInput`)
//! 3. Rebuild state from historical events (via `apply`)
//! 4. Make decisions and emit new events (via `handle`)
//!
//! ## Example
//!
//! ```rust,ignore
//! use rive_core::prelude::*;
//! use serde::Deserialize;
//! use my_schema::{OpenedAccount, SentFunds};
//!
//! #[derive(EventSet)]
//! enum Query {
//!     OpenedAccount(OpenedAccount),
//!     SentFunds(SentFunds),
//! }
//!
//! #[derive(CommandInput, Deserialize)]
//! struct Input {
//!     #[domain_id("account_id")]
//!     account_id: String,
//!     amount: f64,
//! }
//!
//! #[derive(Default)]
//! struct Withdraw {
//!     balance: f64,
//! }
//!
//! impl Command for Withdraw {
//!     type Query = Query;
//!     type Input = Input;
//!
//!     fn apply(&mut self, event: Query) {
//!         match event {
//!             Query::OpenedAccount(ev) => self.balance = ev.initial_balance,
//!             Query::SentFunds(ev) => self.balance -= ev.amount,
//!         }
//!     }
//!
//!     fn handle(self, input: Input) -> Result<Emit, CommandError> {
//!         if self.balance < input.amount {
//!             return Err(CommandError::rejected("Insufficient funds"));
//!         }
//!         
//!         Ok(Emit::new().event(SentFunds {
//!             account_id: input.account_id,
//!             amount: input.amount,
//!             recipient_id: None,
//!         }))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = UmaDBClient::new("http://0.0.0.0:50051".to_string())
//!         .connect_async()
//!         .await?,
//!
//!     Withdraw::execute(&client, Input {
//!         account_id: "bob".to_string(),
//!         amount: 14.50,
//!     }).await?;
//!
//!     Ok(())
//! }
//! ```

pub use umari_macros::{CommandInput, Event, EventSet, export_command};

pub mod command;
pub mod domain_id;
pub mod emit;
pub mod error;
pub mod event;
#[macro_use]
mod macros;
pub mod projection;
pub mod runtime;

pub mod prelude {
    pub use crate::command::*;
    pub use crate::domain_id::*;
    pub use crate::emit;
    pub use crate::emit::*;
    pub use crate::error::*;
    pub use crate::event::*;
    pub use crate::projection::*;
    pub use umari_macros::{CommandInput, Event, EventSet, export_command};
}

#[doc(hidden)]
pub mod __private {
    pub use serde_json;
    pub use umadb_dcb;
}
