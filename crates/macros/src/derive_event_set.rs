use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{
    DeriveInput, Ident, LitStr, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

pub struct DeriveEventSet {
    ident: Ident,
    events: Vec<QueryEvent>,
}

struct QueryEvent {
    scope: Option<Vec<ScopeEntry>>,
    ident: Ident,
    ty: Type,
}

enum ScopeEntry {
    Dynamic(Ident),
    Static(Ident, LitStr),
}

impl Parse for ScopeEntry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        if input.peek(Token![=]) {
            let _: Token![=] = input.parse()?;
            let value: LitStr = input.parse()?;
            Ok(ScopeEntry::Static(ident, value))
        } else {
            Ok(ScopeEntry::Dynamic(ident))
        }
    }
}

impl DeriveEventSet {
    pub fn expand(self) -> TokenStream {
        let Self { ident, events } = self;

        let event_types = events.iter().map(|QueryEvent { ty, .. }| ty);
        let event_domain_ids = events
            .iter()
            .map(|QueryEvent { scope, ty, .. }| match scope {
                Some(scope) => {
                    let dynamic: Vec<_> = scope
                        .iter()
                        .filter_map(|e| match e {
                            ScopeEntry::Dynamic(id) => {
                                Some(LitStr::new(&id.to_string(), id.span()))
                            }
                            ScopeEntry::Static(..) => None,
                        })
                        .collect();

                    let static_pairs: Vec<_> = scope
                        .iter()
                        .filter_map(|e| match e {
                            ScopeEntry::Static(id, val) => {
                                let field = LitStr::new(&id.to_string(), id.span());
                                Some(quote! { (#field, #val) })
                            }
                            ScopeEntry::Dynamic(_) => None,
                        })
                        .collect();

                    quote! {
                        ::umari_core::event::EventDomainId {
                            event_type: <#ty as ::umari_core::event::Event>::EVENT_TYPE,
                            dynamic_fields: &[ #(#dynamic,)* ],
                            static_fields: &[ #(#static_pairs,)* ],
                        }
                    }
                }
                None => {
                    quote! {
                        ::umari_core::event::EventDomainId {
                            event_type: <#ty as ::umari_core::event::Event>::EVENT_TYPE,
                            dynamic_fields: <#ty as ::umari_core::event::Event>::DOMAIN_ID_FIELDS,
                            static_fields: &[],
                        }
                    }
                }
            });

        let match_arms = events.iter().map(
            |QueryEvent {
                 ident: variant_ident,
                 ty,
                 ..
             }| {
                quote! {
                    <#ty as ::umari_core::event::Event>::EVENT_TYPE => {
                        ::std::option::Option::Some(
                            ::umari_core::__private::serde_json::from_value::<#ty>(data)
                                .map(#ident::#variant_ident)
                                .map_err(::umari_core::error::SerializationError::from)
                        )
                    }
                }
            },
        );

        let as_into_event_impls = events.iter().map(
            |QueryEvent {
                 ident: variant_ident,
                 ty,
                 ..
             }| {
                quote! {
                    #[automatically_derived]
                    impl ::umari_core::event::AsEvent<#ty> for #ident {
                        fn as_event(&self) -> ::std::option::Option<&#ty> {
                            match self {
                                #ident::#variant_ident(ev) => ::std::option::Option::Some(ev),
                                _ => ::std::option::Option::None,
                            }
                        }
                    }

                    #[automatically_derived]
                    impl ::umari_core::event::IntoEvent<#ty> for #ident {
                        fn into_event(self) -> ::std::option::Option<#ty> {
                            match self {
                                #ident::#variant_ident(ev) => ::std::option::Option::Some(ev),
                                _ => ::std::option::Option::None,
                            }
                        }
                    }
                }
            },
        );

        let validations = events.iter().filter_map(|QueryEvent { scope, ty, .. }| {
            scope.as_ref().map(|scope_entries| {
                let validations = scope_entries.iter().map(|entry| {
                    let (field_ident, field_str) = match entry {
                        ScopeEntry::Dynamic(id) => (id, id.to_string()),
                        ScopeEntry::Static(id, _) => (id, id.to_string()),
                    };
                    quote_spanned! {
                        field_ident.span()=>
                        const _: () = {
                            const fn contains_str(haystack: &[&str], needle: &str) -> bool {
                                let mut i = 0;
                                while i < haystack.len() {
                                    if const_str_eq(haystack[i], needle) {
                                        return true;
                                    }
                                    i += 1;
                                }
                                false
                            }

                            const fn const_str_eq(a: &str, b: &str) -> bool {
                                let a = a.as_bytes();
                                let b = b.as_bytes();
                                if a.len() != b.len() { return false; }
                                let mut i = 0;
                                while i < a.len() {
                                    if a[i] != b[i] { return false; }
                                    i += 1;
                                }
                                true
                            }

                            if !contains_str(<#ty as ::umari_core::event::Event>::DOMAIN_ID_FIELDS, #field_str) {
                                panic!(concat!("Domain ID '", #field_str, "' not found in ", stringify!(#ty), "::DOMAIN_ID_FIELDS"));
                            }
                        };
                    }
                });

                quote! {
                    #( #validations )*
                }
            })
        });

        quote! {
            #[automatically_derived]
            impl ::umari_core::event::EventSet for #ident {
                type Item = Self;

                fn event_types() -> ::std::vec::Vec<&'static str> {
                    ::std::vec![ #( <#event_types as ::umari_core::event::Event>::EVENT_TYPE, )* ]
                }

                fn event_domain_ids() -> ::std::vec::Vec<::umari_core::event::EventDomainId> {
                    ::std::vec![ #( #event_domain_ids , )* ]
                }

                fn from_event(event_type: &str, data: ::umari_core::__private::serde_json::Value) -> ::std::option::Option<::std::result::Result<Self::Item, ::umari_core::error::SerializationError>> {
                    match event_type {
                        #( #match_arms )*
                        _ => ::std::option::Option::None
                    }
                }
            }

            #( #as_into_event_impls )*

            #( #validations )*
        }
    }
}

impl Parse for DeriveEventSet {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: DeriveInput = input.parse()?;

        let events = match input.data {
            syn::Data::Enum(data) => data
                .variants
                .into_iter()
                .map(|variant| match variant.fields {
                    syn::Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
                        let scope = variant.attrs.into_iter().find_map(|attr| {
                            if attr.path().is_ident("scope") {
                                match attr.meta {
                                    syn::Meta::List(list) => match list.parse_args_with(Punctuated::<ScopeEntry, Token![,]>::parse_terminated) {
                                        Ok(entries) => Some(Ok(entries.into_iter().collect())),
                                        Err(err) => Some(Err(err)),
                                    },
                                    syn::Meta::Path(_) | syn::Meta::NameValue(_) => {
                                        Some(Err(syn::Error::new(
                                            attr.span(),
                                            "scope attribute only supports a list of domain ids",
                                        )))
                                    }
                                }
                            } else {
                                None
                            }
                        }).transpose()?;
                        let field = unnamed.unnamed.into_iter().next().unwrap();
                        Ok(QueryEvent { scope, ident: variant.ident, ty: field.ty })
                    }
                    _ => Err(syn::Error::new(
                        variant.fields.span(),
                        "EventSet requires one unnamed field per event type",
                    )),
                })
                .collect::<Result<_, _>>()?,
            _ => {
                return Err(syn::Error::new(
                    input.span(),
                    "EventSet can only be derived on enums",
                ));
            }
        };

        Ok(DeriveEventSet {
            ident: input.ident,
            events,
        })
    }
}
