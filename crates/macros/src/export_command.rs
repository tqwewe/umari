use heck::ToPascalCase;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    FnArg, Ident, ItemFn, Type,
    parse::{Parse, ParseStream},
    spanned::Spanned,
};

pub struct ExportCommand {
    input_fn: ItemFn,
    fn_name: Ident,
    input: Type,
}

impl ExportCommand {
    pub fn expand(self) -> TokenStream {
        let Self {
            input_fn,
            fn_name,
            input,
        } = self;
        let zst_name = Ident::new(
            &format!("{}Export", fn_name.to_string().to_pascal_case()),
            Span::call_site(),
        );
        let type_alias_name = Ident::new(
            &format!("__internal_{}Export", fn_name.to_string().to_pascal_case()),
            Span::call_site(),
        );

        quote! {
            #input_fn

            type #type_alias_name = ::umari::runtime::command::CommandExport<#zst_name>;

            struct #zst_name;

            impl ::umari::runtime::command::ExportedCommand for #zst_name {
                type Input = #input;

                fn execute(input: Self::Input, context: ::umari::command::CommandContext) -> anyhow::Result<::umari::runtime::command::ExecuteOutput> {
                    ::std::result::Result::Ok(
                        <::umari::command::ExecuteOutput as ::std::convert::Into<::umari::runtime::command::ExecuteOutput>>::into(
                            #fn_name(input, context)?
                        )
                    )
                }
            }

            ::umari::runtime::command::export!(#type_alias_name with_types_in ::umari::runtime::command);
        }
    }
}

impl Parse for ExportCommand {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let input: ItemFn = input.parse()?;

        let sig_span = input.sig.span();
        let mut inputs = input.sig.inputs.iter();

        let first_input = inputs
            .next()
            .ok_or(syn::Error::new(sig_span, "missing command input parameter"))?;
        let FnArg::Typed(input_ty) = first_input else {
            return Err(syn::Error::new(
                first_input.span(),
                "expected command input",
            ));
        };

        let second_input = inputs.next().ok_or(syn::Error::new(
            sig_span,
            "missing command context parameter",
        ))?;
        let FnArg::Typed(_) = second_input else {
            return Err(syn::Error::new(
                second_input.span(),
                "expected command context parameter",
            ));
        };

        Ok(ExportCommand {
            fn_name: input.sig.ident.clone(),
            input: *input_ty.ty.clone(),
            input_fn: input,
        })
    }
}
