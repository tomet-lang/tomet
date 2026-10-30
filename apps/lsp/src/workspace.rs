//! The vaults the open documents belong to, and the diagnostics that need one.
//!
//! [`diagnostics_for`](crate::diagnostics_for) reads a document from its text
//! alone: what the parser and the CST validator can say. What `tomet check`
//! says needs more -- the vocabularies in scope, the blueprint the `::kind`
//! names, the settings' field types, the files an `include` reads -- and all
//! of that belongs to a vault. [`Workspace`] finds the vault of each open
//! document the way the CLI does, from the file's own path, and asks it for
//! the same findings `tomet check` reports.

use std::collections::{HashMap, HashSet};
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
    /// What a vault said was wrong with its own setup (a declared vocabulary
    /// that is not there) the last time it was built, by vault root. A vault
    /// is built again on every save, and the same complaint should not be made
    /// again each time it is.
    reported: HashMap<PathBuf, HashSet<String>>,
    /// Complaints not yet taken: see [`Workspace::take_notices`].
    notices: Vec<String>,
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

    /// Hover information at `pos` in the document open at `path`.
    ///
    /// An expression comes to what it comes to when the document is rendered,
    /// in its vault: it can ask about other documents (`tmt(...)`, `ref(...)`,
    /// `vault.docs`), and about an open buffer as the editor has it. Nothing
    /// open at `path` gives nothing.
    pub fn hover(&mut self, path: &Path, pos: lsp_types::Position) -> Option<lsp_types::Hover> {
        let text = self.buffers.get(path)?.clone();
        let vault = self.vault_for(path);
        crate::hover_in(&text, pos, &|doc, expr| vault.evaluate(doc, path, expr))
    }

    /// Where the reference at `pos` in the document open at `path` is defined.
    ///
    /// A reference to a node of the same document, and one into another --
    /// `@link(tmt:path#x)`, `@link(ref:Name)`, `${tmt("p")#x}`, a bare
    /// `tmt("p")` (the document's start). The target is read as the editor has
    /// it if it is open. Nothing open at `path`, or nothing to follow, gives
    /// nothing.
    pub fn definition(
        &mut self,
        path: &Path,
        pos: lsp_types::Position,
    ) -> Option<lsp_types::GotoDefinitionResponse> {
        let text = self.buffers.get(path)?.clone();
        self.vault_for(path);
        let vault = &self.vaults[&vault_root(path)];
        crate::vault_definition::definition(vault, &self.buffers, path, &text, pos)
    }

    /// The completions at `pos` in the document open at `path`: what the text
    /// alone offers, and what its vault adds -- the elements of the
    /// vocabularies in scope, the ids of a document, the documents there are.
    /// Nothing open at `path` gives nothing.
    pub fn completions(
        &mut self,
        path: &Path,
        pos: lsp_types::Position,
        uri: Option<&lsp_types::Uri>,
    ) -> Vec<lsp_types::CompletionItem> {
        let Some(text) = self.buffers.get(path).cloned() else {
            return Vec::new();
        };
        let vault = self.vault_for(path);
        crate::vault_completion::completions(vault, &text, pos, uri)
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
            let now: HashSet<String> = vault.vocabulary_errors.iter().cloned().collect();
            let before = self.reported.remove(&root).unwrap_or_default();
            for problem in &vault.vocabulary_errors {
                if !before.contains(problem) {
                    self.notices
                        .push(format!("{problem} (vault: {})", vault.root.display()));
                }
            }
            self.reported.insert(root.clone(), now);
            self.vaults.insert(root.clone(), vault);
        }
        &self.vaults[&root]
    }

    /// What the vaults found wrong with their own setup since this was last
    /// asked, once each: a vocabulary the config declares that does not exist,
    /// or that does not name itself. Not about any document, so there is no
    /// place in one to put it; the caller says it where the client keeps its
    /// log.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
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
            message: u.message(),
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
        fs::write(root.join(tomet_load_config::VAULT_CONFIG_FILE), config).unwrap();
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
            "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n",
            &[],
        );
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "::meta{ id: not-a-uuid }\n".to_string());

        let diags = ws.findings_diagnostics(&path);
        assert_eq!(diags.len(), 1, "{:?}", messages(&diags));
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diags[0].message.contains("not a valid uuid"));

        ws.set_buffer(
            path.clone(),
            "::meta{ id: 123e4567-e89b-42d3-a456-426614174000 }\n".to_string(),
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
            "::kind(config)\n::config{ vocabularies: list(\".tomet/vocabularies/deck.vocabulary.tmt\") }\n",
            &[(
                ".tomet/vocabularies/deck.vocabulary.tmt",
                "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{ card: {} }\n",
            )],
        );
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "::kind(deck)\n\n@card{}\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());

        ws.set_buffer(path.clone(), "::kind(deck)\n\n@nonesuch{}\n".to_string());
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
        let root = vault("dup", "::kind(config)\n", &[]);
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
            "::kind(config)\n",
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
        let root = vault("late", "::kind(config)\n", &[("b.tmt", "Mine: ${nope}.\n")]);
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
        let root = vault("config", "::kind(config)\n", &[]);
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), "::meta{ id: not-a-uuid }\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n",
        )
        .unwrap();
        ws.files_changed();
        assert_eq!(ws.findings_diagnostics(&path).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_open_and_text_that_does_not_parse_give_nothing() {
        let root = vault("none", "::kind(config)\n", &[]);
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        assert!(ws.findings_diagnostics(&path).is_empty());
        ws.set_buffer(path.clone(), "@caution[ unterminated\n".to_string());
        assert!(ws.findings_diagnostics(&path).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// A vocabulary the config declares and that is not there is the vault's
    /// own problem, not a document's. It is said once, however many times the
    /// vault is built again, and said again if it is fixed and then broken.
    #[test]
    fn a_vault_complaint_is_made_once_until_it_is_fixed_and_broken_again() {
        let root = vault(
            "notices",
            "::kind(config)\n::config{ vocabularies: list(\".tomet/vocabularies/missing.vocabulary.tmt\") }\n",
            &[],
        );
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        assert!(ws.take_notices().is_empty());

        ws.set_buffer(path.clone(), "Text.\n".to_string());
        let notices = ws.take_notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains("missing.vocabulary.tmt"), "{notices:?}");
        assert!(ws.take_notices().is_empty(), "taken, so not repeated");

        // Saved: the vault is built again, and the problem is still there.
        ws.files_changed();
        ws.findings_diagnostics(&path);
        assert!(ws.take_notices().is_empty());

        // The vocabulary is written: nothing to say.
        let declared = root.join(".tomet/vocabularies/missing.vocabulary.tmt");
        fs::write(
            &declared,
            "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{ tile: {} }\n",
        )
        .unwrap();
        ws.files_changed();
        ws.findings_diagnostics(&path);
        assert!(ws.take_notices().is_empty());

        // Removed again: said again.
        fs::remove_file(&declared).unwrap();
        ws.files_changed();
        ws.findings_diagnostics(&path);
        assert_eq!(ws.take_notices().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    /// The text of a hover, whatever its shape.
    fn hover_text(hover: Option<lsp_types::Hover>) -> String {
        match hover.expect("a hover").contents {
            lsp_types::HoverContents::Markup(markup) => markup.value,
            other => panic!("unexpected hover contents: {other:?}"),
        }
    }

    /// An expression on hover comes to what rendering would give it, in the
    /// vault: it can ask about another document, and about that document as
    /// the editor has it open.
    #[test]
    fn hover_evaluates_an_expression_in_the_vault() {
        let root = vault(
            "hover",
            "::kind(config)\n::macros{ gh: \"https://github.com/o/r/issues/${1}\" }\n",
            &[("b.tmt", "::meta{ title: Saved }\n")],
        );
        let (a, b) = (root.join("a.tmt"), root.join("b.tmt"));
        let mut ws = Workspace::new();
        ws.set_buffer(
            a.clone(),
            "${tmt(\"b.tmt\").meta.title}\n\n${macro.gh(7)}\n".to_string(),
        );
        let at = |line| lsp_types::Position::new(line, 4);

        // Another document, which the text alone could not know about.
        assert!(hover_text(ws.hover(&a, at(0))).contains("Saved"));
        // The vault's macro.
        assert!(hover_text(ws.hover(&a, at(2))).contains("https://github.com/o/r/issues/7"));

        // `b.tmt` is edited in the editor and not saved.
        ws.set_buffer(b.clone(), "::meta{ title: Edited }\n".to_string());
        assert!(hover_text(ws.hover(&a, at(0))).contains("Edited"));
        let _ = fs::remove_dir_all(&root);
    }

    /// What exists only when a document is created is not there when it is
    /// rendered, so it is not on hover either.
    #[test]
    fn hover_does_not_know_what_rendering_does_not() {
        let root = vault("hover_create", "::kind(config)\n", &[]);
        let a = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(a.clone(), "${generate.uuid()}\n".to_string());
        let text = hover_text(ws.hover(&a, lsp_types::Position::new(0, 4)));
        assert!(text.contains("Error"), "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn hover_on_nothing_open_is_nothing() {
        let root = vault("hover_none", "::kind(config)\n", &[]);
        let mut ws = Workspace::new();
        assert!(
            ws.hover(&root.join("a.tmt"), lsp_types::Position::new(0, 0))
                .is_none()
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn labels(items: &[lsp_types::CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    /// `text` is open at `a.tmt` and the cursor is at the end of its last line.
    fn completions_at_the_end(root: &Path, text: &str) -> Vec<lsp_types::CompletionItem> {
        let path = root.join("a.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(path.clone(), text.to_string());
        let line = text.lines().count().saturating_sub(1) as u32;
        let character = text.lines().last().map_or(0, |l| l.encode_utf16().count()) as u32;
        ws.completions(&path, lsp_types::Position::new(line, character), None)
    }

    const DECK: &str = "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{\n  tile: {\n    display: block\n    args: {\n      label: {\n        type: string\n        positional: true\n        about: | The label.\n      }\n    }\n    about: | One tile.\n  }\n}\n";

    /// After `@`, the elements of the vocabulary the document's `::kind` binds
    /// are offered with the built-in ones, which the text alone cannot know.
    #[test]
    fn at_offers_the_elements_of_the_vocabulary_in_scope() {
        let root = vault(
            "at",
            "::kind(config)\n::config{ vocabularies: list(\".tomet/vocabularies/deck.vocabulary.tmt\") }\n",
            &[(".tomet/vocabularies/deck.vocabulary.tmt", DECK)],
        );
        let in_deck = completions_at_the_end(&root, "::kind(deck)\n\n@");
        let names = labels(&in_deck);
        assert!(names.contains(&"tile"), "{names:?}");
        assert!(names.contains(&"link"), "a built-in is still offered");
        assert!(
            in_deck
                .iter()
                .find(|i| i.label == "tile")
                .and_then(|i| i.detail.as_deref())
                .is_some_and(|d| d.contains("label")),
            "the element's parameters are in its detail"
        );

        // A document that does not bind the vocabulary does not get it.
        let elsewhere = completions_at_the_end(&root, "::kind(other)\n\n@");
        assert!(!labels(&elsewhere).contains(&"tile"));
        let _ = fs::remove_dir_all(&root);
    }

    /// After a `#` that writes a reference, the ids of the document -- read
    /// even though the line being typed does not parse.
    #[test]
    fn a_reference_is_offered_the_ids_of_the_document() {
        let root = vault("ids", "::kind(config)\n", &[]);
        for typing in ["${#", "@link(#"] {
            let items = completions_at_the_end(
                &root,
                &format!("=[ A ]#(alpha)\n\n=[ B ]#(beta)\n\n{typing}"),
            );
            assert_eq!(labels(&items), ["alpha", "beta"], "{typing}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// A `#` that declares an id is not a reference.
    #[test]
    fn a_declaration_is_not_offered_ids() {
        let root = vault("decl", "::kind(config)\n", &[]);
        let items = completions_at_the_end(&root, "=[ A ]#(alpha)\n\n=[ B ]#(");
        assert!(!labels(&items).contains(&"alpha"));
        let _ = fs::remove_dir_all(&root);
    }

    /// After the document on the left of the `#`, the ids of that document.
    #[test]
    fn a_reference_into_another_document_is_offered_its_ids() {
        let root = vault(
            "other_ids",
            "::kind(config)\n",
            &[("b.tmt", "=[ One ]#(one)\n\n=[ Two ]#(two)\n")],
        );
        for typing in ["${tmt(\"b.tmt\")#", "@link(tmt:b.tmt#"] {
            let items = completions_at_the_end(&root, &format!("=[ Mine ]#(mine)\n\n{typing}"));
            assert_eq!(labels(&items), ["one", "two"], "{typing}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// In the path of a document, the documents of the vault -- measured from
    /// its root, the way the language measures them.
    #[test]
    fn a_document_path_is_offered_the_documents_of_the_vault() {
        let root = vault("paths", "::kind(config)\n", &[("b.tmt", "text\n")]);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/c.tmt"), "text\n").unwrap();
        for typing in ["${tmt(\"", "@link(tmt:"] {
            let items = completions_at_the_end(&root, typing);
            let names = labels(&items);
            assert!(names.contains(&"b.tmt"), "{typing}: {names:?}");
            assert!(names.contains(&"sub/c.tmt"), "{typing}: {names:?}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn completion_on_nothing_open_is_nothing() {
        let root = vault("complete_none", "::kind(config)\n", &[]);
        let mut ws = Workspace::new();
        let items = ws.completions(&root.join("a.tmt"), lsp_types::Position::new(0, 0), None);
        assert!(items.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    /// Where go-to-definition lands for the reference at `(line, col)` of the
    /// document open at `a.tmt`: the file's name and the line.
    fn jump(ws: &mut Workspace, root: &Path, line: u32, col: u32) -> Option<(String, u32)> {
        let a = root.join("a.tmt");
        match ws.definition(&a, lsp_types::Position::new(line, col))? {
            lsp_types::GotoDefinitionResponse::Scalar(location) => {
                let path = crate::uri_to_file_path(&location.uri).expect("a file");
                Some((
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    location.range.start.line,
                ))
            }
            other => panic!("expected one location, got {other:?}"),
        }
    }

    /// A reference into another document lands on the node in that file, and
    /// follows the file as the editor has it open.
    #[test]
    fn a_reference_into_another_document_jumps_to_its_node() {
        let root = vault(
            "jump_id",
            "::kind(config)\n",
            &[("b.tmt", "=[ One ]#(one)\n\ntext\n\n=[ Two ]#(two)\n")],
        );
        let b = root.join("b.tmt");
        let mut ws = Workspace::new();
        ws.set_buffer(
            root.join("a.tmt"),
            "@link(tmt:b.tmt#two)\n\n${tmt(\"b.tmt\")#one}\n".to_string(),
        );

        assert_eq!(jump(&mut ws, &root, 0, 14), Some(("b.tmt".into(), 4)));
        assert_eq!(jump(&mut ws, &root, 2, 17), Some(("b.tmt".into(), 0)));

        // `b.tmt` is edited and not saved: a line is added above the node.
        ws.set_buffer(
            b,
            "=[ Zero ]#(zero)\n\n=[ One ]#(one)\n\ntext\n\n=[ Two ]#(two)\n".to_string(),
        );
        assert_eq!(jump(&mut ws, &root, 0, 14), Some(("b.tmt".into(), 6)));
        let _ = fs::remove_dir_all(&root);
    }

    /// A reference to a document, with no node named, lands at its start.
    #[test]
    fn a_reference_to_a_document_jumps_to_its_start() {
        let root = vault("jump_doc", "::kind(config)\n", &[("b.tmt", "text\n")]);
        let mut ws = Workspace::new();
        ws.set_buffer(
            root.join("a.tmt"),
            "@link(tmt:b.tmt)\n\n${include(tmt(\"b.tmt\"))}\n".to_string(),
        );
        assert_eq!(jump(&mut ws, &root, 0, 10), Some(("b.tmt".into(), 0)));
        assert_eq!(jump(&mut ws, &root, 2, 14), Some(("b.tmt".into(), 0)));
        let _ = fs::remove_dir_all(&root);
    }

    /// `./` and `../` are measured from the document the link is in.
    #[test]
    fn a_relative_path_is_measured_from_the_document() {
        let root = vault("jump_rel", "::kind(config)\n", &[]);
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/near.tmt"), "=[ N ]#(n)\n").unwrap();
        fs::write(root.join("far.tmt"), "=[ F ]#(f)\n").unwrap();
        let mut ws = Workspace::new();
        let inside = root.join("notes/a.tmt");
        ws.set_buffer(
            inside.clone(),
            "@link(tmt:./near.tmt#n)\n\n@link(tmt:../far.tmt#f)\n".to_string(),
        );
        let at = |ws: &mut Workspace, line| match ws
            .definition(&inside, lsp_types::Position::new(line, 14))
            .expect("a jump")
        {
            lsp_types::GotoDefinitionResponse::Scalar(l) => crate::uri_to_file_path(&l.uri)
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            other => panic!("{other:?}"),
        };
        assert_eq!(at(&mut ws, 0), "near.tmt");
        assert_eq!(at(&mut ws, 2), "far.tmt");
        let _ = fs::remove_dir_all(&root);
    }

    /// A note, by the name it is known by.
    #[test]
    fn a_note_reference_jumps_to_the_note() {
        let root = vault("jump_note", "::kind(config)\n", &[("Weekly.tmt", "text\n")]);
        let mut ws = Workspace::new();
        ws.set_buffer(root.join("a.tmt"), "@link(ref:Weekly)\n".to_string());
        assert_eq!(jump(&mut ws, &root, 0, 12), Some(("Weekly.tmt".into(), 0)));
        let _ = fs::remove_dir_all(&root);
    }

    /// A reference to what is not there does not jump.
    #[test]
    fn a_reference_to_nothing_does_not_jump() {
        let root = vault(
            "jump_none",
            "::kind(config)\n",
            &[("b.tmt", "=[ One ]#(one)\n")],
        );
        let mut ws = Workspace::new();
        ws.set_buffer(
            root.join("a.tmt"),
            "@link(tmt:missing.tmt#x)\n\n@link(tmt:b.tmt#nope)\n\n${#nope}\n".to_string(),
        );
        assert_eq!(jump(&mut ws, &root, 0, 14), None, "no such document");
        assert_eq!(jump(&mut ws, &root, 2, 14), None, "no such id in it");
        assert_eq!(jump(&mut ws, &root, 4, 3), None, "no such id here");
        let _ = fs::remove_dir_all(&root);
    }

    /// The same-document jump is answered here too, in the file's own URI.
    #[test]
    fn a_reference_within_the_document_still_jumps() {
        let root = vault("jump_self", "::kind(config)\n", &[]);
        let mut ws = Workspace::new();
        ws.set_buffer(
            root.join("a.tmt"),
            "=[ Intro ]#(intro)\n\n${#intro}\n".to_string(),
        );
        assert_eq!(jump(&mut ws, &root, 2, 3), Some(("a.tmt".into(), 0)));
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
