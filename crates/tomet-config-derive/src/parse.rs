//! Syntax tree and attribute parsing for `#[derive(Config)]`.

use syn::{Attribute, Expr, ExprLit, Field, Fields, Ident, ItemStruct, Lit, Meta, Type};

/// Parsed container-level configuration metadata.
pub struct ContainerConfig {
    pub ident: Ident,
    pub group_name: String,
    pub doc: String,
}

/// Parsed field-level configuration metadata.
pub struct FieldConfig {
    pub ident: Ident,
    pub ty: Type,
    pub key: String,
    pub value_type: String,
    pub default_value: Option<String>,
    pub doc: String,
    pub aliases: Vec<String>,
    pub flatten: bool,
    pub skip: bool,
    pub hide: bool,
    pub parse_with: Option<syn::Path>,
}

impl ContainerConfig {
    pub fn from_ast(input: &ItemStruct) -> syn::Result<Self> {
        let mut group_name = None;
        let mut doc_override = None;
        let mut doc_lines = Vec::new();

        for attr in &input.attrs {
            if attr.path().is_ident("doc") {
                if let Some(line) = extract_doc_comment(attr) {
                    doc_lines.push(line);
                }
            } else if attr.path().is_ident("config") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("group") {
                        let value = meta.value()?;
                        let s: syn::LitStr = value.parse()?;
                        group_name = Some(s.value());
                        Ok(())
                    } else if meta.path.is_ident("doc") {
                        let value = meta.value()?;
                        let s: syn::LitStr = value.parse()?;
                        doc_override = Some(s.value());
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized container attribute"))
                    }
                })?;
            }
        }

        let doc = doc_override.unwrap_or_else(|| clean_doc_lines(&doc_lines));
        let ident = input.ident.clone();
        let group_name = group_name.unwrap_or_else(|| {
            let s = ident.to_string();
            s.strip_suffix("Config").unwrap_or(&s).to_lowercase()
        });

        Ok(Self {
            ident,
            group_name,
            doc,
        })
    }
}

impl FieldConfig {
    pub fn from_field(field: &Field) -> syn::Result<Self> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new_spanned(field, "expected named field"))?;
        let ty = field.ty.clone();

        let mut key = None;
        let mut value_type_override = None;
        let mut default_value = None;
        let mut doc_override = None;
        let mut doc_lines = Vec::new();
        let mut aliases = Vec::new();
        let mut flatten = false;
        let mut skip = false;
        let mut hide = false;
        let mut parse_with = None;

        for attr in &field.attrs {
            if attr.path().is_ident("doc") {
                if let Some(line) = extract_doc_comment(attr) {
                    doc_lines.push(line);
                }
            } else if attr.path().is_ident("config") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("key") {
                        let val = meta.value()?;
                        let s: syn::LitStr = val.parse()?;
                        key = Some(s.value());
                        Ok(())
                    } else if meta.path.is_ident("alias") {
                        let val = meta.value()?;
                        let s: syn::LitStr = val.parse()?;
                        aliases.push(s.value());
                        Ok(())
                    } else if meta.path.is_ident("default") {
                        let val = meta.value()?;
                        let expr: Expr = val.parse()?;
                        default_value = Some(expr_to_string(&expr));
                        Ok(())
                    } else if meta.path.is_ident("type") {
                        let val = meta.value()?;
                        let s: syn::LitStr = val.parse()?;
                        value_type_override = Some(s.value());
                        Ok(())
                    } else if meta.path.is_ident("doc") {
                        let val = meta.value()?;
                        let s: syn::LitStr = val.parse()?;
                        doc_override = Some(s.value());
                        Ok(())
                    } else if meta.path.is_ident("flatten") {
                        flatten = true;
                        Ok(())
                    } else if meta.path.is_ident("skip") {
                        skip = true;
                        Ok(())
                    } else if meta.path.is_ident("hide") || meta.path.is_ident("skip_spec") {
                        hide = true;
                        Ok(())
                    } else if meta.path.is_ident("parse_with") {
                        let val = meta.value()?;
                        if let Ok(s) = val.parse::<syn::LitStr>() {
                            let p: syn::Path = s.parse()?;
                            parse_with = Some(p);
                        } else {
                            let p: syn::Path = val.parse()?;
                            parse_with = Some(p);
                        }
                        Ok(())
                    } else {
                        Err(meta.error("unrecognized field attribute in #[config(...)]"))
                    }
                })?;
            }
        }

        let key = key.unwrap_or_else(|| ident.to_string());
        let doc = doc_override.unwrap_or_else(|| clean_doc_lines(&doc_lines));
        let inferred_type = infer_type_string(&ty);
        let value_type = value_type_override.unwrap_or(inferred_type);

        Ok(Self {
            ident,
            ty,
            key,
            value_type,
            default_value,
            doc,
            aliases,
            flatten,
            skip,
            hide,
            parse_with,
        })
    }

    pub fn collect_from_fields(fields: &Fields) -> syn::Result<Vec<Self>> {
        match fields {
            Fields::Named(named) => {
                let mut out = Vec::new();
                for field in &named.named {
                    out.push(Self::from_field(field)?);
                }
                Ok(out)
            }
            Fields::Unnamed(unnamed) => Err(syn::Error::new_spanned(
                unnamed,
                "#[derive(Config)] is not supported on tuple structs",
            )),
            Fields::Unit => Ok(Vec::new()),
        }
    }
}

fn extract_doc_comment(attr: &Attribute) -> Option<String> {
    if let Meta::NameValue(meta) = &attr.meta {
        if let Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) = &meta.value
        {
            return Some(s.value());
        }
    }
    None
}

fn clean_doc_lines(lines: &[String]) -> String {
    let cleaned: Vec<&str> = lines
        .iter()
        .map(|l| {
            let trimmed = l.trim_start();
            trimmed.strip_prefix(' ').unwrap_or(trimmed)
        })
        .collect();
    cleaned.join(" ").trim().to_string()
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Str(s) => s.value(),
            Lit::Bool(b) => b.value().to_string(),
            Lit::Int(i) => i.base10_digits().to_string(),
            Lit::Float(f) => f.base10_digits().to_string(),
            _ => quote::quote!(#expr).to_string(),
        },
        _ => quote::quote!(#expr).to_string(),
    }
}

fn infer_type_string(ty: &Type) -> String {
    match ty {
        Type::Path(p) => {
            if let Some(segment) = p.path.segments.last() {
                let ident_str = segment.ident.to_string();
                match ident_str.as_str() {
                    "bool" => "bool".to_string(),
                    "String" => "string".to_string(),
                    "usize" | "u32" | "u64" | "i32" | "i64" => "int".to_string(),
                    "f64" | "f32" => "float".to_string(),
                    "Option" => {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                                return infer_type_string(inner);
                            }
                        }
                        "any".to_string()
                    }
                    "Vec" => {
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                                return format!("list({})", infer_type_string(inner));
                            }
                        }
                        "list".to_string()
                    }
                    "HashMap" | "BTreeMap" => "map".to_string(),
                    other => other.to_string(),
                }
            } else {
                "any".to_string()
            }
        }
        _ => "any".to_string(),
    }
}
