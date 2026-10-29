//! Node and child definitions for `tomet-edit`.

use std::fmt;

/// An opaque, unique identifier for a node in an [`EditDoc`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node:{}", self.0)
    }
}

/// An entry in a child list: either layout trivia (whitespace/comment) or a child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildItem {
    Trivia(String),
    Node(NodeId),
}

impl ChildItem {
    pub fn as_node_id(&self) -> Option<NodeId> {
        match self {
            Self::Node(id) => Some(*id),
            Self::Trivia(_) => None,
        }
    }
}

/// Target attribute slot or group on an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrGroup {
    /// `{ key: value, ... }` data map (e.g. in `::meta{ ... }`).
    Data,
    /// `( key: value, ... )` arguments (e.g. in `@callout(type: "info")`).
    Args,
    /// `#(id)` identity attribute slot. The `key` in `SetAttribute` is unused.
    Id,
}

/// A structured attribute group (`(args)`, `{data}`, or nested `{map}` / `[seq]`)
/// preserving exact CST trivia, brace padding, comments, and per-entry slices.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttrSlot {
    /// Original source slice for the entire group (including outer delimiters).
    pub raw: String,
    /// Opening delimiter (`"("`, `"{"`, or `"["`).
    pub open_delim: String,
    /// Closing delimiter (`")"`, `"}"`, or `"]"`).
    pub close_delim: String,
    /// Whether any entry in this group has been added, removed, or modified.
    pub edited: bool,
    /// Trivia immediately following `open_delim` before the first entry (e.g. `" "` in `{ x: 1 }`).
    pub leading_trivia: String,
    /// Trivia immediately preceding `close_delim` after the last entry (e.g. `" "` in `{ x: 1 }`).
    pub trailing_trivia: String,
    /// Ordered entries in this group.
    pub entries: Vec<AttrEntry>,
}

impl AttrSlot {
    /// Creates a new empty `AttrSlot` with the given delimiters (`"("`/`")"` or `"{"`/`"}"`).
    pub fn new_empty(open: &str, close: &str) -> Self {
        Self {
            raw: format!("{open}{close}"),
            open_delim: open.to_string(),
            close_delim: close.to_string(),
            edited: true,
            leading_trivia: String::new(),
            trailing_trivia: String::new(),
            entries: Vec::new(),
        }
    }

    /// Exports this slot (including its outer delimiters) to source text.
    pub fn export(&self) -> String {
        if !self.edited && !self.entries.iter().any(AttrEntry::is_edited) {
            return self.raw.clone();
        }
        let mut out = String::new();
        self.export_into(&mut out);
        out
    }

