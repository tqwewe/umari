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
        let inline_wit = include_str!("../../../wit/command/command.wit");

        quote! {
            mod __wit {
                // Generate WIT bindings inline
                ::wit_bindgen::generate!({
                    inline: #inline_wit,
                });

                // Export the component implementation
                struct Component;

                impl Guest for Component {
                    fn query(
                        input: String,
                    ) -> Result<String, umari::command::types::CommandError> {
                        ::umari_core::runtime::query_input::<super::#command_type>(input)
                            .and_then(|dcb_query| {
                                ::umari_core::__private::serde_json::to_string(&dcb_query).map_err(|e| ::umari_core::runtime::ErrorOutput {
                                    code: ::umari_core::runtime::ErrorCode::InputDeserialization,
                                    message: format!("failed to serialize DCBQuery: {}", e),
                                })
                            })
                            .map_err(|e| umari::command::types::CommandError {
                                code: match e.code {
                                    ::umari_core::runtime::ErrorCode::ValidationError => {
                                        umari::command::types::ErrorCode::ValidationError
                                    }
                                    ::umari_core::runtime::ErrorCode::InputDeserialization => {
                                        umari::command::types::ErrorCode::DeserializationError
                                    }
                                    _ => umari::command::types::ErrorCode::CommandError,
                                },
                                message: e.message,
                            })
                    }

                    fn execute(
                        input: umari::command::types::ExecuteInput,
                    ) -> Result<umari::command::types::ExecuteOutput, umari::command::types::CommandError> {
                        let core_input = ::umari_core::runtime::ExecuteInput {
                            input: input.input,
                            events: input
                                .events
                                .into_iter()
                                .map(|e| ::umari_core::runtime::EventData {
                                    event_type: e.event_type,
                                    data: e.data,
                                    timestamp: e.timestamp,
                                })
                                .collect(),
                        };

                        ::umari_core::runtime::execute_with_events::<super::#command_type>(core_input)
                            .map(|output| umari::command::types::ExecuteOutput {
                                events: output
                                    .events
                                    .into_iter()
                                    .map(|e| umari::command::types::EmittedEvent {
                                        event_type: e.event_type,
                                        data: e.data,
                                        domain_ids: e.domain_ids
                                            .into_iter()
                                            .map(|(k, v)| {
                                                let wit_value = match v {
                                                    ::umari_core::domain_id::DomainIdValue::Value(s) => {
                                                        umari::command::types::DomainIdValue::Value(s)
                                                    }
                                                    ::umari_core::domain_id::DomainIdValue::None => {
                                                        umari::command::types::DomainIdValue::None
                                                    }
                                                };
                                                (k, wit_value)
                                            })
                                            .collect(),
                                    })
                                    .collect(),
                            })
                            .map_err(|e| umari::command::types::CommandError {
                                code: match e.code {
                                    ::umari_core::runtime::ErrorCode::EventDeserialization => {
                                        umari::command::types::ErrorCode::DeserializationError
                                    }
                                    ::umari_core::runtime::ErrorCode::CommandError => {
                                        umari::command::types::ErrorCode::CommandError
                                    }
                                    ::umari_core::runtime::ErrorCode::ValidationError => {
                                        umari::command::types::ErrorCode::ValidationError
                                    }
                                    ::umari_core::runtime::ErrorCode::InputDeserialization => {
                                        umari::command::types::ErrorCode::DeserializationError
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
