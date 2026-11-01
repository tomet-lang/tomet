//! Pandoc's AST -> `tomet_ast::Document`.
//!
//! The inverse of [`crate::to_pandoc`], and deliberately not a perfect
//! one. All three losses below are pinned by `tomet-tests`'
//! `pandoc_round_trip_is_stable`, so they show up as a diff rather than
//! as a surprise -- they are recorded behaviour, not bugs to be fixed
//! quietly.
//!
//! - **A positional `(args)` returns as `{value: ...}`.** Pandoc's `Attr`
//!   has no key for a keyless group, and nothing in it records which
//!   group a pair came from. `@line(天音かなた)[セリフ]` returns as
//!   `@line[セリフ]{ value: 天音かなた }`. When the exact copy *is*
//!   present (see `tomet_semantics::flatten`) both groups come back
//!   intact.
//! - **A number in `::meta` returns as a string.** Pandoc metadata has no
//!   numeric type.
//! - **A named element inside a `{...}` group returns as body content.**
//!   Pandoc cannot say "this `Div` was a group entry rather than part of
//!   the body", so `@deck.card{ title: x, @note[y] }` comes back with the
//!   entry in `[content]`. A bare `(a)[y]` entry does not share the loss:
//!   it has no name to carry, so [`crate::to_pandoc`] marks it under
//!   [`SIGIL_KEY`] and it returns to the group.
//!
//!   Accepted rather than fixed, and the reason is what this bridge is
//!   for. It exists to reach the formats Pandoc reads and writes --
//!   `tmt -> docx`, `docx -> tmt` -- and in that direction the entry
//!   arrives as a `Div` in the body, which is visible and right: it is an
//!   element, and elements are content. `tmt -> pandoc -> tmt` is nobody's
//!   workflow; it is `pandoc_round_trip_is_stable`, a diagnostic. Making
//!   it lossless would mean carrying the group's whole shape in an opaque
//!   exact-copy, which is precisely the payload every other tool on the
//!   far side cannot read.
//!
//!   The pairs beside it do survive, in `Attr` -- they are data, and
//!   `Attr` is where data goes. Only the elements move.

use tomet_ast::{
    Bare, Block as TmBlock, Data, Declaration, Document, Element, Entry, Group, Id,
    Inline as TmInline, LineBreak, List, Name, Paragraph, Placement, RawText, Section, SoftBreak,
    Text, Value,
};
use tomet_span::Span;
use tomet_semantics::EXACT_DATA_KEY;

use crate::to_pandoc::{BARE_SIGIL, SIGIL_KEY};
use tomet_tree::{ElementExt, declaration_new, element_new, list, list_item};

use crate::ast::{Attr, Block, Inline, MetaValue, PandocDoc, Row, TableParts};

/// The class prefix [`crate::to_pandoc`] puts on an element it has no
/// Pandoc equivalent for, and the only way back to its name.
const CLASS_PREFIX: &str = "tomet-";

/// Converts a Pandoc document to a Tomet document.
pub fn from_pandoc(doc: &PandocDoc) -> Document {
    let mut blocks = Vec::new();
    // Pandoc's `id` is the identity of the document: the declaration `::id(...)`, not a field of `::meta`.
    let mut meta = doc.meta.clone();
    let id = meta
        .remove("id")
        .and_then(|value| match meta_value_to_value(&value) {
            Value::String(text) => Some(text),
            other => other.scalar_text().map(|text| text.into_owned()),
        });
    if let Some(id) = id {
        let mut id_declaration = declaration_new("id");
        id_declaration.data.args = Some(Value::String(id));
        blocks.push(TmBlock::Declaration(id_declaration));
    }
    if !meta.is_empty() {
        blocks.push(TmBlock::Declaration(meta_declaration(&meta)));
    }
    for block in &doc.blocks {
        if let Some(b) = block_from_pandoc(block) {
            blocks.push(b);
        }
    }
    Document::new(structure_sections(blocks), Span::dummy())
}

