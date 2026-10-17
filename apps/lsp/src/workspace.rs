//! The vaults the open documents belong to, and the diagnostics that need one.
//!
//! [`diagnostics_for`](crate::diagnostics_for) reads a document from its text
//! alone: what the parser and the CST validator can say. What `tomet check`
//! says needs more -- the vocabularies in scope, the blueprint the `@kind`
//! names, the settings' field types, the files an `include` reads -- and all
//! of that belongs to a vault. [`Workspace`] finds the vault of each open
//! document the way the CLI does, from the file's own path, and asks it for
//! the same findings `tomet check` reports.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lsp_types::{Diagnostic, DiagnosticSeverity};
use tomet_load::{Findings, Vault, is_error};

use crate::position::TextPositions;

/// The open documents, and the vaults they are in.
#[derive(Default)]
pub struct Workspace {
    /// By the directory the config file sits in.
    vaults: HashMap<PathBuf, Vault>,
    /// What the editor holds open, saved or not.
    buffers: HashMap<PathBuf, String>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    /// `path` is open with `text`, or its text changed. Anything in the vault
    /// that reads that file from here on reads this text.
    pub fn set_buffer(&mut self, path: PathBuf, text: String) {
        self.vault_for(&path).set_buffer(&path, &text);
        self.buffers.insert(path, text);
    }

    /// `path` was closed: its file is the disk's again.
    pub fn close(&mut self, path: &Path) {
        self.buffers.remove(path);
        if let Some(vault) = self.vaults.get(&vault_root(path)) {
            vault.remove_buffer(path);
        }
    }

    /// Files changed on the disk: a config, a vocabulary, a blueprint, or any
    /// document. The vaults are dropped and found again on the next ask, so
    /// nothing keeps a vocabulary or a document read before the change. That
    /// costs the vocabularies being read again, which is small.
    pub fn files_changed(&mut self) {
        self.vaults.clear();
    }

    /// The text held open for `path`, if any.
    pub fn buffer(&self, path: &Path) -> Option<&str> {
        self.buffers.get(path).map(String::as_str)
    }

    /// The paths held open.
    pub fn open_paths(&self) -> Vec<PathBuf> {
        self.buffers.keys().cloned().collect()
    }

    /// What `tomet check` reports for the document open at `path`, as LSP
    /// diagnostics -- not the ones [`diagnostics_for`](crate::diagnostics_for)
    /// gives from the text alone, which the caller adds.
    ///
    /// Empty if nothing is open at `path` or the text does not parse (the
    /// parse error is the other kind's to report).
    pub fn findings_diagnostics(&mut self, path: &Path) -> Vec<Diagnostic> {
        let Some(text) = self.buffers.get(path).cloned() else {
            return Vec::new();
        };
        let vault = self.vault_for(path);
        let Ok((doc, _)) = vault.parse(&text) else {
            return Vec::new();
        };
        to_diagnostics(&text, &vault.findings(path, &doc))
    }

    /// The vault governing `path`, found from the path as the CLI finds it
    /// and kept. The documents open under it are told to it as buffers.
    fn vault_for(&mut self, path: &Path) -> &Vault {
        let root = vault_root(path);
        if !self.vaults.contains_key(&root) {
            let vault = Vault::discover(path);
            for (open, text) in &self.buffers {
                if open.starts_with(&vault.root) {
                    vault.set_buffer(open, text);
                }
            }
            self.vaults.insert(root.clone(), vault);
        }
        &self.vaults[&root]
    }
}

/// The directory of the config that governs `path`; with none, the file's own.
fn vault_root(path: &Path) -> PathBuf {
    tomet_load_config::find_config_file(path)
        .map(|(_, _, root)| root)
        .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).to_path_buf())
}

/// `findings` of the document with text `text`, as LSP diagnostics.
///
/// The validator's duplicate-id finding is left out: the CST validator
/// reports the same thing from `diagnostics_for`, on the exact tokens.
pub fn to_diagnostics(text: &str, findings: &Findings) -> Vec<Diagnostic> {
    let positions = TextPositions::new(text);
    let mut out = Vec::new();
    for d in &findings.diagnostics {
        if matches!(d, tomet_validator::Diagnostic::DuplicateId { .. }) {
            continue;
        }
        out.push(Diagnostic {
            range: positions.range(&d.span()),
            severity: Some(if is_error(d) {
                DiagnosticSeverity::ERROR
            } else {
                DiagnosticSeverity::WARNING
            }),
            source: Some("tomet".to_string()),
            message: d.message_in(positions.index()),
            ..Diagnostic::default()
        });
    }
    for u in &findings.unresolved {
        out.push(Diagnostic {
            range: positions.range(&u.span),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("tomet".to_string()),
            message: format!("unresolved ${{{}}} -- {}", u.source, u.reason),
            ..Diagnostic::default()
        });
    }
    out
}

/// Waits until things have stopped changing: each [`touch`](Debounce::touch)
/// pushes the moment back, and [`due`](Debounce::due) says when it has come.
///
/// What a vault is asked for on every keystroke would be wasted work for a
/// document being typed into, so it is done once the typing pauses.
pub struct Debounce {
    delay: Duration,
    deadline: Option<Instant>,
}

