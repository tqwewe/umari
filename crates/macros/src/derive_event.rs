use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Ident, LitStr,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

pub struct DeriveEvent {
    ident: Ident,
    event_type: LitStr,
    crypto_scope: Option<Ident>,
}

impl DeriveEvent {
    pub fn expand(self) -> TokenStream {
        let Self {
            ident,
            event_type,
            crypto_scope,
        } = self;

        let encryption_scope_fn = crypto_scope.map(|field| {
            let field_str = field.to_string();
            quote! {
                fn encryption_scope(&self) -> Option<String> {
                    Some(format!("{}:{}", #field_str, self.#field))
                }
            }
        });

        quote! {
            #[automatically_derived]
            impl ::umari::event::Event for #ident {
                const EVENT_TYPE: &'static str = #event_type;

                #encryption_scope_fn
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

        let mut crypto_scope = None;
        match &input.data {
            syn::Data::Struct(data_struct) => {
                for field in &data_struct.fields {
                    for attr in &field.attrs {
                        if attr.path().is_ident("crypto_scope")
                            && crypto_scope.replace(field.ident.clone().unwrap()).is_some()
                        {
                            return Err(syn::Error::new(attr.span(), "crypto_scope defined twice"));
                        }
                    }
                }
            }
            syn::Data::Enum(_) => {}
            syn::Data::Union(_) => {}
        }

        Ok(DeriveEvent {
            ident: input.ident,
            event_type,
            crypto_scope,
        })
    }
}
