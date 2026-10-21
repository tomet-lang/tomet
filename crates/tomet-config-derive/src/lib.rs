//! Procedural derive macro for Tomet configuration groups.

use proc_macro::TokenStream;
use syn::{DeriveInput, ItemStruct, parse_macro_input};

mod codegen;
mod parse;

#[proc_macro_derive(Config, attributes(config))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let item_struct = match input.data {
        syn::Data::Struct(s) => ItemStruct {
            attrs: input.attrs,
            vis: input.vis,
            struct_token: s.struct_token,
            ident: input.ident,
            generics: input.generics,
            fields: s.fields,
            semi_token: s.semi_token,
        },
        _ => {
            return syn::Error::new_spanned(input, "#[derive(Config)] is only supported on structs")
                .to_compile_error()
                .into();
        }
    };

    let container = match parse::ContainerConfig::from_ast(&item_struct) {
        Ok(c) => c,
        Err(e) => return e.to_compile_error().into(),
    };

    let fields = match parse::FieldConfig::collect_from_fields(&item_struct.fields) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error().into(),
    };

    let expanded = codegen::generate_config_impl(&container, &fields);
    TokenStream::from(expanded)
}
