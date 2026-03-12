mod derive_command_input;
mod derive_event;
mod derive_event_set;

use proc_macro::TokenStream;
use syn::parse_macro_input;

use crate::derive_command_input::DeriveCommandInput;
use crate::derive_event::DeriveEvent;
use crate::derive_event_set::DeriveEventSet;

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