    /// Writes this slot (including its outer delimiters) into `out`.
    pub fn export_into(&self, out: &mut String) {
        if !self.edited && !self.entries.iter().any(AttrEntry::is_edited) {
            out.push_str(&self.raw);
            return;
        }
        out.push_str(&self.open_delim);
        out.push_str(&self.leading_trivia);
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 || (!entry.leading_sep.is_empty() && entry.leading_sep.contains("//")) {
                out.push_str(&entry.leading_sep);
            }
            entry.export_into(out);
        }
        out.push_str(&self.trailing_trivia);
        out.push_str(&self.close_delim);
    }

    /// Looks up a key in this slot and returns its exported value string, if present.
    pub fn get(&self, key: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.export())
    }

    /// Returns whether this slot contains an entry with the given key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key == key)
    }

    /// Infers the separator to place before a newly appended entry based on existing style.
    pub fn infer_new_entry_sep(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        // Check if any existing entry separator uses a newline (multi-line layout)
        for entry in self.entries.iter().rev() {
            if entry.leading_sep.contains('\n') {
                // Extract the last newline and indentation from `leading_sep`
                if let Some(nl_pos) = entry.leading_sep.rfind('\n') {
                    let indent: String = entry.leading_sep[nl_pos + 1..]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                    let comma_prefix = if entry.leading_sep[..nl_pos].trim_end().ends_with(',') {
                        ","
                    } else {
                        ""
                    };
                    return format!("{comma_prefix}\n{indent}");
                }
            }
        }
        // Single-entry multi-line block: check `leading_trivia`
        if self.leading_trivia.contains('\n')
            && let Some(nl_pos) = self.leading_trivia.rfind('\n') {
                let indent: String = self.leading_trivia[nl_pos + 1..]
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                return format!("\n{indent}");
            }
        ", ".to_string()
    }

    /// Sets, updates, or deletes a key in this `AttrSlot`.
    pub fn set_key(&mut self, key: &str, value: Option<String>) {
        self.edited = true;
        if let Some(pos) = self.entries.iter().position(|e| e.key == key) {
            match value {
                Some(v) => {
                    self.entries[pos].value = EditValue::from_raw(v);
                    self.entries[pos].edited = true;
                }
                None => {
                    self.entries.remove(pos);
                    if pos == 0 && !self.entries.is_empty() {
                        // The new first entry should not start with a leading `, `
                        let first_sep = &self.entries[0].leading_sep;
                        if !first_sep.contains("//") && !first_sep.contains("/*") {
                            self.entries[0].leading_sep.clear();
                        } else if let Some(stripped) = first_sep.strip_prefix(',') {
                            self.entries[0].leading_sep = stripped.trim_start_matches(' ').to_string();
                        }
                    }
                }
            }
        } else if let Some(v) = value {
            let sep = self.infer_new_entry_sep();
            let mut key_raw = String::new();
            if !key.is_empty() {
                cotomet_syntax::write_key(key, &mut key_raw);
            }
            let sep_raw = if key.is_empty() {
                String::new()
            } else {
                ": ".to_string()
            };
            let raw = format!("{key_raw}{sep_raw}{v}");
            self.entries.push(AttrEntry {
                leading_sep: sep,
                key: key.to_string(),
                key_raw,
                sep_raw,
                value: EditValue::from_raw(v),
                trailing_comment: String::new(),
                raw,
                edited: true,
            });
        }
    }
}

/// A single `key: value` map entry or positional sequence item inside an [`AttrSlot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrEntry {
    /// Separator and trivia preceding this entry (empty for the first entry).
    pub leading_sep: String,
    /// Logical key name (unquoted if string literal; empty `""` for positional items).
    pub key: String,
    /// Exact raw key token text as written in source (empty `""` for positional items).
    pub key_raw: String,
    /// Exact separator between key and value (e.g. `": "`; empty for positional items).
    pub sep_raw: String,
    /// Structured value of this entry.
    pub value: EditValue,
    /// Optional inline comment following the value on the same line (e.g. `" // note"`).
    pub trailing_comment: String,
    /// Original raw source slice of `key_raw + sep_raw + value.raw + trailing_comment`.
    pub raw: String,
    /// Whether this entry was modified or newly inserted.
    pub edited: bool,
}

impl AttrEntry {
    pub fn is_edited(&self) -> bool {
        self.edited || self.value.is_edited()
    }

    pub fn export_into(&self, out: &mut String) {
        if !self.is_edited() {
            out.push_str(&self.raw);
            return;
        }
        if !self.key.is_empty() {
            if self.key_raw.is_empty() {
                cotomet_syntax::write_key(&self.key, out);
            } else {
                out.push_str(&self.key_raw);
            }
            if self.sep_raw.is_empty() {
                out.push_str(": ");
            } else {
                out.push_str(&self.sep_raw);
            }
        }
        self.value.export_into(out);
        out.push_str(&self.trailing_comment);
    }
}

/// A structured value inside an [`AttrEntry`], preserving its raw CST slice and nested structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditValue {
    /// Exact source slice of this value.
    pub raw: String,
    /// Whether this value (or a nested entry inside it) has been edited.
    pub edited: bool,
    /// Structured value variant.
    pub kind: EditValueKind,
}

impl EditValue {
    pub fn from_raw(raw: impl Into<String>) -> Self {
        let s = raw.into();
        crate::import::parse_edit_value_str(&s)
    }

