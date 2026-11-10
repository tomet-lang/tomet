//! Root configuration groups, one per concern a config declaration carries (`::format(...)`,
//! `::field(...)`, `::workspace(...)`, `::blueprint(...)`, `::vocabulary....()`, `::dictionary(...)`).
//!
//! Each submodule defines a dedicated strongly-typed configuration struct for a specific domain:
//! - (`format`) Paragraph, heading, link, element, callout, list, and table formatting rules ([`FormatConfig`], [`GroupOrder`], [`ParagraphSeparator`]).
//! - (`meta`) Document metadata formatting and schema rules ([`MetaConfig`], [`FieldConfig`]).
//! - (`workspace`) Vault file indexing exclusions and unswept rules ([`WorkspaceConfig`]).
//! - (`registry`) Blueprints, vocabularies and dictionaries declarations ([`RegistryConfig`]).

pub mod format;
pub mod meta;
pub mod registry;
pub mod workspace;

pub use format::{FormatConfig, GroupOrder, ParagraphSeparator};
pub use meta::{FieldConfig, MetaConfig};
pub use registry::RegistryConfig;
pub use workspace::WorkspaceConfig;
