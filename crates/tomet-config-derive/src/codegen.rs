//! Code generation for `#[derive(Config)]`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

use crate::parse::{ContainerConfig, FieldConfig};

pub fn generate_config_impl(container: &ContainerConfig, fields: &[FieldConfig]) -> TokenStream {
    let struct_ident = &container.ident;
    let group_name = &container.group_name;
    let group_doc = &container.doc;

    // 1. Generate ConfigSpecProvider::spec()
    let mut spec_entries = Vec::new();
    for f in fields {
        if f.skip || f.hide {
            continue;
        }

        if f.flatten {
            let field_ty = &f.ty;
            spec_entries.push(quote! {
                entries.extend(<#field_ty as ::tomet_config::schema::ConfigSpecProvider>::spec().entries);
            });
            continue;
        }

        let key = &f.key;
        let value_type = &f.value_type;
        let doc = &f.doc;
        let aliases = &f.aliases;
        let default_val_tokens = match &f.default_value {
            Some(d) => quote! { Some(#d) },
            None => quote! { None },
        };

        spec_entries.push(quote! {
            entries.push(::tomet_config::schema::ConfigEntrySpec {
                key: #key,
                value_type: #value_type,
                default_value: #default_val_tokens,
                description: #doc,
                aliases: &[ #( #aliases ),* ],
            });
        });
    }

    // 2. Generate from_root(&Value)
    let mut field_extractions = Vec::new();
    for f in fields {
        let field_ident = &f.ident;
        let ty = &f.ty;

        if f.skip {
            continue;
        }

        if f.flatten {
            field_extractions.push(quote! {
                cfg.#field_ident = <#ty>::from_root(root);
            });
            continue;
        }

        let key_parts: Vec<&str> = f.key.split('.').collect();
        let mut all_paths: Vec<Vec<&str>> = vec![key_parts];
        for alias in &f.aliases {
            all_paths.push(alias.split('.').collect());
        }

        let path_lookups: Vec<TokenStream> = all_paths
            .into_iter()
            .map(|parts| {
                quote! {
                    ::tomet_config::tree::get_path(root, &[ #( #parts ),* ])
                }
            })
            .collect();

        let lookup_chain = quote! {
            None #( .or_else(|| #path_lookups) )*
        };

        if let Some(parse_fn) = &f.parse_with {
            let type_str = quote!(#ty).to_string();
            let assign = if type_str.starts_with("Option <") || type_str.starts_with("Option<") {
                quote! { cfg.#field_ident = Some(val); }
            } else {
                quote! { cfg.#field_ident = val; }
            };
            field_extractions.push(quote! {
                if let Some(val) = (#lookup_chain).and_then(#parse_fn) {
                    #assign
                }
            });
        } else {
            let extraction = generate_typed_extraction(field_ident, ty, lookup_chain);
            field_extractions.push(extraction);
        }
    }

    quote! {
        impl ::tomet_config::schema::ConfigSpecProvider for #struct_ident {
            fn spec() -> ::tomet_config::schema::ConfigGroupSpec {
                let mut entries = Vec::new();
                #( #spec_entries )*
                ::tomet_config::schema::ConfigGroupSpec {
                    name: #group_name,
                    description: #group_doc,
                    entries,
                }
            }
        }

        impl #struct_ident {
            /// Extracts configuration from a normalized AST configuration root.
            pub fn from_root(root: &::tomet_ast::Value) -> Self {
                let mut cfg = Self::default();
                #( #field_extractions )*
                cfg
            }
        }
    }
}

fn generate_typed_extraction(
    field_ident: &syn::Ident,
    ty: &Type,
    lookup: TokenStream,
) -> TokenStream {
    let type_str = quote!(#ty).to_string();

    if type_str == "bool" {
        quote! {
            if let Some(v) = #lookup {
                if let Some(b) = ::tomet_tree::ValueExt::as_bool(v) {
                    cfg.#field_ident = b;
                } else if let Some(s) = ::tomet_tree::ValueExt::as_str(v) {
                    if s == "true" || s == "1" {
                        cfg.#field_ident = true;
                    } else if s == "false" || s == "0" {
                        cfg.#field_ident = false;
                    }
                }
            }
        }
    } else if type_str == "Option < bool >" || type_str == "Option<bool>" {
        quote! {
            if let Some(v) = #lookup {
                if let Some(b) = ::tomet_tree::ValueExt::as_bool(v) {
                    cfg.#field_ident = Some(b);
                } else if let Some(s) = ::tomet_tree::ValueExt::as_str(v) {
                    if s == "true" || s == "1" {
                        cfg.#field_ident = Some(true);
                    } else if s == "false" || s == "0" {
                        cfg.#field_ident = Some(false);
                    }
                }
            }
        }
    } else if type_str == "String" {
        quote! {
            if let Some(s) = (#lookup).and_then(::tomet_tree::ValueExt::as_str) {
                cfg.#field_ident = s.to_string();
            }
        }
    } else if type_str == "Option < String >" || type_str == "Option<String>" {
        quote! {
            if let Some(v) = #lookup {
                if let Some(s) = ::tomet_tree::ValueExt::as_str(v) {
                    cfg.#field_ident = Some(s.to_string());
                } else if let Some(b) = ::tomet_tree::ValueExt::as_bool(v) {
                    cfg.#field_ident = Some(b.to_string());
                }
            }
        }
    } else if type_str == "usize"
        || type_str == "u64"
        || type_str == "u32"
        || type_str == "i64"
        || type_str == "i32"
    {
        quote! {
            if let Some(n) = (#lookup).and_then(::tomet_tree::ValueExt::as_i64) {
                cfg.#field_ident = n as #ty;
            }
        }
    } else if type_str.starts_with("Option < usize >")
        || type_str.starts_with("Option<usize>")
        || type_str.starts_with("Option < u64 >")
        || type_str.starts_with("Option<u64>")
    {
        quote! {
            if let Some(n) = (#lookup).and_then(::tomet_tree::ValueExt::as_i64) {
                cfg.#field_ident = Some(n as _);
            }
        }
    } else if type_str.contains("Vec < String >") || type_str.contains("Vec<String>") {
        quote! {
            if let Some(v) = #lookup {
                ::tomet_config::tree::collect_paths(v, &mut cfg.#field_ident);
            }
        }
    } else {
        // Fallback for custom types: try ValueExt conversion if available
        quote! {
            let _ = &root;
        }
    }
}
