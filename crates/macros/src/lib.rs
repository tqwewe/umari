mod derive_domain_ids;
mod derive_event;
mod derive_event_set;
mod derive_from_domain_ids;
mod export_command;

use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::spanned::Spanned;

use crate::derive_domain_ids::DeriveDomainIds;
use crate::derive_event::DeriveEvent;
use crate::derive_event_set::DeriveEventSet;
use crate::derive_from_domain_ids::DeriveFromDomainIds;
use crate::export_command::ExportCommand;

#[proc_macro_derive(DomainIds, attributes(domain_id))]
pub fn domain_ids(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveDomainIds);
    TokenStream::from(input.expand())
}

#[proc_macro_derive(FromDomainIds, attributes(domain_id, from_domain_id))]
pub fn from_domain_ids(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveFromDomainIds);
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

#[proc_macro_attribute]
pub fn export_command(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ExportCommand);
    TokenStream::from(input.expand())
}

struct DomainIdAttrs {
    domain_ids: Vec<(Option<syn::Ident>, syn::LitStr)>,
    non_domain_ids: Vec<(Option<syn::Ident>, NonDomainIdField)>,
}

enum NonDomainIdField {
    Explicit(Box<syn::Type>),
    Default,
}

mod kw {
    syn::custom_keyword!(default);
}

impl DomainIdAttrs {
    fn parse(data: syn::Data) -> Result<Self, syn::Error> {
        let mut domain_ids = Vec::new();
        let mut non_domain_ids = Vec::new();
        if let syn::Data::Struct(data) = data {
            for field in data.fields {
                let non_domain_id_field = {
                    let attr = field
                        .attrs
                        .iter()
                        .find(|attr| attr.path().is_ident("from_domain_id"));
                    match attr {
                        Some(attr) => match &attr.meta {
                            syn::Meta::List(meta) => {
                                let _default: kw::default = meta.parse_args()?;
                                NonDomainIdField::Default
                            }
                            meta => {
                                return Err(syn::Error::new(
                                    meta.span(),
                                    "expected `#[from_domain_id(default)]`",
                                ));
                            }
                        },
                        None => NonDomainIdField::Explicit(Box::new(field.ty)),
                    }
                };
                let attr = field
                    .attrs
                    .into_iter()
                    .find(|attr| attr.path().is_ident("domain_id"));
                match attr {
                    Some(attr) => match attr.meta {
                        syn::Meta::Path(path) => {
                            let ident = field.ident.ok_or_else(|| {
                                syn::Error::new(
                                    path.span(),
                                    "unnamed fields must specify the domain name manually",
                                )
                            })?;
                            let domain_id = syn::LitStr::new(&ident.to_string(), ident.span());
                            domain_ids.push((Some(ident), domain_id));
                        }
                        syn::Meta::List(list) => {
                            return Err(syn::Error::new(
                                list.span(),
                                "expected optional string literal",
                            ));
                        }
                        syn::Meta::NameValue(meta) => match meta.value {
                            syn::Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Str(domain_id),
                                ..
                            }) => {
                                domain_ids.push((field.ident, domain_id));
                            }
                            _ => {
                                return Err(syn::Error::new(
                                    meta.value.span(),
                                    "expected string literal",
                                ));
                            }
                        },
                    },
                    None => {
                        non_domain_ids.push((field.ident, non_domain_id_field));
                    }
                }
            }
        }

        Ok(DomainIdAttrs {
            domain_ids,
            non_domain_ids,
        })
    }
}
