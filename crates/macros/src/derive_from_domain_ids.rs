use std::cmp::Ordering;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Generics, Ident,
    parse::{Parse, ParseStream},
};

use crate::{DomainIdAttrs, NonDomainIdField};

pub struct DeriveFromDomainIds {
    ident: Ident,
    generics: Generics,
    is_unnamed: bool,
    attrs: DomainIdAttrs,
}

impl DeriveFromDomainIds {
    pub fn expand(self) -> TokenStream {
        let Self {
            ident,
            generics,
            is_unnamed,
            mut attrs,
        } = self;

        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        attrs.non_domain_ids.sort_by(|(_, a), (_, b)| match (a, b) {
            (NonDomainIdField::Explicit(_), NonDomainIdField::Explicit(_)) => Ordering::Equal,
            (NonDomainIdField::Explicit(_), NonDomainIdField::Default) => Ordering::Less,
            (NonDomainIdField::Default, NonDomainIdField::Explicit(_)) => Ordering::Greater,
            (NonDomainIdField::Default, NonDomainIdField::Default) => Ordering::Equal,
        });
        let args = attrs
            .non_domain_ids
            .iter()
            .filter_map(|(_, field)| match field {
                NonDomainIdField::Explicit(ty) => Some(ty),
                NonDomainIdField::Default => None,
            });
        let arg_fields = attrs
            .non_domain_ids
            .iter()
            .enumerate()
            .map(|(i, (ident, field))| {
                let i = syn::Index::from(i);
                if is_unnamed {
                    match field {
                        NonDomainIdField::Explicit(_) => quote! { args.#i },
                        NonDomainIdField::Default => {
                            quote! { ::std::default::Default::default() }
                        }
                    }
                } else {
                    match field {
                        NonDomainIdField::Explicit(_) => quote! { #ident: args.#i },
                        NonDomainIdField::Default => {
                            quote! { #ident: ::std::default::Default::default() }
                        }
                    }
                }
            });

        let binding_fields = attrs.domain_ids.iter().map(|(ident, domain_id)| {
            if is_unnamed {
                quote! {
                    ::std::str::FromStr::from_str(
                            bindings
                                .get(#domain_id)
                                .ok_or(
                                    ::umari::error::FromDomainIdsError::MissingDomainId(#domain_id)
                                )?
                        )
                        .map_err(|err| ::umari::error::FromDomainIdsError::ParseDomainId(::std::convert::Into::into(err)))?
                }
            } else {
                quote! {
                    #ident:
                        ::std::str::FromStr::from_str(
                                bindings
                                    .get(#domain_id)
                                    .ok_or(
                                        ::umari::error::FromDomainIdsError::MissingDomainId(#domain_id)
                                    )?
                            )
                            .map_err(|err| ::umari::error::FromDomainIdsError::ParseDomainId(::std::convert::Into::into(err)))?
                }
            }
        });

        let body = if is_unnamed {
            quote! {
                (
                    #( #arg_fields, )*
                    #( #binding_fields, )*
                )
            }
        } else {
            quote! {
                {
                    #( #arg_fields, )*
                    #( #binding_fields, )*
                }
            }
        };

        quote! {
            #[automatically_derived]
            impl #impl_generics ::umari::domain_id::FromDomainIds for #ident #ty_generics #where_clause {
                type Args = (#(#args,)*);

                fn from_domain_ids(args: Self::Args, bindings: &::umari::domain_id::DomainIdBindings) -> ::std::result::Result<Self, ::umari::error::FromDomainIdsError> {
                    Ok(#ident #body)
                }
            }
        }
    }
}

impl Parse for DeriveFromDomainIds {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: DeriveInput = input.parse()?;
        let is_unnamed = matches!(
            input.data,
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Unnamed(_),
                ..
            })
        );
        let attrs = DomainIdAttrs::parse(input.data)?;

        Ok(DeriveFromDomainIds {
            ident: input.ident,
            generics: input.generics,
            is_unnamed,
            attrs,
        })
    }
}
