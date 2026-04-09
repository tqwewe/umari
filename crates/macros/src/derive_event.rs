use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Ident, LitStr,
    parse::{Parse, ParseStream},
};

pub struct DeriveEvent {
    ident: Ident,
    event_type: LitStr,
    domain_ids: HashMap<Ident, LitStr>,
}

impl DeriveEvent {
    pub fn expand(self) -> TokenStream {
        let Self {
            ident,
            event_type,
            domain_ids,
        } = self;

        let domain_id_fields = domain_ids.values();
        let domain_ids_inserts = domain_ids.iter().map(|(ident, domain_id)| {
            quote! {
                ids.insert(#domain_id, ::umari_core::domain_id::DomainIdValue::from(::std::clone::Clone::clone(&self.#ident)));
            }
        });

        quote! {
            #[automatically_derived]
            impl ::umari_core::event::Event for #ident {
                const EVENT_TYPE: &'static str = #event_type;
                const DOMAIN_ID_FIELDS: &'static [&'static str] = &[#( #domain_id_fields ,)*];

                fn domain_ids(&self) -> ::umari_core::domain_id::DomainIdValues {
                    let mut ids = ::std::collections::HashMap::new();
                    #( #domain_ids_inserts )*
                    ids
                }
            }

            #[automatically_derived]
            impl ::umari_core::event::AsEvent<#ident> for #ident {
                #[inline]
                fn as_event(&self) -> ::std::option::Option<&#ident> {
                    Some(self)
                }
            }

            #[automatically_derived]
            impl ::umari_core::event::IntoEvent<#ident> for #ident {
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

        let domain_ids = match input.data {
            syn::Data::Struct(data) => data
                .fields
                .into_iter()
                .filter_map(|field| {
                    let attr = field
                        .attrs
                        .into_iter()
                        .find(|attr| attr.path().is_ident("domain_id"))?;

                    match attr.meta {
                        syn::Meta::Path(_) => {
                            let ident = field.ident?;
                            let domain_id = LitStr::new(&ident.to_string(), ident.span());
                            Some(Ok((ident, domain_id)))
                        }
                        syn::Meta::List(list) => {
                            let ident = field.ident?;
                            match list.parse_args() {
                                Ok(domain_id) => Some(Ok((ident, domain_id))),
                                Err(err) => Some(Err(err)),
                            }
                        }
                        syn::Meta::NameValue(_) => None,
                    }
                })
                .collect::<Result<_, _>>()?,
            _ => HashMap::new(),
        };

        Ok(DeriveEvent {
            ident: input.ident,
            event_type,
            domain_ids,
        })
    }
}
