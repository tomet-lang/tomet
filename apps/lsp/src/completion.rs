use lsp_types::{CompletionItem, CompletionItemKind, Position, Uri};
use tomet_semantics::{BUILTIN_KINDS, ElementKind, Shape};

use crate::position::{get_line_prefix, uri_to_file_path};

/// Provides autocompletion items for elements, keywords, and builtin compute functions.
pub fn completions_for(text: &str, pos: Position) -> Vec<CompletionItem> {
    completions_for_with_uri(text, pos, None)
}

/// Whether a built-in is a declaration about the document (`::name`).
fn is_declaration(kind: &ElementKind) -> bool {
    tomet_semantics::builtin_region(kind) == tomet_semantics::Region::Preamble
}

/// Provides autocompletion items with document URI awareness for path completions.
pub fn completions_for_with_uri(
    text: &str,
    pos: Position,
    uri: Option<&Uri>,
) -> Vec<CompletionItem> {
    let prefix = get_line_prefix(text, pos);

    // If typing a path inside `file:`, `dir:`, or `./`, provide path completions.
    if let Some(path_items) = path_completions(prefix, uri) {
        return path_items;
    }

    let mut items = Vec::new();

    // Driven by `BUILTIN_KINDS` rather than a hand-kept list. The old
    // list had drifted badly -- it offered `callout`/`warning`/`connect`,
    // which are not built-in at all, and omitted `link`/`embed`/`table`.
    // Bare names are reserved for exactly this set now, so offering a
    // non-builtin one would suggest something that fails to validate.
    //
    // The spelling follows the element's `Region`, not its shape: a
    // declaration about the document (the ones that belong in the
    // preamble) is `::name`, anything else is `@name`. Where in a body an
    // element may be *placed* is reported in the completion detail instead.
    //
    // `::` opening a line starts a declaration: only the built-ins that are
    // declarations are offered. The sigil is typed already, so the name goes
    // in alone.
    if let Some(typed) = prefix.trim_start().strip_prefix("::")
        && typed
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        for (name, kind) in BUILTIN_KINDS.iter() {
            if !is_declaration(kind) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                insert_text: Some(name.to_string()),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail_for(name, kind)),
                ..CompletionItem::default()
            });
        }
        return items;
    }

    if prefix.ends_with('@') {
        for (name, kind) in BUILTIN_KINDS.iter() {
            if is_declaration(kind) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                insert_text: Some(name.to_string()),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail_for(name, kind)),
                ..CompletionItem::default()
            });
        }
        return items;
    }

    if prefix.ends_with("${") || prefix.ends_with("${ ") {
        // What may start an expression: the functions called by a bare name,
        // and the reserved values with the members worth offering first. Both
        // lists are the evaluator's own, so what is offered is what exists.
        for (func, detail) in tomet_expr::FUNCTIONS {
            items.push(CompletionItem {
                label: format!("{func}(...)"),
                insert_text: Some(format!("{func}()")),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.to_string()),
                ..CompletionItem::default()
            });
        }
        for (name, detail) in tomet_expr::RESERVED_NAMES {
            items.push(CompletionItem {
                label: name.to_string(),
                insert_text: Some(name.to_string()),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some(detail.to_string()),
                ..CompletionItem::default()
            });
        }
        return items;
    }

    // Unprefixed: offer every built-in under the sigil its region requires.
    for (name, kind) in BUILTIN_KINDS.iter() {
        let sigil = if is_declaration(kind) { "::" } else { "@" };
        items.push(CompletionItem {
            label: format!("{sigil}{name}"),
            insert_text: Some(format!("{sigil}{name}")),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail_for(name, kind)),
            ..CompletionItem::default()
        });
    }

    // The schemes an address can start with, from the one list the address
    // parser reads, so a scheme renamed there is renamed here. `unresolved`
    // is what the tooling writes for a reference that did not resolve, never
    // something to type.
    for scheme in tomet_address::EXPLICIT_SCHEMES {
        if scheme == "unresolved" {
            continue;
        }
        items.push(CompletionItem {
            label: scheme.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(scheme_detail(scheme).to_string()),
            ..CompletionItem::default()
        });
    }
    items.push(CompletionItem {
        label: "tag".to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some("Tag classification".to_string()),
        ..CompletionItem::default()
    });

    items
}

/// One line saying what an address scheme points at.
fn scheme_detail(scheme: &str) -> &'static str {
    match scheme {
        "tmt" => "Another Tomet document: `tmt:path/to/doc.tmt#id`",
        "id" => "An element of this document, by its id: `id:intro`",
        "ref" => "A note found by file name: `ref:Note Name`",
        "file" => "A file, by path: `file:path/to/file`",
        "dir" => "A directory, by path: `dir:path/to/dir`",
        "url" => "A web address: `url:https://example.com`",
        _ => "An address",
    }
}

