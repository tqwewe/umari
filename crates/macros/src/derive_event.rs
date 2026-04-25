use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Ident, LitStr,
    parse::{Parse, ParseStream},
};

pub struct DeriveEvent {
    ident: Ident,
    event_type: LitStr,
}

impl DeriveEvent {
    pub fn expand(self) -> TokenStream {
        let Self { ident, event_type } = self;

        quote! {
            #[automatically_derived]
            impl ::umari::event::Event for #ident {
                const EVENT_TYPE: &'static str = #event_type;
            }

            #[automatically_derived]
            impl ::umari::event::AsEvent<#ident> for #ident {
                #[inline]
                fn as_event(&self) -> ::std::option::Option<&#ident> {
                    Some(self)
                }
            }

            #[automatically_derived]
            impl ::umari::event::IntoEvent<#ident> for #ident {
                #[inline]
                fn into_event(self) -> ::std::option::Option<#ident> {
                    Some(self)
                }
            }
        }
    }
}

impl Parse for DeriveEvent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: DeriveInput = input.parse()?;

        let event_type = input
            .attrs
            .iter()
            .find_map(|attr| {
                if attr.path().is_ident("event_type") {
                    Some(attr.parse_args())
                } else {
                    None
                }
            })
            .transpose()?
            .unwrap_or_else(|| LitStr::new(&input.ident.to_string(), input.ident.span()));

        Ok(DeriveEvent {
            ident: input.ident,
            event_type,
        })
    }
}
