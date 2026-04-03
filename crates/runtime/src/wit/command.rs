use kameo::error::SendError;
use umari_core::prelude::CommandContext;
use uuid::Uuid;
use wasmtime::{component::bindgen, error::Context};

pub use self::umari::command::{types::*, *};

use crate::{
    command::actor::{CommandPayload, Execute},
    wit,
};

bindgen!({
    path: "../../wit/command",
    world: "command",
    imports: {
        "umari:command/executor.execute": async | trappable,
        default: tracing | trappable
    },
    exports: { default: async },
    with: {
        "umari:common": crate::wit::common,
    }
});

impl Host for wit::CommandComponentState {}

impl Host for wit::EventHandlerComponentState {}

impl executor::Host for wit::CommandComponentState {
    async fn execute(
        &mut self,
        _command: String,
        _input: String,
        _context: executor::CommandContext,
    ) -> wasmtime::Result<Result<Vec<StoredEvent>, String>> {
        panic!("executor not available in commands")
    }
}

impl executor::Host for wit::EventHandlerComponentState {
    async fn execute(
        &mut self,
        command: String,
        input: String,
        context: executor::CommandContext,
    ) -> wasmtime::Result<Result<Vec<StoredEvent>, String>> {
        let input = serde_json::from_str(&input).context("invalid json input")?; // trap
        let context = context.try_into()?; // trap
        let msg = Execute {
            name: command.into(),
            command: CommandPayload { input, context },
        };

        let result = self.command_ref.ask(msg).await;
        match result {
            Ok(_) => todo!(),
            Err(SendError::HandlerError(err)) => Ok(Err(err.to_string())),
            Err(err) => Err(wasmtime::Error::msg(err.to_string())),
        }
    }
}

impl TryFrom<executor::CommandContext> for CommandContext {
    type Error = wasmtime::Error;

    fn try_from(ctx: executor::CommandContext) -> Result<Self, Self::Error> {
        Ok(CommandContext {
            correlation_id: ctx
                .correlation_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .context("invalid correlation id")?
                .unwrap_or_else(Uuid::new_v4),
            triggering_event_id: ctx
                .triggering_event_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .context("invalid causation id")?,
        })
    }
}
