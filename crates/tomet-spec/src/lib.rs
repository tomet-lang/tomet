//! =[ tomet-spec ]
//!
//! Automated `.tmt` specification and reference generation for CLI commands (`clap`)
//! and configuration schemas (`#[derive(Config)]`).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Single-Crate Consumer Surface)
//!   | Re-exports [`Config`] (`#[derive(Config)]`), [`ConfigSpecProvider`],
//!   | [`ConfigGroupSpec`], and [`ConfigEntrySpec`] so external repositories
//!   | can define configuration schemas and render `.tmt` specifications by
//!   | depending only on `tomet-spec`.
//! - (Pure In-Memory AST & `.tmt` Generators)
//!   - ([`cli`]) Converts `clap::Command` introspection trees into `.tmt` [`Document`](tomet_ast::Document) ASTs and formatted strings.
//!   - ([`config`]) Converts [`ConfigGroupSpec`] slices into `.tmt` [`Document`](tomet_ast::Document) ASTs and formatted strings.
//!   - ([`util`]) Shared inline code escaping and AST construction utilities.

extern crate self as tomet_spec;

pub mod cli;
pub mod config;
pub mod util;

pub use tomet_config::PrinterConfig;
pub use tomet_config::schema::{self, ConfigEntrySpec, ConfigGroupSpec, ConfigSpecProvider};
pub use tomet_spec_derive::Config;

#[doc(hidden)]
pub mod __private {
    pub use tomet_ast::Value;
    pub use tomet_config::tree::{collect_paths, get_path};
    pub use tomet_tree::ValueExt;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example external tool configuration group.
    #[derive(Debug, Clone, Default, PartialEq, Config)]
    #[config(group = "catasto")]
    struct SampleToolConfig {
        /// Strict lockfile verification mode.
        #[config(key = "catasto.strict", default = true)]
        pub strict: bool,

        /// Excluded directory patterns.
        #[config(key = "catasto.ignore", alias = "ignore", type = "list(string)")]
        pub ignore: Vec<String>,
    }

    #[test]
    fn derive_and_render_config_tmt_roundtrip() {
        let spec = SampleToolConfig::spec();
        assert_eq!(spec.name, "catasto");
        assert_eq!(spec.entries.len(), 2);

        let rendered = config::render(
            &[spec],
            "doc-sampleCfg",
            "Sample Configuration Reference",
            &["Generated from `SampleToolConfig`."],
            &PrinterConfig::default(),
        );

        let parsed = tomet_parser::parse_document(&rendered).expect("valid .tmt output");
        assert!(!parsed.blocks.is_empty());
        assert!(rendered.contains("==[catasto]") || rendered.contains("==[ catasto ]"));
        assert!(rendered.contains("`catasto.strict` (bool, default: `true`)"));
        assert!(rendered.contains("Aliases: `ignore`."));
    }
}
