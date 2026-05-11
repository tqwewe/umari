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
                        ::umari::event::EventDomainId {
                            event_type: <#ty as ::umari::event::Event>::EVENT_TYPE,
                            dynamic_fields: &[ #(#dynamic,)* ],
                            static_fields: &[ #(#static_pairs,)* ],
                        }
                    }
                }
                None => {
                    quote! {
                        ::umari::event::EventDomainId {
                            event_type: <#ty as ::umari::event::Event>::EVENT_TYPE,
                            dynamic_fields: <#ty as ::umari::domain_id::DomainIds>::DOMAIN_ID_FIELDS,
                            static_fields: &[],
                        }
                    }
                }
            });

        // Group variants by type (using token string as key) so that variants sharing
        // the same Rust type are matched in a single arm with scope-based if-else dispatch.
        let mut seen_match_keys: Vec<String> = Vec::new();
        let mut match_groups: Vec<(&Type, Vec<&QueryEvent>)> = Vec::new();
        for event in &events {
            let ty = &event.ty;
            let key = quote!(#ty).to_string();
            if let Some(pos) = seen_match_keys.iter().position(|k| k == &key) {
                match_groups[pos].1.push(event);
            } else {
                seen_match_keys.push(key);
                match_groups.push((ty, vec![event]));
            }
        }

        let match_arms = match_groups.iter().map(|(ty, group)| {
            // Separate scoped (has static fields) from fallback (no static constraints) variants.
            let mut scoped: Vec<(Vec<TokenStream>, &QueryEvent)> = Vec::new();
            let mut fallback: Option<&QueryEvent> = None;

            for event in group.iter() {
                let static_checks: Vec<TokenStream> = event
                    .scope
                    .as_ref()
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(|e| match e {
                                ScopeEntry::Static(id, val) => {
                                    let field = id.to_string();
                                    Some(quote! {
                                        data.get(#field).and_then(|v| v.as_str()) == ::std::option::Option::Some(#val)
                                    })
                                }
                                ScopeEntry::Dynamic(_) => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                if static_checks.is_empty() {
                    fallback = Some(event);
                } else {
                    scoped.push((static_checks, event));
                }
            }

            let arm_body = if scoped.is_empty() {
                // No static constraints at all — simple deserialization.
                let event = fallback.unwrap_or(group[0]);
                let variant_ident = &event.ident;
                quote! {
                    ::std::option::Option::Some(
                        ::umari::__private::serde_json::from_value::<#ty>(::std::clone::Clone::clone(data))
                            .map(#ident::#variant_ident)
                            .map_err(::umari::error::SerializationError::from)
                    )
                }
            } else {
                // Build if / else-if chain for each scoped variant, then an else branch.
                let mut chain = quote! {};
                for (i, (checks, event)) in scoped.iter().enumerate() {
                    let variant_ident = &event.ident;
                    let deserialize = quote! {
                        ::std::option::Option::Some(
                            ::umari::__private::serde_json::from_value::<#ty>(::std::clone::Clone::clone(data))
                                .map(#ident::#variant_ident)
                                .map_err(::umari::error::SerializationError::from)
                        )
                    };
                    let keyword = if i == 0 { quote!(if) } else { quote!(else if) };
                    chain = quote! { #chain #keyword #(#checks)&&* { #deserialize } };
                }

                let else_branch = match fallback {
                    Some(event) => {
                        let variant_ident = &event.ident;
                        quote! {
                            else {
                                ::std::option::Option::Some(
                                    ::umari::__private::serde_json::from_value::<#ty>(::std::clone::Clone::clone(data))
                                        .map(#ident::#variant_ident)
                                        .map_err(::umari::error::SerializationError::from)
                                )
                            }
                        }
                    }
                    None => quote! { else { ::std::option::Option::None } },
                };

                quote! { #chain #else_branch }
            };

            quote! {
                <#ty as ::umari::event::Event>::EVENT_TYPE => { #arm_body }
            }
        });

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

                            if !contains_str(<#ty as ::umari::domain_id::DomainIds>::DOMAIN_ID_FIELDS, #field_str) {
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
            impl ::umari::event::EventSet for #ident {
                type Item = Self;

                fn event_types() -> ::std::vec::Vec<&'static str> {
                    ::std::vec![ #( <#event_types as ::umari::event::Event>::EVENT_TYPE, )* ]
                }

                fn event_domain_ids() -> ::std::vec::Vec<::umari::event::EventDomainId> {
                    ::std::vec![ #( #event_domain_ids , )* ]
                }

                fn from_event(event_type: &str, data: &::umari::__private::serde_json::Value) -> ::std::option::Option<::std::result::Result<Self::Item, ::umari::error::SerializationError>> {
                    match event_type {
                        #( #match_arms )*
                        _ => ::std::option::Option::None
                    }
                }
            }

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
