mod derive_command_input;
mod derive_event;
mod derive_event_set;
mod export_command;

use proc_macro::TokenStream;
use syn::parse_macro_input;

use crate::derive_command_input::DeriveCommandInput;
use crate::derive_event::DeriveEvent;
use crate::derive_event_set::DeriveEventSet;
use crate::export_command::ExportCommand;

#[proc_macro_derive(CommandInput, attributes(event_type, domain_id))]
pub fn command(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveCommandInput);
    TokenStream::from(input.expand())
}

#[proc_macro_derive(Event, attributes(event_type, domain_id))]
pub fn event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveEvent);
    TokenStream::from(input.expand())
}

#[proc_macro_derive(EventSet, attributes(scope))]
pub fn event_set(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveEventSet);
    TokenStream::from(input.expand())
}

/// Export a command as a WASM component.
///
/// This macro generates all the boilerplate needed to export a command type
/// as a WASM component using WIT bindings.
///
/// # Example
///
/// ```rust,ignore
/// use rivo_core::prelude::*;
///
/// rivo_core::export_command!(OpenAccount);
///
/// // Your clean command implementation
/// #[derive(Default)]
/// pub struct OpenAccount {
///     is_open: bool,
/// }
///
/// impl Command for OpenAccount {
///     // ...
/// }
/// ```
#[proc_macro]
pub fn export_command(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ExportCommand);
    TokenStream::from(input.expand())
}