fn structure_sections(blocks: Vec<TmBlock>) -> Vec<TmBlock> {
    let mut doc_blocks = Vec::new();
    let mut stack: Vec<Section> = Vec::new();

    for block in blocks {
        match block {
            TmBlock::Section(sec) => {
                while let Some(top) = stack.last() {
                    if top.level >= sec.level {
                        let finished = stack.pop().unwrap();
                        if let Some(parent) = stack.last_mut() {
                            parent.blocks.push(TmBlock::Section(finished));
                        } else {
                            doc_blocks.push(TmBlock::Section(finished));
                        }
                    } else {
                        break;
                    }
                }
                stack.push(sec);
            }
            other => {
                if let Some(top) = stack.last_mut() {
                    top.blocks.push(other);
                } else {
                    doc_blocks.push(other);
                }
            }
        }
    }

    while let Some(finished) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.blocks.push(TmBlock::Section(finished));
        } else {
            doc_blocks.push(TmBlock::Section(finished));
        }
    }

    doc_blocks
}

/// Rebuilds `::meta{...}` from Pandoc's metadata map.
fn meta_declaration(meta: &crate::ast::Meta) -> Declaration {
    let entries: Vec<(String, Value)> = meta
        .iter()
        .map(|(k, v)| (k.clone(), meta_value_to_value(v)))
        .collect();
    let mut declaration = declaration_new("meta");
    declaration.data.value = Some(Group::from_map(Value::Map(entries)));
    declaration
}

fn meta_value_to_value(v: &MetaValue) -> Value {
    match v {
        MetaValue::MetaString(s) => Value::String(s.clone()),
        MetaValue::MetaBool(b) => Value::Bool(*b),
        MetaValue::MetaList(items) => Value::Seq(items.iter().map(meta_value_to_value).collect()),
        MetaValue::MetaMap(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), meta_value_to_value(v)))
                .collect(),
        ),
        // Metadata written as inlines flattens back to its text. Tomet
        // metadata values are plain data, so nothing is lost that was
        // there to begin with.
        MetaValue::MetaInlines(inlines) => Value::String(inlines_to_text(inlines)),
        MetaValue::MetaBlocks(blocks) => Value::String(
            blocks
                .iter()
                .map(|b| match b {
                    Block::Plain(i) | Block::Para(i) => inlines_to_text(i),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

// ---- blocks --------------------------------------------------------

fn block_from_pandoc(block: &Block) -> Option<TmBlock> {
    match block {
        Block::Header(level, attr, inlines) => {
            let title = inlines_from_pandoc(inlines);
            let mut sec = Section::new(*level as usize, title, Span::dummy());
            let (args, value) = extract_data_from_attr(attr);
            sec.data.args = args;
            sec.data.value = value;
            sec.id = id_from_attr(attr);
            Some(TmBlock::Section(sec))
        }
        Block::Para(inlines) | Block::Plain(inlines) => Some(TmBlock::Paragraph(Paragraph::new(
            inlines_from_pandoc(inlines),
            Span::dummy(),
        ))),
        // No tomet spelling is decided yet for a term/definition pair --
        // the `@definition` element is still on hold. This used to land
        // in `@links`, which had the right shape by accident; `@links`
        // was a different, now-removed feature (reference-link
        // definitions) and is not a stand-in for a real definition list,
        // so this is dropped rather than forced into it.
        Block::DefinitionList(_) => None,
        Block::BulletList(items) => Some(TmBlock::List(list_element(items, false))),
        Block::OrderedList(_, items) => Some(TmBlock::List(list_element(items, true))),
        other => Some(TmBlock::Element(block_element(other))),
    }
}

/// A Pandoc block as a block-placed Tomet element.
fn block_element(block: &Block) -> Element {
    let el = match block {
        Block::Header(level, attr, inlines) => {
            let mut el = named_element("heading", attr);
            el.data.args = Some(Value::Int(*level));
            el.content = Some(wrap_inline_content(inlines_from_pandoc(inlines)));
            el
        }
        Block::HorizontalRule => element_new(Name::bare("hr")),
        Block::CodeBlock(attr, code) => {
            let mut el = named_element("raw", attr);
            // The language travelled as the first class; put it back in
            // `lang:` where `to_pandoc` looks for it.
            if let Some(lang) = attr.1.iter().find(|c| !c.starts_with(CLASS_PREFIX)) {
                el.data.args = Some(Value::Map(vec![(
                    "lang".to_string(),
                    Value::String(lang.clone()),
                )]));
            }
            el.content = Some(vec![TmBlock::Paragraph(Paragraph::new(
                vec![TmInline::Raw(RawText::new(code.clone(), Span::dummy()))],
                Span::dummy(),
            ))]);
            el
        }
        Block::RawBlock(format, text) => {
            // No Tomet spelling for "raw output in some other format", so
            // it lands as a code block tagged with the format rather than
            // being dropped.
            let mut el = element_new(Name::bare("raw"));
            el.data.args = Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String(format.clone()),
            )]));
            el.content = Some(vec![TmBlock::Paragraph(Paragraph::new(
                vec![TmInline::Raw(RawText::new(text.clone(), Span::dummy()))],
                Span::dummy(),
            ))]);
            el
        }
        Block::BlockQuote(blocks) => {
            let mut el = element_new(Name::bare("quote"));
            el.content = Some(pandoc_blocks_to_content(blocks));
            el
        }
        // A `List` is not an `Element` at all any more (see
        // `tomet_ast::List`), so it can't be produced here -- both
        // callers (`block_from_pandoc`, `blocks_to_content`) handle
        // `BulletList`/`OrderedList` themselves before ever reaching
        // this function, the same way `DefinitionList` is handled
        // before `block_element` below.
        Block::BulletList(_) | Block::OrderedList(..) => {
            unreachable!("BulletList/OrderedList is handled before block_element")
        }
        Block::Table(parts) => table_element(parts),
        Block::Div(attr, blocks) => {
            let mut el = element_from_attr(attr, || "div".to_string());
            let (content, entries) = body_from_pandoc(blocks);
            el.content = Some(content);
            push_bare_entries(&mut el.data, entries);
            el
        }
        Block::Figure(attr, _, blocks) => {
            let mut el = named_element("figure", attr);
            el.content = Some(pandoc_blocks_to_content(blocks));
            el
        }
        Block::LineBlock(lines) => {
            let mut el = element_new(Name::bare("quote"));
            let mut content = Vec::new();
            for line in lines {
                content.extend(inlines_from_pandoc(line));
            }
            el.content = Some(wrap_inline_content(content));
            el
        }
        // Handled by `block_from_pandoc`; reached only if this is called
        // directly with one.
        Block::Para(inlines) | Block::Plain(inlines) => {
            let mut el = element_new(Name::bare("paragraph"));
            el.content = Some(wrap_inline_content(inlines_from_pandoc(inlines)));
            el
        }
        // Dropped by both callers (`block_from_pandoc`, `blocks_to_content`)
        // before it ever reaches here -- see `block_from_pandoc`'s own arm.
        Block::DefinitionList(_) => unreachable!("DefinitionList is dropped before block_element"),
    };
    el.with_placement(Placement::Block)
}

