use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, LitStr, parse::Parse};

pub struct ExportCommand {
    command_type: Ident,
    wit_path: Option<LitStr>,
}

impl Parse for ExportCommand {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let command_type: Ident = input.parse()?;

        let wit_path = if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        Ok(ExportCommand {
            command_type,
            wit_path,
        })
    }
}

impl ExportCommand {
    pub fn expand(self) -> TokenStream {
        let command_type = &self.command_type;
        let wit_path = self
            .wit_path
            .as_ref()
            .map(|p| p.value())
            .unwrap_or_else(|| "../../../wit".to_string());

        quote! {
            mod __wit {
                // Generate WIT bindings inline
                ::wit_bindgen::generate!({
                    world: "command",
                    path: #wit_path,
                });

                // Export the component implementation
                struct Component;

                impl Guest for Component {
                    fn query(
                        input: String,
                    ) -> Result<String, rivo::command::types::CommandError> {
                        ::rivo_core::runtime::query_input::<super::#command_type>(input)
                            .and_then(|dcb_query| {
                                ::serde_json::to_string(&dcb_query).map_err(|e| ::rivo_core::runtime::ErrorOutput {
                                    code: ::rivo_core::runtime::ErrorCode::InputDeserialization,
                                    message: format!("failed to serialize DCBQuery: {}", e),
                                })
                            })
                            .map_err(|e| rivo::command::types::CommandError {
                                code: match e.code {
                                    ::rivo_core::runtime::ErrorCode::ValidationError => {
                                        rivo::command::types::ErrorCode::ValidationError
                                    }
                                    ::rivo_core::runtime::ErrorCode::InputDeserialization => {
                                        rivo::command::types::ErrorCode::DeserializationError
                                    }
                                    _ => rivo::command::types::ErrorCode::CommandError,
                                },
                                message: e.message,
                            })
                    }

                    fn execute(
                        input: rivo::command::types::ExecuteInput,
                    ) -> Result<rivo::command::types::ExecuteOutput, rivo::command::types::CommandError> {
                        let core_input = ::rivo_core::runtime::ExecuteInput {
                            input: input.input,
                            events: input
                                .events
                                .into_iter()
                                .map(|e| ::rivo_core::runtime::EventData {
                                    event_type: e.event_type,
                                    data: e.data,
                                    timestamp: e.timestamp,
                                })
                                .collect(),
                        };

                        ::rivo_core::runtime::execute_with_events::<super::#command_type>(core_input)
                            .map(|output| rivo::command::types::ExecuteOutput {
                                events: output
                                    .events
                                    .into_iter()
                                    .map(|e| rivo::command::types::EmittedEvent {
                                        event_type: e.event_type,
                                        data: e.data,
                                        domain_ids: e.domain_ids
                                            .into_iter()
                                            .map(|(k, v)| {
                                                let wit_value = match v {
                                                    ::rivo_core::domain_id::DomainIdValue::Value(s) => {
                                                        rivo::command::types::DomainIdValue::Value(s)
                                                    }
                                                    ::rivo_core::domain_id::DomainIdValue::None => {
                                                        rivo::command::types::DomainIdValue::None
                                                    }
                                                };
                                                (k, wit_value)
                                            })
                                            .collect(),
                                    })
                                    .collect(),
                            })
                            .map_err(|e| rivo::command::types::CommandError {
                                code: match e.code {
                                    ::rivo_core::runtime::ErrorCode::EventDeserialization => {
                                        rivo::command::types::ErrorCode::DeserializationError
                                    }
                                    ::rivo_core::runtime::ErrorCode::CommandError => {
                                        rivo::command::types::ErrorCode::CommandError
                                    }
                                    ::rivo_core::runtime::ErrorCode::ValidationError => {
                                        rivo::command::types::ErrorCode::ValidationError
                                    }
                                    ::rivo_core::runtime::ErrorCode::InputDeserialization => {
                                        rivo::command::types::ErrorCode::DeserializationError
                                    }
                                },
                                message: e.message,
                            })
                    }
                }

                export!(Component);
            }
        }
    }
}
