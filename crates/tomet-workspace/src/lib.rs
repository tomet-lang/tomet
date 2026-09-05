//! Workspace-level batch refactoring, structural search, metadata editing, and I/O execution.
//!
//! Provides:
//! - **[`diff`]**: [`FileDiff`] model and disk persistence.
//! - **[`refactor`]**: Refactoring pipeline across `.tmt` files.
//! - **[`structural`]**: Structural search & replace engine across a workspace.
//! - **[`batch_meta`]**: Batch metadata update engine.
//! - **[`blueprint`]**: Blueprint discovery and document instantiation.

pub mod batch_meta;
pub mod blueprint;
pub mod diff;
pub mod refactor;
pub mod structural;

pub use batch_meta::*;
pub use blueprint::*;
pub use diff::*;
pub use refactor::*;
pub use structural::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_refactor_source_end_to_end() {
        let src = r#"@version(1.0)
@meta(format:yaml)+++
id: doc-test1234
type: task
created: 2026-04-12T18:00:00+09:00
+++
@config{
  macros: {
    youtube_ch: "https://www.youtube.com/${1}"
  }
}

- @link("https://www.youtube.com/@realakibaboyz")[REAL AKIBA BOYZ]
"#;

        let cfg = tomet_config::PrinterConfig::default();
        let opts = RefactorOptions::default();

        let (result, count) = refactor_source(src, &cfg, &opts).unwrap();
        assert!(count >= 2);

        assert!(result.contains("@kind(task)"));
        assert!(result.contains("@meta"));
        assert!(!result.contains("format:yaml"));
        assert!(!result.contains("type: task"));
        assert!(result.contains("$youtube_ch(\"@realakibaboyz\")"));
    }

    #[test]
    fn test_structural_apply_action() {
        let mut matches = vec![FileDiff::new(
            "test.tmt".into(),
            "@meta{author: Charlie}\n".into(),
            "@meta{author: Charlie}\n".into(),
            1,
        )];

        StructuralEngine::apply_action(
            &mut matches,
            &tomet_transform::StructuralAction::RenameKey {
                old_key: "author".into(),
                new_key: "creator".into(),
            },
        );

        assert!(matches[0].modified_src.contains("creator: Charlie"));
        assert!(matches[0].is_changed());
    }
}
