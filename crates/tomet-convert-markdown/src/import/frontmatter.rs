//! YAML frontmatter (`---\n...\n---`) extraction and conversion to
//! `tomet_ast::Value` entries for the synthesized `@meta` block.

use tomet_ast::Value;

pub(super) fn extract_yaml_frontmatter(src: &str) -> (Option<Vec<(String, Value)>>, &str) {
    let trimmed = src.trim_start();
    if !trimmed.starts_with("---") {
        return (None, src);
    }

    let rest = &trimmed[3..];
    if !rest.starts_with('\n') && !rest.starts_with("\r\n") {
        return (None, src);
    }

    let end_pos = if let Some(pos) = rest.find("\n---") {
        pos
    } else if let Some(pos) = rest.find("\n...") {
        pos
    } else {
        return (None, src);
    };

    let yaml_text = &rest[..end_pos];
    let closing_slice = &rest[end_pos..];
    let after_closing_idx =
        if closing_slice.starts_with("\n---") || closing_slice.starts_with("\n...") {
            end_pos + 4
        } else if closing_slice.starts_with("\r\n---") || closing_slice.starts_with("\r\n...") {
            end_pos + 5
        } else {
            return (None, src);
        };

    let remaining_src = rest[after_closing_idx..].trim_start_matches(['\r', '\n']);

    if let Ok(serde_yaml::Value::Mapping(map)) =
        serde_yaml::from_str::<serde_yaml::Value>(yaml_text)
    {
        let entries: Vec<(String, Value)> = map
            .into_iter()
            .map(|(k, v)| (yaml_key_to_string(k), yaml_to_value(v)))
            .collect();
        if !entries.is_empty() {
            return (Some(entries), remaining_src);
        }
    }

    (None, remaining_src)
}

fn yaml_to_value(v: serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else {
                Value::Float(n.as_f64().unwrap_or_default())
            }
        }
        serde_yaml::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with("[[") && trimmed.ends_with("]]") && trimmed.len() > 4 {
                let inner = &trimmed[2..trimmed.len() - 2];
                let unescaped = inner.replace(r"\|", "|");
                let target = if let Some((t, _)) = unescaped.split_once('|') {
                    t.trim()
                } else {
                    unescaped.trim()
                };
                if target.contains([
                    '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
                ]) {
                    let escaped = target.replace('\\', "\\\\").replace('"', "\\\"");
                    Value::String(format!("@link(ref:\"{escaped}\")"))
                } else {
                    Value::String(format!("@link(ref:{target})"))
                }
            } else {
                Value::String(s)
            }
        }
        serde_yaml::Value::Sequence(items) => {
            Value::Seq(items.into_iter().map(yaml_to_value).collect())
        }
        serde_yaml::Value::Mapping(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| (yaml_key_to_string(k), yaml_to_value(v)))
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => yaml_to_value(tagged.value),
    }
}

fn yaml_key_to_string(k: serde_yaml::Value) -> String {
    match k {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::import::from_markdown;
    use tomet_ast::{Block, Sigil, Value};

    #[test]
    fn yaml_frontmatter_converts_to_meta() {
        let src = "---\ntitle: \"Hello\"\nauthor: Alice\ndraft: false\n---\n\n# Main Title\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::named("meta"));
                assert_eq!(
                    el.value,
                    Some(tomet_ast::ElementValue::from_map(Value::Map(vec![
                        ("title".to_string(), Value::String("Hello".to_string())),
                        ("author".to_string(), Value::String("Alice".to_string())),
                        ("draft".to_string(), Value::Bool(false)),
                    ])))
                );
            }
            other => panic!("expected meta element, got {other:?}"),
        }
    }

    #[test]
    fn yaml_frontmatter_with_arrays_converts_to_meta() {
        let src = "---\ntitle: \"Doc\"\ntags:\n  - rust\n  - tomet\n---\n\n# Main\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::named("meta"));
                assert_eq!(
                    el.value,
                    Some(tomet_ast::ElementValue::from_map(Value::Map(vec![
                        ("title".to_string(), Value::String("Doc".to_string())),
                        (
                            "tags".to_string(),
                            Value::Seq(vec![
                                Value::String("rust".to_string()),
                                Value::String("tomet".to_string()),
                            ])
                        ),
                    ])))
                );
            }
            other => panic!("expected meta element, got {other:?}"),
        }
    }

    #[test]
    fn frontmatter_wikilinks_convert_to_ref_maps() {
        let src = "---\ntopics:\n  - \"[[@Templater]]\"\n  - \"[[@QuickAdd]]\"\n---\n\n# Title\n";
        let doc = from_markdown(src);
        assert_eq!(doc.blocks.len(), 2);
        let Block::Element(el) = &doc.blocks[0] else {
            panic!("expected meta element");
        };
        assert_eq!(
            el.value,
            Some(tomet_ast::ElementValue::from_map(Value::Map(vec![(
                "topics".to_string(),
                Value::Seq(vec![
                    Value::String("@link(ref:@Templater)".to_string()),
                    Value::String("@link(ref:@QuickAdd)".to_string()),
                ])
            )])))
        );
    }
}
