use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Ident, LitStr,
    parse::{Parse, ParseStream},
};

pub struct DeriveCommandInput {
    ident: Ident,
    domain_ids: HashMap<Ident, LitStr>,
}

impl DeriveCommandInput {
    pub fn expand(self) -> TokenStream {
        let Self { ident, domain_ids } = self;

        let domain_ids_inserts = domain_ids.into_iter().map(|(ident, domain_id)| {
            quote! {
                if let ::umari_core::domain_id::DomainIdValue::Value(domain_id) = ::std::convert::Into::into(::std::clone::Clone::clone(&self.#ident)) {
                    bindings
                        .entry(#domain_id)
                        .or_insert_with(::std::vec::Vec::new)
                        .push(domain_id);
                }
            }
        });

        quote! {
            #[automatically_derived]
            impl ::umari_core::command::CommandInput for #ident {
                fn domain_id_bindings(&self) -> ::umari_core::domain_id::DomainIdBindings {
                    let mut bindings: ::umari_core::domain_id::DomainIdBindings = ::std::collections::HashMap::new();
                    #( #domain_ids_inserts )*
                    bindings
                }
            }
        }
    }
}

impl Parse for DeriveCommandInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: DeriveInput = input.parse()?;

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

        Ok(DeriveCommandInput {
            ident: input.ident,
            domain_ids,
        })
    }
}