/// `sublist` is always `None` here: a nested `BulletList`/`OrderedList`
/// inside an item's own Pandoc blocks round-trips as a `Block::List`
/// entry in that item's `content` instead (via `block_from_pandoc`'s own
/// `BulletList`/`OrderedList` arm, reached through `pandoc_blocks_to_content`
/// below), the same shape this crate's reconstruction has always produced
/// (`tests/ref/roadmap.pandoc.roundtrip.tmt` pins the exact text this
/// produces -- a nested list reached this way prints at the printer's
/// top-level indent, not nested under the item, which is a pre-existing
/// round-trip quirk, not something this task's `List`/`ListItem`
/// promotion set out to fix).
fn list_element(items: &[Vec<Block>], ordered: bool) -> List {
    let elements = items
        .iter()
        .map(|blocks| {
            list_item(
                None,
                pandoc_blocks_to_content(blocks),
                None,
                None,
                None,
                Vec::new(),
                None,
                Span::dummy(),
            )
        })
        .collect();
    list(ordered, elements, Span::dummy())
}

/// Rebuilds a `@table` from Pandoc's table.
///
/// Tomet has no table node of its own: a table is an element whose
/// `[content]` holds `[cell][cell]` runs that `parse_table_rows` reads
/// back. So the cells are re-emitted as that literal text.
fn table_element(parts: &TableParts) -> Element {
    let TableParts(attr, _, colspecs, head, bodies, foot) = parts;
    let mut rows: Vec<&Row> = head.1.iter().collect();
    for body in bodies {
        rows.extend(body.2.iter());
        rows.extend(body.3.iter());
    }
    rows.extend(foot.1.iter());

    let mut content = Vec::new();
    for (i, Row(_, cells)) in rows.iter().enumerate() {
        if i > 0 {
            content.push(TmInline::Text(Text::from("\n".to_string())));
        }
        for cell in cells {
            content.push(TmInline::Text(Text::from("[ ".to_string())));
            content.extend(blocks_to_content(&cell.4));
            content.push(TmInline::Text(Text::from(" ]".to_string())));
        }
    }

    let mut el = named_element("table", attr);
    el.content = Some(wrap_inline_content(content));
    // `header: false` only needs saying when there is no head row; the
    // reader's default is that row 0 is the header.
    let mut args: Vec<(String, Value)> = Vec::new();
    if head.1.is_empty() {
        args.push(("header".to_string(), Value::Bool(false)));
    }
    if let Some(align) = colspecs.first().and_then(|c| alignment_name(&c.0)) {
        args.push(("align".to_string(), Value::String(align.to_string())));
    }
    if !args.is_empty() {
        el.data.args = Some(Value::Map(args));
    }
    el
}

