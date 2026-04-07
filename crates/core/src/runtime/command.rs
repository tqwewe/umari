use std::marker::PhantomData;

use garde::Validate;
use serde::de::DeserializeOwned;
use umadb_dcb::DcbQuery;
use uuid::Uuid;

use crate::command::{
    Command, CommandInput, EventMeta, FoldSet, RuleSet, RuleSetRunner,
    build_query_items_from_domain_ids,
};

pub use self::umari::command::types::*;

wit_bindgen::generate!({
    world: "command",
    path: "../../wit/command",
    additional_derives: [PartialEq, Clone, serde::Serialize, serde::Deserialize],
    generate_unused_types: true,
    pub_export_macro: true,
    with: {
        "umari:common/types@0.1.0": crate::runtime::common,
    },
});

#[macro_export]
macro_rules! export_command {
    ($ty:path) => {
        impl $crate::command::CommandName for $ty {
            const COMMAND_NAME: &'static str = env!("CARGO_PKG_NAME");
        }

        $crate::runtime::command::export!($crate::runtime::command::CommandExport<$ty>, with_types_in $crate::runtime::command);
    };
}

pub struct CommandExport<T>(PhantomData<T>);

impl<T: Command> Guest for CommandExport<T>
where
    T: Command,
    T::Input: DeserializeOwned,
    <<T as Command>::Input as Validate>::Context: Default,
{
    fn schema() -> Option<Json> {
        let schema = schemars::schema_for!(T::Input);
        Some(serde_json::to_string(&schema).unwrap_or_else(|_| panic!("invalid json schema")))
    }

    fn query(input: Json) -> Result<EventQuery, Error> {
        let input: T::Input =
            serde_json::from_str(&input).map_err(|err| Error::InvalidInput(err.to_string()))?;

        input
            .validate()
            .map_err(|err| Error::Rejected(err.to_string()))?;

        let runner = T::rules(&input).into_runner();
        let mut event_domain_ids = <T::State as FoldSet>::event_domain_ids();
        event_domain_ids.extend(runner.event_domain_ids());
        let items = build_query_items_from_domain_ids(
            &event_domain_ids,
            &<T::Input as CommandInput>::domain_id_bindings(&input),
        );

        Ok(DcbQuery::with_items(items).into())
    }

    fn execute(input: Json, events: Vec<StoredEvent>) -> Result<ExecuteOutput, Error> {
        let input: T::Input =
            serde_json::from_str(&input).map_err(|err| Error::InvalidInput(err.to_string()))?;

        let bindings = <T::Input as CommandInput>::domain_id_bindings(&input);
        let mut state = T::State::default();
        let mut runner = T::rules(&input).into_runner();

        for stored_event in events {
            let event: crate::event::StoredEvent<serde_json::Value> = stored_event.into();

            let meta = EventMeta {
                timestamp: event.timestamp,
            };

            <T::State as FoldSet>::apply(
                &mut state,
                &event.event_type,
                event.data.clone(),
                &event.tags,
                &bindings,
                meta,
            )
            .unwrap_or_else(|err| panic!("failed to deserialize event data: {}", err.message));
            runner
                .apply_event(&event.event_type, event.data, &event.tags, &bindings, meta)
                .unwrap_or_else(|err| panic!("failed to deserialize event data: {}", err.message));
        }

        runner
            .check()
            .map_err(|err| Error::Rejected(err.to_string()))?;
        let emitted_events = T::emit(state, input)
            .into_events()
            .into_iter()
            .map(|event| {
                let data = serde_json::to_string(&event.data)
                    .unwrap_or_else(|err| panic!("failed to serialize event data: {err}"));
                EmittedEvent {
                    event_type: event.event_type,
                    data,
                    domain_ids: event
                        .domain_ids
                        .into_iter()
                        .map(|(k, v)| DomainId {
                            name: k.to_string(),
                            id: v.into_option(),
                        })
                        .collect(),
                }
            })
            .collect();

        Ok(ExecuteOutput {
            events: emitted_events,
        })
    }
}

impl From<umari::command::executor::CommandReceipt> for crate::command::CommandReceipt {
    fn from(receipt: umari::command::executor::CommandReceipt) -> Self {
        crate::command::CommandReceipt {
            position: receipt.position,
            events: receipt
                .events
                .into_iter()
                .map(|event| event.into())
                .collect(),
        }
    }
}

impl From<umari::command::executor::EmittedEvent> for crate::command::EmittedEventRef {
    fn from(event: umari::command::executor::EmittedEvent) -> Self {
        crate::command::EmittedEventRef {
            id: Uuid::parse_str(&event.id).expect("invalid event id"),
            event_type: event.event_type,
            tags: event.tags,
        }
    }
}
