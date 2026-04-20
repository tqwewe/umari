use kameo::error::SendError;
use umari_core::command::CommandContext;
use uuid::Uuid;
use wasmtime::{component::bindgen, error::Context};

pub use self::umari::command::{types::*, *};

use crate::{
    command::actor::{CommandPayload, Execute},
    wit,
};

bindgen!({
    path: "../umari/wit/command",
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
    ) -> wasmtime::Result<()> {
        panic!("executor not available in commands")
    }
}

impl executor::Host for wit::EventHandlerComponentState {
    async fn execute(
        &mut self,
        command: String,
        input: String,
        context: executor::CommandContext,
    ) -> wasmtime::Result<()> {
        let mut context: CommandContext = context.try_into()?; // trap
        context
            .correlation_id
            .get_or_insert(self.current_correlation_id);
        context
            .triggering_event_id
            .get_or_insert(self.current_event_id);
        let msg = Execute {
            name: command.into(),
            command: CommandPayload { input, context },
        };

        let result = self.command_ref.ask(msg).await;
        match result {
            Ok(_) => Ok(()),
            Err(SendError::HandlerError(err)) => {
                Err(wasmtime::Error::msg(format!("command rejected: {err}")))
            }
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
                .context("invalid correlation id")?,
            triggering_event_id: ctx
                .triggering_event_id
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .context("invalid causation id")?,
            idempotency_key: ctx
                .idempotency_key
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .context("invalid indempotency key")?,
        })
    }
}
