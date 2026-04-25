use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    DeriveInput, Generics, Ident,
    parse::{Parse, ParseStream},
};

use crate::DomainIdAttrs;

pub struct DeriveDomainIds {
    ident: Ident,
    generics: Generics,
    attrs: DomainIdAttrs,
}

impl DeriveDomainIds {
    pub fn expand(self) -> TokenStream {
        let Self {
            ident,
            generics,
            attrs,
        } = self;

        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        let domain_id_fields = attrs.domain_ids.iter().map(|(_, domain_id)| domain_id);
        let domain_ids_inserts = attrs.domain_ids.iter().map(|(ident, domain_id)| {
            quote! {
                bindings.insert(#domain_id, ::std::string::ToString::to_string(&self.#ident));
            }
        });

        quote! {
            #[automatically_derived]
            impl #impl_generics ::umari::domain_id::DomainIds for #ident #ty_generics #where_clause {
                const DOMAIN_ID_FIELDS: &'static [&'static str] = &[#( #domain_id_fields ,)*];

                fn domain_ids(&self) -> ::umari::domain_id::DomainIdBindings {
                    let mut bindings = ::umari::domain_id::DomainIdBindings::new();
                    #( #domain_ids_inserts )*
                    bindings
                }
            }
        }
    }
}

impl Parse for DeriveDomainIds {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: DeriveInput = input.parse()?;
        let attrs = DomainIdAttrs::parse(input.data)?;

        Ok(DeriveDomainIds {
            ident: input.ident,
            generics: input.generics,
            attrs,
        })
    }
}
