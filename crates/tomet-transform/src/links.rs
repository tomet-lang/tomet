use std::path::{Path, PathBuf};

use tomet_ast::{Document, Element, Value};
use tomet_semantics::{ElementKind, TargetScheme, classify_std_lenient, link_target, target_scheme};
use tomet_tree::for_each_element_mut;

/// Destination target format when rewriting `ref:` links and `@embed` assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetMode {
    /// Web format: URL slug for notes, and URL prefix for static assets
    WebSlug {
        url_prefix: String,
        asset_prefix: String,
    },
    /// Local format: Relative or normalized file path
    FilePath { ext: Option<String> },
}

fn is_doc_path(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "tmt" | "tm" | "md"))
        .unwrap_or(true)
}

impl TargetMode {
    pub fn format_path(
        &self,
        target_path: &Path,
        fragment: Option<&str>,
        _from_path: Option<&Path>,
    ) -> String {
        match self {
            TargetMode::WebSlug {
                url_prefix,
                asset_prefix,
            } => {
                let p_str = target_path.to_string_lossy().replace('\\', "/");
                let is_doc = is_doc_path(target_path);

                let mut res = if is_doc {
                    let clean_slug = p_str
                        .strip_suffix(".tmt")
                        .or_else(|| p_str.strip_suffix(".tm"))
                        .or_else(|| p_str.strip_suffix(".md"))
                        .unwrap_or(&p_str);

                    let prefix = url_prefix.trim_end_matches('/');
                    if prefix.is_empty() {
                        format!("/{}", clean_slug.trim_start_matches('/'))
                    } else {
                        format!("{}/{}", prefix, clean_slug.trim_start_matches('/'))
                    }
                } else {
                    let prefix = asset_prefix.trim_end_matches('/');
                    if prefix.is_empty() {
                        format!("/{}", p_str.trim_start_matches('/'))
                    } else {
                        format!("{}/{}", prefix, p_str.trim_start_matches('/'))
                    }
                };

                if let Some(frag) = fragment {
                    if !frag.is_empty() {
                        res.push('#');
                        res.push_str(frag);
                    }
                }
                res
            }
            TargetMode::FilePath { ext } => {
                let p_str = target_path.to_string_lossy().replace('\\', "/");
                let is_doc = is_doc_path(target_path);

                let mut res = if is_doc {
                    if let Some(new_ext) = ext {
                        let clean_ext = new_ext.trim_start_matches('.');
                        let without_ext = p_str
                            .strip_suffix(".tmt")
                            .or_else(|| p_str.strip_suffix(".tm"))
                            .or_else(|| p_str.strip_suffix(".md"))
                            .unwrap_or(&p_str);
                        format!("{without_ext}.{clean_ext}")
                    } else {
                        p_str
                    }
                } else {
                    p_str
                };

                if let Some(frag) = fragment {
                    if !frag.is_empty() {
                        res.push('#');
                        res.push_str(frag);
                    }
                }
                res
            }
        }
    }
}

/// Walks `doc` and resolves any `ref:` targets and local `@embed` targets using `resolver`.
///
/// - Resolved links have their target rewritten according to `mode`.
/// - Unresolved links are rewritten to the `unresolved:` scheme (e.g. `unresolved:Note Name`).
pub fn resolve_document_links<F>(
    doc: &mut Document,
    from_path: Option<&Path>,
    resolver: F,
    mode: &TargetMode,
) where
    F: Fn(&str, Option<&Path>) -> Option<PathBuf>,
{
    for_each_element_mut(doc, |el| {
        let kind = classify_std_lenient(el);
        if kind != ElementKind::Link && kind != ElementKind::Embed {
            return;
        }

        let Some(raw_target) = link_target(el, &kind) else {
            return;
        };
        let (scheme, rest) = target_scheme(&raw_target);

        // Never rewrite external URLs, in-document IDs, or absolute web paths
        if scheme == TargetScheme::Url || scheme == TargetScheme::Id || rest.starts_with('/') {
            return;
        }

        // For links: only resolve ref: targets
        if kind == ElementKind::Link && scheme != TargetScheme::Ref {
            return;
        }

        // For embeds: resolve ref: or local file: targets (e.g. +hash.png, ./image.png)
        if kind == ElementKind::Embed && scheme != TargetScheme::Ref && scheme != TargetScheme::File {
            return;
        }

        let (stem_or_name, fragment) = match rest.split_once('#') {
            Some((stem, frag)) => (stem, Some(frag)),
            None => (rest, None),
        };

        let new_target = match resolver(stem_or_name, from_path) {
            Some(resolved_path) => mode.format_path(&resolved_path, fragment, from_path),
            None => format!("unresolved:{rest}"),
        };

        update_element_target(el, new_target);
    });
}

