use typedmark_ast::Value;

/// Keys recognized in a bare `@(key:...)` element's `args` map, checked
/// in this order, to infer its kind when no explicit `@name` is given.
///
/// This exists only for `url`/`file`/`ref`, which need to work *inline* in
/// running prose where every character counts -- it's not a general
/// "authors may omit the name" convenience. `meta` is deliberately **not**
/// here: `@meta(format:tag){...}` is always block-level, never needs to be
/// terse, so it always takes the explicit name. (A bare `@(meta:yaml){...}`
/// would otherwise look like it means the same thing as
/// `@meta(format:yaml){...}` but silently not be: real JSON/YAML/TOML
/// parsing is driven by a `format` key in `args`, checked independently of
/// the sigil/name -- see `typedmark_parser`'s `embedded_format` module --
/// and a bare `@(meta:yaml)`'s `args` has no such key.) `links` is
/// explicit-name-only for the same reason `meta` now is -- it was never in
/// this list.
pub const INFERRED_AT_KEYS: [&str; 3] = ["url", "file", "ref"];

/// Infers a bare `@(...)` element's kind: whichever of [`INFERRED_AT_KEYS`]
/// appears first as a top-level key in `args`. `None` if `args` isn't a
/// map, or none of those keys are present (the caller then typically falls
/// back to a generic "at" kind).
pub fn infer_at_kind(args: Option<&Value>) -> Option<&'static str> {
    let Some(Value::Map(entries)) = args else {
        return None;
    };
    INFERRED_AT_KEYS
        .iter()
        .find(|key| entries.iter().any(|(k, _)| k == *key))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_url_file_ref_from_a_bare_at_element() {
        for key in ["url", "file", "ref"] {
            let args = Value::Map(vec![(key.to_string(), Value::String("x".into()))]);
            assert_eq!(infer_at_kind(Some(&args)), Some(key));
        }
    }

    #[test]
    fn does_not_infer_meta_from_a_bare_at_element() {
        let args = Value::Map(vec![("meta".to_string(), Value::String("yaml".into()))]);
        assert_eq!(infer_at_kind(Some(&args)), None);
    }

    #[test]
    fn no_recognized_key_or_non_map_args_infers_nothing() {
        assert_eq!(infer_at_kind(None), None);
        assert_eq!(infer_at_kind(Some(&Value::String("x".into()))), None);
        let args = Value::Map(vec![("other".to_string(), Value::Int(1))]);
        assert_eq!(infer_at_kind(Some(&args)), None);
    }
}
