#![allow(clippy::too_many_arguments)]

use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

pub use self::umari::command::types::*;

wit_bindgen::generate!({
    world: "command",
    path: "wit/command",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
    },
});

pub struct CommandExport<T>(PhantomData<T>);

pub trait ExportedCommand {
    type Input: DeserializeOwned + JsonSchema;

    fn execute(
        input: Self::Input,
        context: crate::command::CommandContext,
    ) -> anyhow::Result<ExecuteOutput>;
}

impl<T> Guest for CommandExport<T>
where
    T: ExportedCommand,
{
    fn schema() -> Option<Json> {
        let schema = schemars::schema_for!(T::Input);
        Some(serde_json::to_string(&schema).unwrap_or_else(|_| panic!("invalid json schema")))
    }

    fn execute(input: Json, context: CommandContext) -> Result<ExecuteOutput, Error> {
        let input: T::Input =
            serde_json::from_str(&input).map_err(|err| Error::InvalidInput(err.to_string()))?;

        let context = crate::command::CommandContext {
            correlation_id: context
                .correlation_id
                .as_deref()
                .map(|id| uuid::Uuid::parse_str(id).unwrap()),
            triggering_event_id: context
                .triggering_event_id
                .as_deref()
                .map(|id| uuid::Uuid::parse_str(id).unwrap()),
            idempotency_key: context
                .idempotency_key
                .as_deref()
                .map(|id| uuid::Uuid::parse_str(id).unwrap()),
        };

        T::execute(input, context).map_err(|err| Error::Rejected(err.to_string()))
    }
}

impl From<crate::command::ExecuteOutput> for ExecuteOutput {
    fn from(output: crate::command::ExecuteOutput) -> Self {
        ExecuteOutput {
            position: output.position,
            events: output
                .events
                .into_iter()
                .map(|event| EmittedEvent {
                    id: event.id.to_string(),
                    event_type: event.event_type,
                    domain_ids: event
                        .domain_ids
                        .into_iter()
                        .map(|(name, id)| DomainId { name, id })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl From<crate::command::CommandContext> for CommandContext {
    fn from(ctx: crate::command::CommandContext) -> Self {
        CommandContext {
            correlation_id: ctx.correlation_id.as_ref().map(ToString::to_string),
            triggering_event_id: ctx.triggering_event_id.as_ref().map(ToString::to_string),
            idempotency_key: ctx.idempotency_key.as_ref().map(ToString::to_string),
        }
    }
}