fn update_element_target(el: &mut Element, new_target: String) {
    match &mut el.args {
        Some(Value::String(s)) => {
            *s = new_target;
        }
        Some(Value::Map(entries)) => {
            let mut found = false;
            for (k, v) in entries.iter_mut() {
                if k == "target" || k == "ref" {
                    *k = "target".to_string();
                    *v = Value::String(new_target.clone());
                    found = true;
                    break;
                }
            }
            if !found {
                entries.push(("target".to_string(), Value::String(new_target)));
            }
        }
        _ => {
            el.args = Some(Value::String(new_target));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_doc(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("valid source")
    }

    #[test]
    fn test_resolve_links_web_slug() {
        let mut doc = parse_doc(
            r#"
- @link("ref:Linux")[Go to Linux]
- @link("ref:Linux#kernel")[Go to Kernel]
- @link("ref:MissingNote")[Go to Missing]
- @link("https://example.com")[External]
"#,
        );

        let resolver = |target: &str, _from: Option<&Path>| -> Option<PathBuf> {
            if target == "Linux" {
                Some(PathBuf::from("30-39 Knowledge/Linux.tmt"))
            } else {
                None
            }
        };

        let mode = TargetMode::WebSlug {
            url_prefix: "/docs".into(),
            asset_prefix: "/vault".into(),
        };

        resolve_document_links(&mut doc, None, resolver, &mode);

        let mut targets = Vec::new();
        for_each_element_mut(&mut doc, |el| {
            if let Some((_, t)) = tomet_semantics::link_target_of(el) {
                targets.push(t);
            }
        });

        assert_eq!(
            targets,
            vec![
                "/docs/30-39 Knowledge/Linux".to_string(),
                "/docs/30-39 Knowledge/Linux#kernel".to_string(),
                "unresolved:MissingNote".to_string(),
                "https://example.com".to_string(),
            ]
        );
    }

    #[test]
    fn test_resolve_embed_assets() {
        let mut doc = parse_doc(
            r#"
- @embed(+8c3002a8a891b78137b6547f600a88141a828640.png)
- @embed("ref:+700fe7be15805a34ad1044c351f7dde08c010ec1.png")
- @embed("https://example.com/cat.png")
- @embed(+missing.png)
- @link("ref:manual.pdf")[Download PDF]
"#,
        );

        let resolver = |target: &str, _from: Option<&Path>| -> Option<PathBuf> {
            match target {
                "+8c3002a8a891b78137b6547f600a88141a828640.png" => {
                    Some(PathBuf::from("30-39 Knowledge/ミーム/-/+8c3002a8a891b78137b6547f600a88141a828640.png"))
                }
                "+700fe7be15805a34ad1044c351f7dde08c010ec1.png" => {
                    Some(PathBuf::from("50-59 Sandbox/ゲーム/-/+700fe7be15805a34ad1044c351f7dde08c010ec1.png"))
                }
                "manual.pdf" => {
                    Some(PathBuf::from("40-49 Master/manual.pdf"))
                }
                _ => None,
            }
        };

        let mode = TargetMode::WebSlug {
            url_prefix: "/docs".into(),
            asset_prefix: "/vault".into(),
        };

        resolve_document_links(&mut doc, None, resolver, &mode);

        let mut targets = Vec::new();
        for_each_element_mut(&mut doc, |el| {
            if let Some((_, t)) = tomet_semantics::link_target_of(el) {
                targets.push(t);
            }
        });

        assert_eq!(
            targets,
            vec![
                "/vault/30-39 Knowledge/ミーム/-/+8c3002a8a891b78137b6547f600a88141a828640.png".to_string(),
                "/vault/50-59 Sandbox/ゲーム/-/+700fe7be15805a34ad1044c351f7dde08c010ec1.png".to_string(),
                "https://example.com/cat.png".to_string(),
                "unresolved:+missing.png".to_string(),
                "/vault/40-49 Master/manual.pdf".to_string(),
            ]
        );
    }
}
