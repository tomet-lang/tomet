//! `serde` support for Tomet's data subset (`tomet_ast::Value`):
//! plain `key: value`, nested `{ }`/`[ ]`, and scalars -- the part of the
//! grammar with a direct mapping to Rust structs, the same role
//! `serde_yaml`/`serde_json` play for their formats.
//!
//! Headings, prose, and links (the rest of `tomet_ast::Document`) have
//! no serde equivalent and aren't handled here; `to_string`/`from_str`
//! only ever read or write a data-only `.tmt` document.

mod de;
mod error;
mod print;
mod ser;

pub use error::{Error, Result};
pub use tomet_ast::Value;

/// Parse a data-only `.tmt` document into `T`.
pub fn from_str<T: for<'de> serde::Deserialize<'de>>(src: &str) -> Result<T> {
    let value = tomet_parser::parse_value(src)?;
    de::from_value(value)
}

/// Render `T` as a data-only `.tmt` document.
pub fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    let v = ser::to_value(value)?;
    Ok(print::print_value(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Settings {
        style: Option<String>,
        status_bar_visible: Option<bool>,
        refresh_ms: Option<u32>,
        font_size: Option<f64>,
        bookmarks: Vec<Bookmark>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Bookmark {
        label: String,
        path: String,
    }

    #[test]
    fn round_trips_a_struct() {
        let settings = Settings {
            style: Some("Fusion".into()),
            status_bar_visible: Some(true),
            refresh_ms: Some(500),
            font_size: Some(10.5),
            bookmarks: vec![
                Bookmark {
                    label: "Inbox".into(),
                    path: "notes/inbox".into(),
                },
                Bookmark {
                    label: "日誌".into(),
                    path: "notes/journal".into(),
                },
            ],
        };

        let text = to_string(&settings).unwrap();
        let round_tripped: Settings = from_str(&text).unwrap();
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn missing_optional_fields_deserialize_to_none() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Small {
            a: Option<String>,
            b: i32,
        }
        let parsed: Small = from_str("b: 7").unwrap();
        assert_eq!(parsed, Small { a: None, b: 7 });
    }

    #[test]
    fn enum_round_trips_as_unit_or_newtype() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        enum Effect {
            None,
            Blur(u32),
        }
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Wrapper {
            effect: Effect,
        }

        let a = Wrapper {
            effect: Effect::None,
        };
        let b = Wrapper {
            effect: Effect::Blur(4),
        };
        assert_eq!(from_str::<Wrapper>(&to_string(&a).unwrap()).unwrap(), a);
        assert_eq!(from_str::<Wrapper>(&to_string(&b).unwrap()).unwrap(), b);
    }
}