    pub fn is_edited(&self) -> bool {
        if self.edited {
            return true;
        }
        match &self.kind {
            EditValueKind::Scalar(_) => false,
            EditValueKind::Map(slot) | EditValueKind::Seq(slot) => {
                slot.edited || slot.entries.iter().any(AttrEntry::is_edited)
            }
            EditValueKind::Element { args, data, .. } => {
                args.as_ref()
                    .is_some_and(|s| s.edited || s.entries.iter().any(AttrEntry::is_edited))
                    || data
                        .as_ref()
                        .is_some_and(|s| s.edited || s.entries.iter().any(AttrEntry::is_edited))
            }
        }
    }

    pub fn export(&self) -> String {
        if !self.is_edited() {
            return self.raw.clone();
        }
        let mut out = String::new();
        self.export_into(&mut out);
        out
    }

    pub fn export_into(&self, out: &mut String) {
        if !self.is_edited() {
            out.push_str(&self.raw);
            return;
        }
        match &self.kind {
            EditValueKind::Scalar(s) => out.push_str(s),
            EditValueKind::Map(slot) | EditValueKind::Seq(slot) => slot.export_into(out),
            EditValueKind::Element {
                sigil_raw,
                args,
                data,
                content_raw,
                ..
            } => {
                out.push_str(sigil_raw);
                if let Some(a) = args {
                    a.export_into(out);
                }
                if let Some(d) = data {
                    d.export_into(out);
                }
                if let Some(c) = content_raw {
                    out.push_str(c);
                }
            }
        }
    }
}

/// Specific variant of an [`EditValue`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum EditValueKind {
    /// Scalar literal, identifier, or call expression.
    Scalar(String),
    /// Nested map literal `{ ... }`.
    Map(AttrSlot),
    /// Nested sequence literal `[ ... ]`.
    Seq(AttrSlot),
    /// Inline element in value position (e.g. `@doc.icon(star)` or `@conflict(a: 1, b: 2)`).
    Element {
        sigil_raw: String,
        name: String,
        args: Option<AttrSlot>,
        data: Option<AttrSlot>,
        content_raw: Option<String>,
    },
}

/// A piece of an element's CST layout in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementPiece {
    Sigil(String),
    Trivia(String),
    Id { raw: String },
    Args,
    Data,
    Content {
        open_raw: String,
        close_raw: String,
        raw: String,
    },
    Connect(String),
}

/// Fine-grained slot and child-block representation for a [`NodeKind::BlockElement`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ElementSlots {
    /// Ordered pieces of the element header/groups from the CST.
    pub pieces: Vec<ElementPiece>,
    /// Whether the `#(id)` slot was edited.
    pub id_edited: bool,
    /// Structured `(args)` group, if present.
    pub args_slot: Option<AttrSlot>,
    /// Structured `{data}` group, if present.
    pub data_slot: Option<AttrSlot>,
    /// Child block/trivia items inside `[content]`, if a `[content]` group is present.
    pub content_children: Option<Vec<ChildItem>>,
    /// Whether the `[content]` slot was directly replaced or modified.
    pub content_edited: bool,
}

impl ElementSlots {
    pub fn ensure_piece_id(&mut self) {
        if !self
            .pieces
            .iter()
            .any(|p| matches!(p, ElementPiece::Id { .. }))
        {
            let pos = self
                .pieces
                .iter()
                .position(|p| matches!(p, ElementPiece::Sigil(_)))
                .map_or(0, |i| i + 1);
            self.pieces.insert(
                pos,
                ElementPiece::Id {
                    raw: String::new(),
                },
            );
        }
    }

    pub fn ensure_piece_args(&mut self) {
        if !self.pieces.iter().any(|p| matches!(p, ElementPiece::Args)) {
            let pos = self
                .pieces
                .iter()
                .rposition(|p| matches!(p, ElementPiece::Id { .. } | ElementPiece::Sigil(_)))
                .map_or(0, |i| i + 1);
            self.pieces.insert(pos, ElementPiece::Args);
        }
    }

    pub fn ensure_piece_data(&mut self) {
        if !self.pieces.iter().any(|p| matches!(p, ElementPiece::Data)) {
            let pos = self
                .pieces
                .iter()
                .rposition(|p| {
                    matches!(
                        p,
                        ElementPiece::Args | ElementPiece::Id { .. } | ElementPiece::Sigil(_)
                    )
                })
                .map_or(0, |i| i + 1);
            self.pieces.insert(pos, ElementPiece::Data);
        }
    }

