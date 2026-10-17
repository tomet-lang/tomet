//! Go to definition across documents: the references that point out of the
//! document they are written in.
//!
//! The text alone can follow a reference to a node of its own document
//! ([`definition_for`](crate::definition_for)). A reference to another one --
//! `@link(tmt:path#x)`, `@link(ref:Name)`, `${tmt("p")#x}` -- needs the vault
//! to say which document that is, and the target's text to say where in it.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};
use tomet_expr::{DocRef, EvalValue, Resolver};
use tomet_load::Vault;

use crate::definition::{Reference, Target, find_def_in_blocks, reference_at};
use crate::position::TextPositions;

/// Where the reference at `pos` in `text`, a document of `vault` at `path`,
/// is defined.
///
/// `buffers` is what the editor holds open: the text of a document that is
/// open, rather than the disk's, so a position is a position in what the
/// person sees.
pub(crate) fn definition(
    vault: &Vault,
    buffers: &HashMap<PathBuf, String>,
    path: &Path,
    text: &str,
    pos: Position,
) -> Option<GotoDefinitionResponse> {
    let (doc, _) = vault.parse(text).ok()?;
    let positions = TextPositions::new(text);
    let reference = reference_at(&doc, positions.offset(pos))?;

    let (target_path, range) = match reference {
        Reference::Id(id) => {
            let span = find_def_in_blocks(&doc.blocks, &id)?;
            (path.to_path_buf(), positions.range(&span))
        }
        Reference::In { document, id } => {
            let target = match document {
                Target::Path(written) => resolve_path(vault, path, &written),
                Target::Note(name) => vault.resolver().note(&name).ok()?,
                Target::Expression(expr) => match vault.evaluate(&doc, path, &expr).ok()? {
                    EvalValue::Document(document) => document,
                    _ => return None,
                },
            };
            let target_doc = vault.resolver().document(&target).ok()?;
            let target_path = vault.root.join(&target.0);
            let range = match id {
                // The document itself: where it begins.
                None => Range::default(),
                Some(id) => {
                    let target_text = match buffers.get(&target_path) {
                        Some(open) => open.clone(),
                        None => std::fs::read_to_string(&target_path).ok()?,
                    };
                    let span = find_def_in_blocks(&target_doc.blocks, &id)?;
                    TextPositions::new(&target_text).range(&span)
                }
            };
            (target_path, range)
        }
    };

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: file_uri(&target_path)?,
        range,
    }))
}

/// The document a written `tmt:` path names: `./` and `../` are measured from
/// the document the link is written in, anything else from the vault's root --
/// the same rule a link follows when it is checked.
fn resolve_path(vault: &Vault, from: &Path, written: &str) -> DocRef {
    let written = written.trim();
    if !(written.starts_with("./") || written.starts_with("../")) {
        return DocRef(written.to_string());
    }
    let dir = from
        .parent()
        .and_then(|dir| dir.strip_prefix(&vault.root).ok())
        .unwrap_or(Path::new(""));
    let mut parts: Vec<String> = Vec::new();
    for component in dir.join(written).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    DocRef(parts.join("/"))
}

/// `file://` and the path, with what a URI cannot hold written as `%XX`.
pub(crate) fn file_uri(path: &Path) -> Option<Uri> {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out.parse().ok()
}
