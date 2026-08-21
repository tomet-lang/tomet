//! Small pure helpers for `@meta` field values, driven by
//! `typedmark_config::FieldConfig`: id generation/validation
//! (`generate_id_for_field`/`is_valid_id_format`) and ISO8601/RFC3339
//! timestamp conversion (`is_iso8601`/`format_rfc3339`). None of these
//! touch `typedmark_ast::Document` -- they're a grab bag of
//! generate/convert helpers, not a rules engine, so kept as plain
//! utility functions rather than an invented abstraction. Split out of
//! `typedmark-printer` (which still owns the `Document`-mutating
//! `ensure_document_id_with_config` and the `.tm`-syntax rendering that
//! call these) so `typedmark-formatter` can reuse the same
//! generate/validate/convert logic later without depending on printer's
//! whole-document-rebuild model.

use typedmark_config::FieldConfig;

pub fn generate_id_for_field(cfg: &FieldConfig) -> String {
    let prefix = cfg.prefix.as_deref().unwrap_or("");
    match cfg.field_type.as_deref() {
        Some("nanoid") => {
            let len = cfg.length.unwrap_or(8);
            let id = nanoid::nanoid!(len);
            format!("{prefix}{id}")
        }
        Some("uuid") => {
            let id = uuid::Uuid::new_v4().to_string();
            format!("{prefix}{id}")
        }
        _ => {
            let len = cfg.length.unwrap_or(8);
            let id = nanoid::nanoid!(len);
            format!("{prefix}{id}")
        }
    }
}

pub fn is_valid_id_format(existing: &str, cfg: &FieldConfig) -> bool {
    let prefix = cfg.prefix.as_deref().unwrap_or("");
    if !existing.starts_with(prefix) {
        return false;
    }
    let rest = &existing[prefix.len()..];
    let expected_len = cfg.length.unwrap_or(8);
    if rest.len() != expected_len {
        return false;
    }
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn is_iso8601(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-' {
        if bytes.len() == 10 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit());
        }
        if (bytes[10] == b'T' || bytes[10] == b' ') && bytes.len() >= 19 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit())
                && bytes[13] == b':'
                && bytes[16] == b':';
        }
    }
    false
}

pub fn format_rfc3339(s: &str, offset: Option<&str>) -> String {
    let trimmed = s.trim();
    if is_iso8601(trimmed) {
        if !trimmed.ends_with('Z')
            && !trimmed.contains('+')
            && trimmed.len() >= 19
            && !trimmed[10..].contains('-')
        {
            let off = offset.unwrap_or("Z");
            format!("{trimmed}{off}")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_for_field_nanoid_respects_length_and_prefix() {
        let cfg = FieldConfig {
            field_type: Some("nanoid".to_string()),
            length: Some(8),
            prefix: Some("doc-".to_string()),
            ..Default::default()
        };
        let id = generate_id_for_field(&cfg);
        assert!(id.starts_with("doc-"));
        assert_eq!(id.len(), 4 + 8);
    }

    #[test]
    fn generate_id_for_field_uuid_ignores_length() {
        let cfg = FieldConfig {
            field_type: Some("uuid".to_string()),
            ..Default::default()
        };
        let id = generate_id_for_field(&cfg);
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn is_valid_id_format_checks_prefix_and_length() {
        let cfg = FieldConfig {
            field_type: Some("nanoid".to_string()),
            length: Some(8),
            prefix: Some("doc-".to_string()),
            ..Default::default()
        };
        assert!(is_valid_id_format("doc-12345678", &cfg));
        assert!(!is_valid_id_format("invalid-slug", &cfg));
        assert!(!is_valid_id_format("doc-123", &cfg));
    }

    #[test]
    fn is_iso8601_recognizes_date_and_datetime() {
        assert!(is_iso8601("2026-06-17"));
        assert!(is_iso8601("2026-06-17T05:52:44"));
        assert!(!is_iso8601("not-a-date"));
        assert!(!is_iso8601("2026/06/17"));
    }

    #[test]
    fn format_rfc3339_appends_default_offset() {
        assert_eq!(
            format_rfc3339("2026-06-17T05:52:44", None),
            "2026-06-17T05:52:44Z"
        );
    }

    #[test]
    fn format_rfc3339_appends_explicit_offset() {
        assert_eq!(
            format_rfc3339("2026-06-17T05:52:44", Some("+09:00")),
            "2026-06-17T05:52:44+09:00"
        );
    }

    #[test]
    fn format_rfc3339_leaves_already_offset_values_unchanged() {
        assert_eq!(
            format_rfc3339("2026-06-17T05:52:44Z", None),
            "2026-06-17T05:52:44Z"
        );
        assert_eq!(
            format_rfc3339("2026-06-17T05:52:44+09:00", None),
            "2026-06-17T05:52:44+09:00"
        );
    }

    #[test]
    fn format_rfc3339_leaves_non_iso8601_values_unchanged() {
        assert_eq!(format_rfc3339("not-a-date", None), "not-a-date");
    }
}