    pub fn ensure_piece_content(&mut self) {
        if !self
            .pieces
            .iter()
            .any(|p| matches!(p, ElementPiece::Content { .. }))
        {
            let pos = self
                .pieces
                .iter()
                .rposition(|p| {
                    matches!(
                        p,
                        ElementPiece::Data
                            | ElementPiece::Args
                            | ElementPiece::Id { .. }
                            | ElementPiece::Sigil(_)
                    )
                })
                .map_or(self.pieces.len(), |i| i + 1);
            self.pieces.insert(
                pos,
                ElementPiece::Content {
                    open_raw: "[".to_string(),
                    close_raw: "]".to_string(),
                    raw: String::new(),
                },
            );
        }
    }
}

/// A structured inline item within a paragraph or section heading.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum InlineItem {
    /// Plain prose text run.
    Text(String),
    /// Inline trivia such as `/* ... */` comments, `// ...` comments, or trailing newline.
    Trivia(String),
    /// Delimited inline formatting sugar (`*em*`, `**strong**`, `__strong__`, `_em_`, `~~strikeout~~`, `` `code` ``).
    Sugar {
        kind: String,
        delim: String,
        inner_raw: String,
        raw: String,
    },
    /// An inline element (`@link(...)`, `${...}`, `$func(...)`, `@conflict(...)`).
    Element {
        name: String,
        raw: String,
        slots: ElementSlots,
    },
}

impl InlineItem {
    pub fn raw_str(&self) -> &str {
        match self {
            Self::Text(s) | Self::Trivia(s) => s,
            Self::Sugar { raw, .. } | Self::Element { raw, .. } => raw,
        }
    }
}

/// An editable node in a Tomet document tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// The original source slice for this node in the CST.
    /// When `edited` is false, exporting writes this verbatim.
    pub raw: String,
    /// Whether this node has been modified since import.
    pub edited: bool,
    /// Fine-grained CST slots for [`NodeKind::BlockElement`] (and section heading groups).
    pub slots: ElementSlots,
    /// Structured inline children for [`NodeKind::Paragraph`] and [`NodeKind::Section`] headings.
    pub inlines: Vec<InlineItem>,
    /// Prefix before section heading title (e.g. `"=[ "` in `"=[ Main Heading ]\n"`).
    pub heading_prefix: String,
    /// Suffix after section heading title (e.g. `" ]\n"` in `"=[ Main Heading ]\n"`).
    pub heading_suffix: String,
}

