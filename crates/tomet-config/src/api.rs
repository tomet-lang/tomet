//! API extraction configuration ([`ApiConfig`]).

use tomet_ast::Value;
use tomet_tree::ValueExt;

use crate::tree::get_path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApiConfig {
    /// `api.rust.out` -- destination directory for extracted Rust API .tmt documents.
    pub rust_out: Option<String>,
}

impl ApiConfig {
    /// Extracts API configuration from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut cfg = Self::default();

        if let Some(s) = get_path(root, &["api", "rust", "out"]).and_then(|v| v.as_str()) {
            cfg.rust_out = Some(s.to_string());
        }

        cfg
    }
}