impl Debounce {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            deadline: None,
        }
    }

    /// Something changed at `now`.
    pub fn touch(&mut self, now: Instant) {
        self.deadline = Some(now + self.delay);
    }

    /// How long to wait for the next message before the work is due, or
    /// `None` if there is nothing waiting (wait for a message as long as it
    /// takes).
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(now))
    }

    /// Whether the work is due at `now`. Taking it clears it.
    pub fn due(&mut self, now: Instant) -> bool {
        match self.deadline {
            Some(d) if d <= now => {
                self.deadline = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A vault in a temp directory: a config, and the files given.
    fn vault(name: &str, config: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tomet_lsp_ws_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".tomet/vocabularies")).unwrap();
        fs::write(root.join("default.config.tmt"), config).unwrap();
        for (path, source) in files {
            fs::write(root.join(path), source).unwrap();
        }
        root
    }

    fn messages(diags: &[Diagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.message.as_str()).collect()
    }

    #[test]
    fn a_field_type_the_settings_declare_is_checked() {
        let root = vault(
            "types",
            "@kind(config)\n@config{ meta: { id: { type: uuid } } }\n",
            &[],
        );
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "@meta{ id: not-a-uuid }\n".to_string());

        let diags = ws.findings_diagnostics(&path);
        assert_eq!(diags.len(), 1, "{:?}", messages(&diags));
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diags[0].message.contains("not a valid uuid"));

        ws.set_buffer(
            path.clone(),
            "@meta{ id: 123e4567-e89b-42d3-a456-426614174000 }\n".to_string(),
        );
        assert!(ws.findings_diagnostics(&path).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// The vocabulary the vault declares is in scope, which the text alone
    /// cannot know.
    #[test]
    fn the_vaults_vocabulary_is_in_scope() {
        let root = vault(
            "vocab",
            "@kind(config)\n@config{ vocabularies: list(\".tomet/vocabularies/deck.vocabulary.tmt\") }\n",
            &[(
                ".tomet/vocabularies/deck.vocabulary.tmt",
                "@kind(vocabulary)\n@vocabulary(deck)\n\n@element(card){}[ One card. ]\n",
            )],
        );
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "@kind(deck)\n\n@card{}\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());

        ws.set_buffer(path.clone(), "@kind(deck)\n\n@nonesuch{}\n".to_string());
        let diags = ws.findings_diagnostics(&path);
        assert_eq!(diags.len(), 1, "{:?}", messages(&diags));
        assert!(diags[0].message.contains("unknown element"));
        let _ = fs::remove_dir_all(&root);
    }

    /// The CST validator reports a duplicate id as an error on its exact
    /// tokens, so the validator's error for the same thing is not added to it.
    /// What is left is `tomet check`'s other note about it, that the later id
    /// was dropped -- a warning, as it is there.
    #[test]
    fn a_duplicate_id_error_is_not_reported_twice() {
        let root = vault("dup", "@kind(config)\n", &[]);
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "=[ One ]#(a)\n=[ Two ]#(a)\n".to_string());
        let diags = ws.findings_diagnostics(&path);
        assert!(
            diags
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::WARNING)),
            "{:?}",
            messages(&diags)
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// An include says it brought in something unresolved, and it follows the
    /// included file's unsaved text.
    #[test]
    fn an_include_follows_the_included_files_unsaved_text() {
        let root = vault(
            "include",
            "@kind(config)\n",
            &[("b.tmt", "Mine: ${nope}.\n")],
        );
        let (a, b) = (root.join("a.tmt"), root.join("b.tmt"));
        let mut ws = Workspace::new();
        ws.set_buffer(a.clone(), "${include(tmt(\"b.tmt\"))}\n".to_string());

        let diags = ws.findings_diagnostics(&a);
        assert_eq!(diags.len(), 1, "{:?}", messages(&diags));
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(diags[0].message.contains("`b.tmt` has 1 unresolved item"));

        // `b.tmt` is fixed in the editor, not yet saved.
        ws.set_buffer(b.clone(), "Mine: fine.\n".to_string());
        assert!(ws.findings_diagnostics(&a).is_empty());

        // Closed without saving: the disk's `b.tmt` again.
        ws.close(&b);
        assert_eq!(ws.findings_diagnostics(&a).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    /// A buffer opened before its vault was built is told to it when it is.
    #[test]
    fn buffers_are_told_to_a_vault_built_after_them() {
        let root = vault("late", "@kind(config)\n", &[("b.tmt", "Mine: ${nope}.\n")]);
        let (a, b) = (root.join("a.tmt"), root.join("b.tmt"));
        let mut ws = Workspace::new();
        ws.set_buffer(b.clone(), "Mine: fine.\n".to_string());
        ws.set_buffer(a.clone(), "${include(tmt(\"b.tmt\"))}\n".to_string());
        ws.files_changed();
        assert!(ws.findings_diagnostics(&a).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_changed_config_is_read_after_files_changed() {
        let root = vault("config", "@kind(config)\n", &[]);
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "@meta{ id: not-a-uuid }\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());

        fs::write(
            root.join("default.config.tmt"),
            "@kind(config)\n@config{ meta: { id: { type: uuid } } }\n",
        )
        .unwrap();
        ws.files_changed();
        assert_eq!(ws.findings_diagnostics(&path).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_open_and_text_that_does_not_parse_give_nothing() {
        let root = vault("none", "@kind(config)\n", &[]);
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        assert!(ws.findings_diagnostics(&path).is_empty());
        ws.set_buffer(path.clone(), "@caution[ unterminated\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn debounce_waits_for_a_pause() {
        let start = Instant::now();
        let mut d = Debounce::new(Duration::from_millis(300));
        assert_eq!(d.wait(start), None);
        assert!(!d.due(start));

        d.touch(start);
        assert_eq!(d.wait(start), Some(Duration::from_millis(300)));
        assert!(!d.due(start + Duration::from_millis(299)));

        // Typing again pushes it back.
        d.touch(start + Duration::from_millis(200));
        assert!(!d.due(start + Duration::from_millis(300)));
        assert!(d.due(start + Duration::from_millis(500)));

        // Due once, then nothing waits.
        assert!(!d.due(start + Duration::from_millis(600)));
        assert_eq!(d.wait(start + Duration::from_millis(600)), None);
    }
}