fn alignment_name(align: &crate::ast::Alignment) -> Option<&'static str> {
    match align {
        crate::ast::Alignment::AlignLeft => Some("left"),
        crate::ast::Alignment::AlignCenter => Some("center"),
        crate::ast::Alignment::AlignRight => Some("right"),
        crate::ast::Alignment::AlignDefault => None,
    }
}

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>` now):
/// one `Paragraph`, or nothing for an empty sequence. Every pandoc import
/// path here builds `[content]` as plain inlines first (the loss notes at
/// the top of this module are about that shape, pinned by
/// `pandoc_round_trip_is_stable`), so this is the single place that
/// adapts all of them to the new field type without changing what any of
/// them actually produce.
fn wrap_inline_content(inlines: Vec<TmInline>) -> Vec<TmBlock> {
    if inlines.is_empty() {
        Vec::new()
    } else {
        vec![TmBlock::Paragraph(Paragraph::new(inlines, Span::dummy()))]
    }
}

/// Pandoc blocks as `Element.content` (`Vec<Block>` now): each one through
/// [`block_from_pandoc`], same as the document's own top level, dropping
/// anything that drops there too (a `DefinitionList`). This is what a
/// `Div`/`BlockQuote`/`Figure`/list item/footnote's body should use now
/// that content can hold real blocks -- [`blocks_to_content`] below is
/// the older flatten-to-one-inline-run shape, kept for the few positions
/// that are still genuinely inline (a table cell).
fn pandoc_blocks_to_content(blocks: &[Block]) -> Vec<TmBlock> {
    blocks.iter().filter_map(block_from_pandoc).collect()
}

/// Pandoc blocks as a Tomet `[content]` group.
///
/// The inverse of `to_pandoc`'s `content_to_blocks`: a paragraph
/// contributes its inlines directly, and anything else becomes a
/// block-placed `Inline::Element` -- which is exactly the shape
/// `Placement` exists to record.
fn blocks_to_content(blocks: &[Block]) -> Vec<TmInline> {
    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            out.push(TmInline::Text(Text::from(" ".to_string())));
        }
        match block {
            Block::Para(inlines) | Block::Plain(inlines) => {
                out.extend(inlines_from_pandoc(inlines))
            }
            // See `block_from_pandoc`'s own `DefinitionList` arm: dropped
            // here too, for the same reason.
            Block::DefinitionList(_) => {}
            // No `TmInline` form for a `List` at all (see
            // `tomet_ast::List`) -- flattens each item's own content into
            // this same inline run instead, same lossy treatment
            // `tomet-convert-markdown`'s importer gives a list inside a
            // flattened blockquote.
            Block::BulletList(items) | Block::OrderedList(_, items) => {
                for (j, item_blocks) in items.iter().enumerate() {
                    if j > 0 {
                        out.push(TmInline::Text(Text::from(" ".to_string())));
                    }
                    out.extend(blocks_to_content(item_blocks));
                }
            }
            other => out.push(TmInline::Element(block_element(other))),
        }
    }
    out
}

// ---- inlines -------------------------------------------------------

fn inlines_from_pandoc(inlines: &[Inline]) -> Vec<TmInline> {
    let mut out: Vec<TmInline> = Vec::new();
    for inline in inlines {
        match inline_from_pandoc(inline) {
            // Merge adjacent text so the result reads like prose rather
            // than one node per word.
            TmInline::Text(t) => match out.last_mut() {
                Some(TmInline::Text(prev)) => prev.value.push_str(&t.value),
                _ => out.push(TmInline::Text(t)),
            },
            other => out.push(other),
        }
    }
    out
}

fn inline_from_pandoc(inline: &Inline) -> TmInline {
    match inline {
        Inline::Str(s) => TmInline::Text(Text::from(s.clone())),
        Inline::Space => TmInline::Text(Text::from(" ".to_string())),
        Inline::SoftBreak => TmInline::SoftBreak(SoftBreak {
            span: Span::dummy(),
        }),
        Inline::LineBreak => TmInline::LineBreak(LineBreak {
            span: Span::dummy(),
        }),
        Inline::Code(attr, code) => {
            let mut el = named_element("raw", attr);
            el.placement = Placement::Inline;
            el.content = Some(vec![TmBlock::Paragraph(Paragraph::new(
                vec![TmInline::Raw(RawText::new(code.clone(), Span::dummy()))],
                Span::dummy(),
            ))]);
            TmInline::Element(el)
        }
        Inline::Math(_, text) => TmInline::Text(Text::from(text.clone())),
        Inline::RawInline(_, text) => TmInline::Text(Text::from(text.clone())),
        Inline::Emph(inner) => inline_element("em", &Attr::empty(), inner),
        Inline::Underline(inner) => inline_element("em", &Attr::empty(), inner),
        Inline::Strong(inner) => inline_element("strong", &Attr::empty(), inner),
        Inline::Strikeout(inner) => inline_element("strikeout", &Attr::empty(), inner),
        // No element, and no lie either. Small caps is a way of drawing
        // letters rather than something the words mean, so it is the one
        // of these that is not worth an element -- the text comes back
        // and the drawing does not. Recorded with the other losses in
        // this module's doc.
        Inline::SmallCaps(inner) => TmInline::Text(Text::from(inlines_to_text(inner))),
        // Still `mark`, and still wrong: a superscript is not a
        // highlight. Unlike small caps these two carry meaning -- `x²`
        // and `H₂O` are not the same words as `x2` and `H2O` -- so
        // dropping the decoration is not free either. They want elements.
        // See `.agents/tasks/quote-and-inline-marks.md`.
        Inline::Superscript(inner) | Inline::Subscript(inner) => {
            inline_element("mark", &Attr::empty(), inner)
        }
        // A quotation is not a highlight. It used to become one: there
        // was no inline quote element to give it, so it went to the
        // nearest builtin, and `mark` is a builtin so the document
        // validated and nobody saw it.
        Inline::Quoted(_, inner) => inline_element("quote", &Attr::empty(), inner),
        Inline::Link(attr, text, target) => {
            let mut el = named_element("link", attr);
            merge_arg(&mut el, "target", Value::String(target.0.clone()));
            el.content = Some(wrap_inline_content(inlines_from_pandoc(text)));
            TmInline::Element(el)
        }
        Inline::Image(attr, alt, target) => {
            let mut el = named_element("embed", attr);
            merge_arg(&mut el, "target", Value::String(target.0.clone()));
            el.content = Some(wrap_inline_content(inlines_from_pandoc(alt)));
            TmInline::Element(el)
        }
        Inline::Note(blocks) => {
            let mut el = element_new(Name::bare("quote"));
            el.content = Some(pandoc_blocks_to_content(blocks));
            TmInline::Element(el)
        }
        Inline::Span(attr, inner) => {
            // `mark` travelled as a span carrying that class.
            let mut el = element_from_attr(attr, || {
                if attr.1.iter().any(|c| c == "mark") {
                    "mark".to_string()
                } else {
                    "span".to_string()
                }
            });
            el.content = Some(wrap_inline_content(inlines_from_pandoc(inner)));
            TmInline::Element(el)
        }
    }
}

fn inline_element(name: &str, attr: &Attr, inner: &[Inline]) -> TmInline {
    let mut el = named_element(name, attr);
    el.content = Some(wrap_inline_content(inlines_from_pandoc(inner)));
    TmInline::Element(el)
}

fn inlines_to_text(inlines: &[Inline]) -> String {
    inlines_to_text_tm(&inlines_from_pandoc(inlines))
}

fn inlines_to_text_tm(inlines: &[TmInline]) -> String {
    let mut out = String::new();
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            TmInline::Text(t) => out.push_str(&t.value),
            TmInline::Raw(t) => out.push_str(&t.value),
            TmInline::SoftBreak(_) => {
                let before = out.chars().last();
                let after = inlines.get(idx + 1).and_then(TmInline::first_char);
                out.push_str(tomet_ast::softbreak_join(before, after));
            }
            TmInline::LineBreak(_) => out.push(' '),
            TmInline::Element(el) => {
                if let Some(content) = &el.content {
                    out.push_str(&blocks_to_text_tm(content));
                }
            }
            TmInline::Interp(_) | TmInline::Hole(_) => {}
        }
    }
    out
}

/// [`inlines_to_text_tm`] over `Element.content`'s `Vec<Block>` shape.
fn blocks_to_text_tm(blocks: &[TmBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            TmBlock::Paragraph(p) => out.push_str(&inlines_to_text_tm(&p.content)),
            TmBlock::Element(el) => {
                if let Some(content) = &el.content {
                    out.push_str(&blocks_to_text_tm(content));
                }
            }
            TmBlock::Section(sec) => out.push_str(&inlines_to_text_tm(&sec.title)),
            TmBlock::List(list) => {
                for item in &list.items {
                    out.push_str(&blocks_to_text_tm(&item.content));
                }
            }
            TmBlock::Interp(_) | TmBlock::Declaration(_) | TmBlock::Hole(_) => {}
        }
    }
    out
}

// ---- attributes ----------------------------------------------------

/// Rebuilds the element an `Attr` describes.
fn element_from_attr(attr: &Attr, fallback: impl FnOnce() -> String) -> Element {
    named_element(&class_name(attr).unwrap_or_else(fallback), attr)
}

/// Whether `attr` marks a `Div` as a bare `(marker)[content]` entry of the
/// `{...}` group around it.
///
/// A bare entry has no name to carry in a class, so [`crate::to_pandoc`]
/// records it under [`SIGIL_KEY`] instead. Reading it back is what keeps
/// `@deck.card{ (a)[ x ] }` from returning as an element named `bare` -- a
/// name no vocabulary declares, which made the round-tripped document fail
/// validation.
fn is_bare_entry(attr: &Attr) -> bool {
    attr.2
        .iter()
        .any(|(k, v)| k == SIGIL_KEY && v == BARE_SIGIL)
}

/// The body of a `Div` as the content and the bare entries it came from:
/// each block through [`block_from_pandoc`], except a `Div` marked as a bare
/// entry, which goes back into the `{...}` group it was written in.
fn body_from_pandoc(blocks: &[Block]) -> (Vec<TmBlock>, Vec<Bare>) {
    let mut content = Vec::new();
    let mut entries = Vec::new();
    for block in blocks {
        match block {
            Block::Div(attr, inner) if is_bare_entry(attr) => {
                let (inner_content, inner_entries) = body_from_pandoc(inner);
                let (args, value) = extract_data_from_attr(attr);
                let mut data = Data { args, value };
                push_bare_entries(&mut data, inner_entries);
                entries.push(Bare {
                    data,
                    content: (!inner_content.is_empty()).then_some(inner_content),
                    id: id_from_attr(attr),
                    span: Span::dummy(),
                });
            }
            other => content.extend(block_from_pandoc(other)),
        }
    }
    (content, entries)
}

/// Appends `entries` to the `{...}` group of `data`, opening one if there is
/// none yet.
fn push_bare_entries(data: &mut Data, entries: Vec<Bare>) {
    if entries.is_empty() {
        return;
    }
    let Group(group) = data.value.get_or_insert_with(|| Group(Vec::new()));
    group.extend(entries.into_iter().map(Entry::Bare));
}

/// The element name a `tomet-` class carries, if there is one.
fn class_name(attr: &Attr) -> Option<String> {
    attr.1
        .iter()
        .find_map(|c| c.strip_prefix(CLASS_PREFIX).map(|n| n.to_string()))
}

/// Builds an element of `name`, restoring whatever data its `Attr` holds.
///
/// When [`EXACT_DATA_KEY`] is present it wins outright: it is the whole
/// of the original `(args)` and `{value}`, so both groups come back
fn extract_data_from_attr(attr: &Attr) -> (Option<Value>, Option<Group>) {
    let exact = attr
        .2
        .iter()
        .find(|(k, _)| k == EXACT_DATA_KEY)
        .and_then(|(_, json)| serde_json::from_str(json).ok());
    if let Some(serde_json::Value::Object(obj)) = exact {
        let args = obj.get("args").map(json_to_value);
        let value = obj.get("value").map(|v| Group::from_map(json_to_value(v)));
        return (args, value);
    }

    let mut entries: Vec<(String, Value)> = Vec::new();
    for (k, v) in &attr.2 {
        if k == EXACT_DATA_KEY || k == SIGIL_KEY {
            continue;
        }
        entries.push((k.clone(), Value::String(v.clone())));
    }
    let value = if !entries.is_empty() {
        Some(Group::from_map(Value::Map(entries)))
    } else {
        None
    };
    (None, value)
}

/// exactly. Otherwise only the flat projection survived, and it all lands
/// in `{value}` -- `{}` is always data, and there is nothing left in the
/// attribute map to say which pair came from which group.
fn named_element(name: &str, attr: &Attr) -> Element {
    let mut el = element_new(parse_name(name));
    let (args, value) = extract_data_from_attr(attr);
    el.data.args = args;
    el.data.value = value;
    el.id = id_from_attr(attr);
    el
}

/// Pandoc's `Attr.0` identifier slot, round-tripped as the element/
/// section's own `#(id)` rather than as an `id:` data key.
fn id_from_attr(attr: &Attr) -> Option<Id> {
    if attr.0.is_empty() {
        None
    } else {
        Some(Id(attr.0.clone()))
    }
}