fn path_completions(prefix: &str, uri: Option<&Uri>) -> Option<Vec<CompletionItem>> {
    let (raw_path, is_dir_only) = detect_path_context(prefix)?;

    let current_dir = uri
        .and_then(uri_to_file_path)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });

    let (search_dir, base_prefix) = if let Some(stripped) = raw_path.strip_prefix('/') {
        let root = find_project_root(&current_dir);
        if let Some(last_slash) = stripped.rfind('/') {
            let dir_part = &stripped[..=last_slash];
            let base = &stripped[last_slash + 1..];
            (root.join(dir_part), base.to_string())
        } else {
            (root, stripped.to_string())
        }
    } else if let Some(last_slash) = raw_path.rfind('/') {
        let dir_part = &raw_path[..=last_slash];
        let base = &raw_path[last_slash + 1..];
        (current_dir.join(dir_part), base.to_string())
    } else {
        (current_dir, raw_path.to_string())
    };

    let entries = std::fs::read_dir(&search_dir).ok()?;
    let mut dir_items = Vec::new();
    let mut file_items = Vec::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.starts_with('.') && !base_prefix.starts_with('.') {
            continue;
        }

        if !base_prefix.is_empty()
            && !file_name
                .to_lowercase()
                .starts_with(&base_prefix.to_lowercase())
        {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if file_name == "target" || file_name == "node_modules" || file_name == ".git" {
                continue;
            }
            dir_items.push(CompletionItem {
                label: format!("{file_name}/"),
                insert_text: Some(format!("{file_name}/")),
                kind: Some(CompletionItemKind::FOLDER),
                detail: Some("Directory".to_string()),
                sort_text: Some(format!("0_{file_name}")),
                ..CompletionItem::default()
            });
        } else if !is_dir_only && file_type.is_file() {
            let detail = file_extension_detail(&file_name);
            file_items.push(CompletionItem {
                label: file_name.clone(),
                insert_text: Some(file_name.clone()),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(detail),
                sort_text: Some(format!("1_{file_name}")),
                ..CompletionItem::default()
            });
        }
    }

    dir_items.sort_by(|a, b| a.label.cmp(&b.label));
    file_items.sort_by(|a, b| a.label.cmp(&b.label));

    let mut all = dir_items;
    all.extend(file_items);
    Some(all)
}

fn detect_path_context(prefix: &str) -> Option<(&str, bool)> {
    if let Some(idx) = prefix.rfind("dir:") {
        let after = &prefix[idx + 4..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, true));
        }
    }

    if let Some(idx) = prefix.rfind("@dir(") {
        let after = &prefix[idx + 5..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, true));
        }
    }

    if let Some(idx) = prefix.rfind("file:") {
        let after = &prefix[idx + 5..];
        if !after.contains(')') && !after.contains(']') && !after.contains('}') {
            let trimmed = after.trim_start();
            let path_str = trimmed.trim_matches(['"', '\'']);
            return Some((path_str, false));
        }
    }

    if let Some(idx) = prefix.rfind("./").or_else(|| prefix.rfind("../")) {
        let before = &prefix[..idx];
        if before.ends_with('(')
            || before.ends_with(':')
            || before.ends_with(',')
            || before.ends_with(' ')
            || before.ends_with('\t')
            || before.ends_with('"')
            || before.ends_with('\'')
        {
            let after = &prefix[idx..];
            if !after.contains(')') && !after.contains(']') && !after.contains('}') {
                let path_str = after.trim_matches(['"', '\'']);
                return Some((path_str, false));
            }
        }
    }

    None
}

fn find_project_root(start: &std::path::Path) -> std::path::PathBuf {
    let mut cur = start.to_path_buf();
    loop {
        // The directory a vault is: where its config is, under either name.
        if cur.join("default.config.tmt").exists()
            || cur.join("tomet.config.tmt").exists()
            || cur.join(".git").exists()
        {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    start.to_path_buf()
}

fn file_extension_detail(name: &str) -> String {
    let path = std::path::Path::new(name);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext.to_lowercase().as_str() {
        "tmt" | "tm" => "Tomet document".to_string(),
        "md" | "markdown" => "Markdown document".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "Image".to_string(),
        "typ" => "Typst document".to_string(),
        "pdf" => "PDF document".to_string(),
        "json" | "yaml" | "yml" | "toml" => "Data file".to_string(),
        _ => "File".to_string(),
    }
}

/// The shape a built-in kind must take, or `None` when either is legal.
fn shape_of(kind: &ElementKind) -> Option<Shape> {
    tomet_semantics::required_shape(kind)
}

/// Completion detail for a built-in: its description, plus where it may
/// be placed when only one placement is legal.
fn detail_for(name: &str, kind: &ElementKind) -> String {
    match shape_of(kind) {
        Some(Shape::Block) => format!("{} (block)", describe_kind(name)),
        Some(Shape::Inline) => format!("{} (inline)", describe_kind(name)),
        None => describe_kind(name).to_string(),
    }
}

/// One-line description for a built-in element name.
fn describe_kind(name: &str) -> &'static str {
    match name {
        "version" => "Tomet language specification version",
        "kind" => "Document kind (archetype / schema) declaration",
        "blueprint" => {
            "Declares the structure of one document kind, and what `tomet new` instantiates"
        }
        "raw" => "Raw verbatim block or inline text",
        "quote" => "Quote",
        "hr" => "Horizontal rule divider",
        "meta" => "Metadata key-value declaration",
        "config" => "Document-wide configuration",
        "settings" => "Schema and settings block",
        "macros" => "Macro definitions: name to template",
        "vars" => "Variable declarations: type, default, about",
        "use" => "Bind a vocabulary as a namespace",
        "references" => "Remote connection container",
        "id" => "Attach attributes to a remote element by id",
        "link" => "Link to a url, file, or document",
        "embed" => "Embed another document or asset",
        "table" => "Table",
        "heading" => "Section heading",
        "em" => "Emphasis",
        "strong" => "Strong emphasis",
        "mark" => "Highlighted text",
        "ol" => "Ordered list",
        "ul" => "Unordered list",
        _ => "Built-in element",
    }
}