impl Node {
    pub fn new(id: NodeId, kind: NodeKind, raw: String) -> Self {
        let inlines = match &kind {
            NodeKind::Paragraph { text } => crate::import::parse_inline_items(text),
            _ => Vec::new(),
        };
        Self {
            id,
            kind,
            raw,
            edited: false,
            slots: ElementSlots::default(),
            inlines,
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Mark this node as edited so export regenerates its text.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Returns the identity attribute `#(id)` attached to this node, if any.
    pub fn id_attr(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::BlockElement { id_attr, .. }
            | NodeKind::Section { id_attr, .. }
            | NodeKind::ListItem { id_attr, .. } => id_attr.as_deref(),
            _ => None,
        }
    }

    /// Returns the child block/trivia list inside an element's `[content]` group, if any.
    pub fn content_children(&self) -> Option<&[ChildItem]> {
        self.slots.content_children.as_deref()
    }

    /// Returns the structured `(args)` slot on an element, if any.
    pub fn args_slot(&self) -> Option<&AttrSlot> {
        self.slots.args_slot.as_ref()
    }

    /// Returns the structured `{data}` slot on an element, if any.
    pub fn data_slot(&self) -> Option<&AttrSlot> {
        self.slots.data_slot.as_ref()
    }

    /// Returns the structured inline items of a paragraph or section heading.
    pub fn inlines(&self) -> &[InlineItem] {
        &self.inlines
    }

    /// Constructs a new [`NodeKind::Paragraph`] node.
    pub fn paragraph(id: NodeId, text: impl Into<String>) -> Self {
        let mut text_str = text.into();
        if !text_str.ends_with('\n') {
            text_str.push('\n');
        }
        let inlines = crate::import::parse_inline_items(&text_str);
        Self {
            id,
            kind: NodeKind::Paragraph {
                text: text_str.clone(),
            },
            raw: text_str,
            edited: true,
            slots: ElementSlots::default(),
            inlines,
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new [`NodeKind::CodeBlock`] node.
    pub fn code_block(
        id: NodeId,
        lang: Option<impl Into<String>>,
        code: impl Into<String>,
    ) -> Self {
        let lang_opt = lang.map(Into::into);
        let code_str = code.into();
        let formatted = if code_str.starts_with("```") {
            code_str
        } else {
            let l = lang_opt.as_deref().unwrap_or("");
            let trimmed = code_str.trim_end_matches(['\r', '\n']);
            format!("```{l}\n{trimmed}\n```\n")
        };
        Self {
            id,
            kind: NodeKind::CodeBlock {
                lang: lang_opt,
                code: formatted.clone(),
            },
            raw: formatted,
            edited: true,
            slots: ElementSlots::default(),
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new [`NodeKind::Section`] node with a formatted heading.
    pub fn section(
        id: NodeId,
        level: usize,
        title: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let level = level.max(1);
        let prefix = "=".repeat(level);
        let title_str = title.into();
        let id_opt = id_attr.map(Into::into);
        let heading_prefix = format!("{prefix}[ ");
        let heading_suffix = match &id_opt {
            Some(id_str) => format!(" ] #({id_str})\n"),
            None => " ]\n".to_string(),
        };
        let heading_raw = format!("{heading_prefix}{title_str}{heading_suffix}");
        let inlines = crate::import::parse_inline_items(&title_str);
        Self {
            id,
            kind: NodeKind::Section {
                level,
                heading_raw: heading_raw.clone(),
                id_attr: id_opt,
                children: Vec::new(),
            },
            raw: heading_raw,
            edited: true,
            slots: ElementSlots::default(),
            inlines,
            heading_prefix,
            heading_suffix,
        }
    }

    /// Constructs a new [`NodeKind::ThematicBreak`] node.
    pub fn thematic_break(id: NodeId) -> Self {
        let raw = "---\n".to_string();
        Self {
            id,
            kind: NodeKind::ThematicBreak { raw: raw.clone() },
            raw,
            edited: true,
            slots: ElementSlots::default(),
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new [`NodeKind::List`] node grouping child list items.
    pub fn list(id: NodeId, items: impl IntoIterator<Item = NodeId>) -> Self {
        let children = items.into_iter().map(ChildItem::Node).collect();
        Self {
            id,
            kind: NodeKind::List { children },
            raw: String::new(),
            edited: true,
            slots: ElementSlots::default(),
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new [`NodeKind::ListItem`] node.
    pub fn list_item(
        id: NodeId,
        marker: impl Into<String>,
        text: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let marker = marker.into();
        let mut text_str = text.into();
        if !text_str.ends_with('\n') {
            text_str.push('\n');
        }
        let id_opt = id_attr.map(Into::into);
        let formatted = match &id_opt {
            Some(id_str) => {
                let trimmed = text_str.trim_end_matches(['\r', '\n']);
                format!("{marker} {trimmed} #({id_str})\n")
            }
            None => format!("{marker} {text_str}"),
        };
        Self {
            id,
            kind: NodeKind::ListItem {
                marker,
                text: formatted.clone(),
                id_attr: id_opt,
                children: vec![ChildItem::Trivia(formatted.clone())],
            },
            raw: formatted,
            edited: true,
            slots: ElementSlots::default(),
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new task [`NodeKind::ListItem`] with a Tomet value marker (`- (x)` or `- ()`).
    pub fn task_item(
        id: NodeId,
        marker: impl Into<String>,
        checked: bool,
        text: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let marker = marker.into();
        let marker_val = if checked { "(x)" } else { "()" };
        let text_str = text.into();
        let trimmed = text_str.trim_end_matches(['\r', '\n']);
        let id_opt = id_attr.map(Into::into);
        let formatted = match &id_opt {
            Some(id_str) => format!("{marker} {marker_val} {trimmed} #({id_str})\n"),
            None => format!("{marker} {marker_val} {trimmed}\n"),
        };
        Self {
            id,
            kind: NodeKind::ListItem {
                marker,
                text: formatted.clone(),
                id_attr: id_opt,
                children: vec![ChildItem::Trivia(formatted.clone())],
            },
            raw: formatted,
            edited: true,
            slots: ElementSlots::default(),
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }

    /// Constructs a new [`NodeKind::BlockElement`] node.
    pub fn block_element(
        id: NodeId,
        name: impl Into<String>,
        declaration: bool,
        id_attr: Option<impl Into<String>>,
        args: Vec<(String, String)>,
        data: Vec<(String, String)>,
        content: Option<impl Into<String>>,
    ) -> Self {
        let name_str = name.into();
        let id_opt = id_attr.map(Into::into);
        let content_opt = content.map(Into::into);
        let sigil_prefix = if declaration { "::" } else { "@" };
        let mut slots = ElementSlots {
            pieces: vec![ElementPiece::Sigil(format!("{sigil_prefix}{name_str}"))],
            id_edited: id_opt.is_some(),
            args_slot: None,
            data_slot: None,
            content_children: None,
            content_edited: content_opt.is_some(),
        };
        if let Some(ref id_val) = id_opt {
            slots.pieces.push(ElementPiece::Id {
                raw: format!("#({id_val})"),
            });
        }
        if !args.is_empty() {
            let mut slot = AttrSlot::new_empty("(", ")");
            for (k, v) in &args {
                slot.set_key(k, Some(v.clone()));
            }
            slots.args_slot = Some(slot);
            slots.pieces.push(ElementPiece::Args);
        }
        if !data.is_empty() {
            let mut slot = AttrSlot::new_empty("{", "}");
            for (k, v) in &data {
                slot.set_key(k, Some(v.clone()));
            }
            slots.data_slot = Some(slot);
            slots.pieces.push(ElementPiece::Data);
        }
        if let Some(ref c) = content_opt {
            slots.content_children = Some(vec![ChildItem::Trivia(c.clone())]);
            slots.pieces.push(ElementPiece::Content {
                open_raw: "[".to_string(),
                close_raw: "]".to_string(),
                raw: format!("[{c}]"),
            });
        }
        Self {
            id,
            kind: NodeKind::BlockElement {
                name: name_str,
                declaration,
                id_attr: id_opt,
            },
            raw: String::new(),
            edited: true,
            slots,
            inlines: Vec::new(),
            heading_prefix: String::new(),
            heading_suffix: String::new(),
        }
    }
}

/// Specific variant of an editable node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A section with its heading and child blocks/trivia.
    Section {
        level: usize,
        heading_raw: String,
        id_attr: Option<String>,
        children: Vec<ChildItem>,
    },
    /// A paragraph whose content is maintained both as raw text and structured inline items on [`Node::inlines`].
    Paragraph { text: String },
    /// A block element (e.g. `@callout`, `@note`, `@table`, `::meta`).
    /// Its `(args)`, `{data}`, and `[content]` child slots live in [`Node::slots`].
    BlockElement {
        name: String,
        /// Written as a document declaration (`::name`) rather than an
        /// element (`@name`); export keeps the spelling it was read with.
        declaration: bool,
        id_attr: Option<String>,
    },
    /// A list grouping list items and trivia.
    List { children: Vec<ChildItem> },
    /// A single item within a list.
    ListItem {
        marker: String,
        text: String,
        id_attr: Option<String>,
        children: Vec<ChildItem>,
    },
    /// A fenced code block (` ```lang ... ``` `).
    CodeBlock { lang: Option<String>, code: String },
    /// A thematic break / horizontal rule (`---`).
    ThematicBreak { raw: String },
    /// Other block kinds preserved as raw text.
    Other { kind_name: String },
}