/// Adds a key to an element's `(args)`, keeping whatever is already there.
fn merge_arg(el: &mut Element, key: &str, value: Value) {
    match el.data.args.take() {
        Some(Value::Map(mut entries)) => {
            entries.retain(|(k, _)| k != key);
            entries.insert(0, (key.to_string(), value));
            el.data.args = Some(Value::Map(entries));
        }
        _ => el.data.args = Some(Value::Map(vec![(key.to_string(), value)])),
    }
}

/// Splits a dotted name the way the parser does: the last segment is the
/// name, everything before it the namespace.
fn parse_name(name: &str) -> Name {
    match name.rsplit_once('.') {
        Some((ns, last)) => Name::namespaced(ns, last),
        None => Name::bare(name),
    }
}

fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => Value::Seq(items.iter().map(json_to_value).collect()),
        serde_json::Value::Object(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

/// Kept honest: `value_to_json` and `json_to_value` must agree, or the
/// exact copy stops being exact.
#[cfg(test)]
fn assert_json_round_trip(v: &Value) {
    assert_eq!(&json_to_value(&tomet_semantics::value_to_json(v)), v);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_pandoc;
    use tomet_parser::parse_document;
    use tomet_printer::document_to_tm;

    /// Tomet source -> Pandoc -> Tomet source.
    fn round_trip(src: &str) -> String {
        let doc = parse_document(src).unwrap();
        document_to_tm(&from_pandoc(&to_pandoc(&doc)))
    }

    #[test]
    fn prose_and_headings_survive_a_round_trip() {
        let back = from_pandoc(&to_pandoc(
            &parse_document("=[ Title ]\n\n本文です。\n").unwrap(),
        ));
        let TmBlock::Section(sec) = &back.blocks[0] else {
            panic!("expected a section, got {:?}", back.blocks[0]);
        };
        assert_eq!(sec.level, 1);
        assert_eq!(sec.blocks.len(), 1);
        assert!(matches!(&sec.blocks[0], TmBlock::Paragraph(_)));
    }

    #[test]
    fn emphasis_survives_a_round_trip() {
        // Structure, not spelling: the printer re-emits an `em` element
        // as `@em[a]` rather than the `*a*` sugar it was written with.
        let back = from_pandoc(&to_pandoc(
            &parse_document("*a* と **b** です。\n").unwrap(),
        ));
        let TmBlock::Paragraph(p) = &back.blocks[0] else {
            panic!("expected a paragraph");
        };
        let names: Vec<String> = p
            .content
            .iter()
            .filter_map(|i| match i {
                TmInline::Element(el) => Some(el.name.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["em".to_string(), "strong".to_string()]);
    }

    #[test]
    fn a_backtick_span_survives_a_round_trip() {
        // It leaves Tomet as text, becomes a Pandoc `Code`, and has to
        // come back with its backticks.
        assert_eq!(round_trip("`spec/` を見る\n").trim(), "`spec/` を見る");
    }

    #[test]
    fn a_list_survives_a_round_trip() {
        assert_eq!(round_trip("- one\n- two\n").trim(), "- one\n- two");
    }

    #[test]
    fn a_link_survives_a_round_trip() {
        assert!(
            round_trip("文中の @link(target: \"https://e.com\")[Wiki] です。\n")
                .contains("@link(target: \"https://e.com\")[Wiki]")
        );
    }

    #[test]
    fn a_custom_element_keeps_its_name_and_placement() {
        let out = round_trip("@deck.card(a: 1)[ body ]\n");
        assert!(out.contains("@deck.card"), "got {out:?}");
        assert!(out.contains("body"), "got {out:?}");
    }

    #[test]
    fn the_exact_copy_restores_both_groups() {
        // Without `tomet-data` the two groups cannot be told apart on the
        // way back; with it, they come back exactly as written.
        let doc = parse_document("@deck.card(url: \"x\"){ id: c1, m: { k: v } }\n").unwrap();
        let back = from_pandoc(&to_pandoc(&doc));
        let TmBlock::Element(el) = &back.blocks[0] else {
            panic!("expected an element, got {:?}", back.blocks[0]);
        };
        assert_eq!(
            el.data.args,
            Some(Value::Map(vec![(
                "url".to_string(),
                Value::String("x".to_string())
            )]))
        );
        let value = el.data.value.as_ref().map(|v| v.as_data()).unwrap();
        assert_eq!(
            value,
            Value::Map(vec![
                ("id".to_string(), Value::String("c1".to_string())),
                (
                    "m".to_string(),
                    Value::Map(vec![("k".to_string(), Value::String("v".to_string()))])
                ),
            ])
        );
    }

    #[test]
    fn without_the_exact_copy_the_data_lands_in_the_value_group() {
        // The documented asymmetry: a flat `(args)` needs no exact copy,
        // so nothing records that it was `(args)` rather than `{value}`.
        let doc = parse_document("@deck.card(a: 1)\n").unwrap();
        let back = from_pandoc(&to_pandoc(&doc));
        let TmBlock::Element(el) = &back.blocks[0] else {
            panic!("expected an element");
        };
        assert_eq!(el.data.args, None);
        assert_eq!(
            el.data.value.as_ref().map(|v| v.as_data()),
            Some(Value::Map(vec![(
                "a".to_string(),
                Value::String("1".to_string())
            )]))
        );
    }

    #[test]
    fn meta_survives_a_round_trip() {
        let doc = parse_document("::meta{title: Hello, draft: true}\n\n本文\n").unwrap();
        let back = from_pandoc(&to_pandoc(&doc));
        let TmBlock::Declaration(declaration) = &back.blocks[0] else {
            panic!("expected the meta declaration");
        };
        assert!(declaration.name.is_bare_named("meta"));
        assert_eq!(
            declaration.data.value.as_ref().map(|v| v.as_data()),
            Some(Value::Map(vec![
                ("draft".to_string(), Value::Bool(true)),
                ("title".to_string(), Value::String("Hello".to_string())),
            ]))
        );
    }

    #[test]
    fn the_id_declaration_survives_a_round_trip() {
        let doc = parse_document("::id(doc-1)\n::meta{title: Hello}\n\n本文\n").unwrap();
        let back = from_pandoc(&to_pandoc(&doc));
        assert_eq!(
            tomet_declaration::document_id(&back).as_deref(),
            Some("doc-1")
        );
        assert!(
            tomet_declaration::document_meta(&back)
                .and_then(|meta| match meta {
                    Value::Map(entries) => Some(entries.iter().all(|(key, _)| key != "id")),
                    _ => None,
                })
                .unwrap_or(false)
        );
    }

    #[test]
    fn a_table_survives_a_round_trip() {
        let out = round_trip("@table()[\n[ h1 ][ h2 ]\n[ a ][ b ]\n]{}\n");
        assert!(out.contains("@table"), "got {out:?}");
        for cell in ["h1", "h2", "a", "b"] {
            assert!(out.contains(cell), "{cell} missing from {out:?}");
        }
    }

    #[test]
    fn a_block_inside_content_comes_back_block_placed() {
        let doc = parse_document("@deck.list[\n  @deck.item(1)[ one ]\n]\n").unwrap();
        let back = from_pandoc(&to_pandoc(&doc));
        let TmBlock::Element(outer) = &back.blocks[0] else {
            panic!("expected an element");
        };
        let inner = outer
            .content
            .as_ref()
            .and_then(|c| {
                c.iter().find_map(|b| match b {
                    TmBlock::Element(el) => Some(el),
                    _ => None,
                })
            })
            .expect("expected a nested element");
        assert_eq!(inner.placement, Placement::Block);
    }

    #[test]
    fn the_json_bridge_is_symmetric() {
        assert_json_round_trip(&Value::Null);
        assert_json_round_trip(&Value::Bool(true));
        assert_json_round_trip(&Value::Int(-7));
        assert_json_round_trip(&Value::String("x".into()));
        assert_json_round_trip(&Value::Seq(vec![Value::Int(1), Value::String("a".into())]));
        assert_json_round_trip(&Value::Map(vec![(
            "m".to_string(),
            Value::Map(vec![("k".to_string(), Value::Int(1))]),
        )]));
    }
}
