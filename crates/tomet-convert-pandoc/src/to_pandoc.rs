//! `tomet_ast::Document` -> Pandoc's AST.

use tomet_ast::{
    Block as TmBlock, Document, Element, ElementValue, Inline as TmInline, Placement, Sigil, Value,
};
use tomet_semantics::{
    ElementKind, TableRow, classify_std_lenient, document_meta, flatten_data, flatten_element_data,
    heading_level, is_directive, link_target, list_items, list_ordered, normalized_element_args,
    parse_table_rows, path_target,
};

use crate::ast::{
    Alignment, Attr, Block, Caption, Cell, ColSpec, ColWidth, Inline, ListAttributes, MetaValue,
    PandocDoc, QuoteType, Row, RowHeadColumns, TableBody, TableFoot, TableHead, TableParts, Target,
};

/// Converts a Tomet document to Pandoc's AST.
///
/// `@meta` feeds Pandoc's document-level `meta` map rather than being
/// dropped with the other directives. That map is what becomes YAML
/// frontmatter, `\title{}`, docx document properties and so on, so
/// `@meta{title: x}` survives into every output format. The sibling
/// writers have nowhere to put `@meta` and drop it; this is deliberately
/// not symmetric with them.
pub fn to_pandoc(doc: &Document) -> PandocDoc {
    let mut blocks = Vec::new();
    for block in &doc.blocks {
        blocks.extend(block_to_pandoc(block));
    }
    let mut out = PandocDoc::new(blocks);
    if let Some(Value::Map(entries)) = document_meta(doc) {
        out.meta = entries
            .iter()
            .map(|(k, v)| (k.clone(), meta_value(v)))
            .collect();
    }
    out
}

/// A Tomet value as Pandoc metadata.
///
/// Strings become `MetaInlines`, which is what Pandoc's own YAML
/// frontmatter reader produces and therefore the shape its writers are
/// built around -- a `title` given as `MetaString` renders in some
/// writers and not others.
///
/// Pandoc has no numeric metadata, so an `Int` or `Float` becomes a
/// string and comes back from `from_pandoc` as one. That is the one
/// lossy edge here, and step 8's round-trip snapshot is where it should
/// stay visible.
fn meta_value(v: &Value) -> MetaValue {
    match v {
        Value::Null => MetaValue::MetaString(String::new()),
        Value::Bool(b) => MetaValue::MetaBool(*b),
        Value::Int(i) => MetaValue::MetaString(i.to_string()),
        Value::Float(f) => MetaValue::MetaString(f.to_string()),
        Value::String(s) => MetaValue::MetaInlines(text_to_inlines(s)),
        Value::Seq(items) => MetaValue::MetaList(items.iter().map(meta_value).collect()),
        Value::Map(entries) => MetaValue::MetaMap(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), meta_value(v)))
                .collect(),
        ),
        // A call (e.g. `:rule`'s `allow:list(...)`) has no meaning as
        // document metadata; nothing writes one into `@meta` today.
        Value::Call(..) => MetaValue::MetaString(String::new()),
        // Same treatment as `Call`: Pandoc's `MetaValue` has no element
        // concept, and `@meta(icon: @doc.icon("x"))`'s whole point is that
        // nothing resolves or renders the embedded element anyway.
        Value::Element(_) => MetaValue::MetaString(String::new()),
    }
}

/// A document-level block. Yields nothing for a directive, and for a
/// paragraph that turns out to hold only directives -- otherwise every
/// `@meta` line would leave an empty `Para` behind, the same trap the
/// Markdown and HTML writers guard against.
fn block_to_pandoc(block: &TmBlock) -> Vec<Block> {
    match block {
        TmBlock::Paragraph(p) => content_to_blocks(&p.content),
        TmBlock::Element(el) => element_to_blocks(el),
        TmBlock::Section(sec) => {
            let mut blocks = vec![Block::Header(
                sec.level.max(1) as i64,
                section_attr(sec),
                inlines_to_pandoc(&sec.title),
            )];
            for child in &sec.blocks {
                blocks.extend(block_to_pandoc(child));
            }
            blocks
        }
    }
}

/// An element in block position.
fn element_to_blocks(el: &Element) -> Vec<Block> {
    let kind = classify_std_lenient(el);
    if is_directive(&kind) {
        return Vec::new();
    }
    match kind.as_str() {
        "heading" => vec![Block::Header(
            heading_level(el).unwrap_or(1) as i64,
            // The level *is* the args group, and it is already the first
            // field -- emitting it again as an attribute would put a
            // `tomet-data` copy on every heading.
            attr_from_value(el, &[]),
            content_to_inlines(el),
        )],
        "hr" => vec![Block::HorizontalRule],
        "raw" => vec![Block::CodeBlock(code_attr(el), content_to_plain_text(el))],
        "quote" => vec![Block::BlockQuote(content_to_blocks(content_of(el)))],
        "ol" | "ul" => vec![list_to_pandoc(el)],
        "table" => table_to_pandoc(el),
        // A path standing as a block is a listing row, so the
        // description goes beside the path rather than instead of it --
        // the same split the HTML/Markdown/Typst writers make. One
        // element serves a mention and a row; only the placement says
        // which, and only the row wants both halves shown.
        "file" | "dir" => {
            let mut inlines = vec![Inline::Code(
                attr_from_value(el, &[]),
                path_target(el, &kind).unwrap_or_default(),
            )];
            let content = content_to_inlines(el);
            if !content.is_empty() {
                inlines.push(Inline::Space);
                inlines.extend(content);
            }
            vec![Block::Para(inlines)]
        }
        // An inline-only kind can still stand alone on its line -- the
        // placement rule makes `@link(...)[x]` on its own line a block.
        // It keeps its own mapping and gets wrapped, rather than falling
        // through to the generic `Div` and losing its link-ness.
        "em" | "strong" | "mark" | "link" | "embed" | "interp" => {
            vec![Block::Para(element_to_inlines(el))]
        }
        // Everything else -- including an inline-only kind that somehow
        // stands as a block -- becomes a `Div` carrying its data. The
        // class is what lets `from_pandoc` recognise it again.
        _ => vec![Block::Div(generic_attr(el, &kind), element_body_blocks(el))],
    }
}

/// An element in inline position.
fn element_to_inlines(el: &Element) -> Vec<Inline> {
    let kind = classify_std_lenient(el);
    if is_directive(&kind) {
        return Vec::new();
    }
    match kind.as_str() {
        "em" => vec![Inline::Emph(content_to_inlines(el))],
        "strong" => vec![Inline::Strong(content_to_inlines(el))],
        "strikeout" => vec![Inline::Strikeout(content_to_inlines(el))],
        // Pandoc has no mark node. A `Span` with the class keeps it
        // recognisable in both directions, and writers that understand
        // classes (HTML, docx) can style it.
        "mark" => vec![Inline::Span(
            Attr::with_class("mark"),
            content_to_inlines(el),
        )],
        // Pandoc draws the same distinction Tomet draws by position:
        // `BlockQuote` above for one standing alone, `Quoted` here for
        // one inside a sentence. Double quotes, because that is what a
        // quotation without a stated style is.
        "quote" => vec![Inline::Quoted(
            QuoteType::DoubleQuote,
            content_to_inlines(el),
        )],
        "link" => vec![Inline::Link(
            attr_from_value(el, &[]),
            content_to_inlines(el),
            Target::url(link_target(el, &kind).unwrap_or_default()),
        )],
        // A path is named, not navigated to: `Code`, not `Link`. Pandoc's
        // `Code` is what a CommonMark code span reads back as, so this
        // matches what the Markdown writer emits.
        "file" | "dir" => vec![Inline::Code(
            attr_from_value(el, &[]),
            path_target(el, &kind).unwrap_or_default(),
        )],
        "embed" => vec![Inline::Image(
            attr_from_value(el, &[]),
            content_to_inlines(el),
            Target::url(link_target(el, &kind).unwrap_or_default()),
        )],
        // `${...}` is emitted as its own source text rather than
        // evaluated: resolving it needs the document and its config, and
        // the Typst writer already sets this precedent.
        "interp" => vec![Inline::Code(Attr::empty(), interp_source(el))],
        "raw" => vec![Inline::Code(code_attr(el), content_to_plain_text(el))],
        _ => vec![Inline::Span(
            generic_attr(el, &kind),
            content_to_inlines(el),
        )],
    }
}

// ---- content -------------------------------------------------------

fn content_of(el: &Element) -> &[TmInline] {
    el.content.as_deref().unwrap_or(&[])
}

/// A `[content]` group as Pandoc blocks.
///
/// `content` is a `Vec<Inline>` whatever it holds, so an element written
/// at a line start inside it -- `@references[` holding its entries -- is
/// an `Inline::Element` carrying `Placement::Block`. That is precisely
/// what placement exists to record, and this is where it is read: a
/// block-placed element becomes its own Pandoc block, and the true
/// inlines around it are gathered into `Plain`.
fn content_to_blocks(inlines: &[TmInline]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut run: Vec<Inline> = Vec::new();

    for item in inlines {
        match item {
            TmInline::Element(el) if el.placement == Placement::Block => {
                flush_run(&mut run, &mut blocks);
                blocks.extend(element_to_blocks(el));
            }
            TmInline::Element(el) => run.extend(element_to_inlines(el)),
            TmInline::Text(t) => run.extend(text_to_inlines(&t.value)),
            TmInline::Raw(t) => run.extend(text_to_inlines(&t.value)),
            TmInline::SoftBreak(_) => run.push(Inline::SoftBreak),
            TmInline::LineBreak(_) => run.push(Inline::LineBreak),
        }
    }
    flush_run(&mut run, &mut blocks);
    blocks
}

fn flush_run(run: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
    if run
        .iter()
        .any(|i| !matches!(i, Inline::Space | Inline::SoftBreak))
    {
        blocks.push(Block::Para(std::mem::take(run)));
    } else {
        run.clear();
    }
}

/// A `[content]` group where Pandoc demands inlines -- a heading's text, a
/// link's label, an emphasis span.
///
/// A block-placed element cannot be a `Div` here, so it degrades to a
/// `Span`. That only happens in documents that put a block inside a
/// heading, which the validator already reports as a shape mismatch.
fn content_to_inlines(el: &Element) -> Vec<Inline> {
    inlines_to_pandoc(content_of(el))
}

fn inlines_to_pandoc(inlines: &[TmInline]) -> Vec<Inline> {
    let mut out = Vec::new();
    for item in inlines {
        match item {
            TmInline::Text(t) => out.extend(text_to_inlines(&t.value)),
            TmInline::Raw(t) => out.extend(text_to_inlines(&t.value)),
            TmInline::SoftBreak(_) => out.push(Inline::SoftBreak),
            TmInline::LineBreak(_) => out.push(Inline::LineBreak),
            TmInline::Element(el) => out.extend(element_to_inlines(el)),
        }
    }
    out
}

/// Splits a text run into Pandoc's word-level inlines.
///
/// Pandoc does not carry runs of prose as single strings: whitespace is a
/// `Space` node and a line break inside a paragraph is a `SoftBreak`.
/// Writers rely on that to re-wrap, so emitting one big `Str` would be
/// accepted but would come back out badly formatted.
///
/// A tomet `Text.value` is guaranteed not to contain `'\n'`/`'\r'` (source
/// line breaks are `TmInline::SoftBreak`/`TmInline::LineBreak` nodes, mapped
/// 1:1 above, not characters) -- this function's own `'\n'` handling is only
/// live for the one caller that feeds it `RawText.value` instead, where a
/// literal newline can legitimately appear.
fn text_to_inlines(s: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // Tomet keeps `` `x` `` as literal text -- the parser only
            // shields the run from further markup -- so it arrives here
            // looking like prose. Pandoc has a real `Code` inline, and
            // using it is what stops its writers from escaping the
            // backticks back out as `\`x\``.
            '`' => match take_code_span(&mut chars) {
                Some(code) => {
                    push_word(&mut word, &mut out);
                    out.push(Inline::Code(Attr::empty(), code));
                }
                None => word.push('`'),
            },
            '\n' => {
                push_word(&mut word, &mut out);
                out.push(Inline::SoftBreak);
            }
            '\r' => {}
            c if c.is_whitespace() => {
                push_word(&mut word, &mut out);
                // Collapse consecutive whitespace, as Pandoc's own
                // readers do. A leading space is kept: a text run often
                // begins right after an element, and that space is the
                // one separating them.
                if !matches!(out.last(), Some(Inline::Space) | Some(Inline::SoftBreak)) {
                    out.push(Inline::Space);
                }
            }
            c => word.push(c),
        }
    }
    push_word(&mut word, &mut out);
    out
}

/// Consumes the rest of a backtick span, closing backtick excluded from
/// the returned text.
///
/// The pairing rule is the parser's: an opening backtick pairs with the
/// next one on the same line. Without a partner it is an ordinary
/// character.
fn take_code_span(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut look = chars.clone();
    let mut span = String::new();
    loop {
        match look.next()? {
            '`' => {
                *chars = look;
                return Some(span);
            }
            '\n' | '\r' => return None,
            c => span.push(c),
        }
    }
}

fn push_word(word: &mut String, out: &mut Vec<Inline>) {
    if !word.is_empty() {
        out.push(Inline::Str(std::mem::take(word)));
    }
}

/// An element's own body as blocks: its `[content]`, then any elements
/// its `{...}` group holds as children.
fn element_body_blocks(el: &Element) -> Vec<Block> {
    let mut blocks = content_to_blocks(content_of(el));
    if let Some(value) = el.value.as_ref() {
        for child in value.as_children() {
            blocks.extend(element_to_blocks(child));
        }
    }
    blocks
}

/// Content flattened to plain text, for slots that cannot hold markup --
/// a code block's body.
fn content_to_plain_text(el: &Element) -> String {
    match el.value.as_ref() {
        // A `+++` fence body is already verbatim text.
        Some(ElementValue::Raw(raw)) => raw.clone(),
        _ => plain_text(content_of(el)),
    }
}

fn plain_text(inlines: &[TmInline]) -> String {
    let mut out = String::new();
    for (idx, item) in inlines.iter().enumerate() {
        match item {
            TmInline::Text(t) => out.push_str(&t.value),
            TmInline::Raw(t) => out.push_str(&t.value),
            TmInline::SoftBreak(_) => {
                let before = out.chars().last();
                let after = inlines.get(idx + 1).and_then(TmInline::first_char);
                out.push_str(tomet_ast::softbreak_join(before, after));
            }
            TmInline::LineBreak(_) => out.push('\n'),
            TmInline::Element(el) => out.push_str(&plain_text(content_of(el))),
        }
    }
    out
}

fn interp_source(el: &Element) -> String {
    match &el.value {
        Some(ElementValue::Interp(expr)) => format!("${{{expr}}}"),
        _ => String::new(),
    }
}

// ---- lists ---------------------------------------------------------

fn list_to_pandoc(el: &Element) -> Block {
    let items: Vec<Vec<Block>> = list_items(el)
        .iter()
        .map(|item| {
            let mut blocks = content_to_blocks(content_of(item));
            // A list item's own content is a single logical line, so
            // Pandoc's `Plain` is a better fit than `Para` -- it is what
            // keeps a tight list tight.
            for block in &mut blocks {
                if let Block::Para(inlines) = block {
                    *block = Block::Plain(std::mem::take(inlines));
                }
            }
            if let Some(children) = &item.children {
                for child in children {
                    blocks.extend(block_to_pandoc(child));
                }
            }
            blocks
        })
        .collect();

    if list_ordered(el).unwrap_or(false) {
        Block::OrderedList(ListAttributes::default(), items)
    } else {
        Block::BulletList(items)
    }
}

// ---- tables --------------------------------------------------------

/// A `@table` as a Pandoc `Table`.
///
/// Pandoc's table carries far more than Tomet's: a caption, per-column
/// specs, and a head/bodies/foot split whose rows can span. Tomet has a
/// grid and an optional header row, so most of this is filling in
/// defaults -- but Pandoc rejects the JSON unless the shapes are exact.
///
/// The one place it can do *better* than the sibling writers: `align:`
/// becomes a real `ColSpec` alignment. The Markdown and Typst writers
/// both ignore that argument today, because neither target has a good
/// place to put it.
fn table_to_pandoc(el: &Element) -> Vec<Block> {
    let rows = parse_table_rows(content_of(el));
    let col_count = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    let colspec = ColSpec(table_alignment(el), ColWidth::ColWidthDefault);
    let colspecs = vec![colspec; col_count];

    // Row 0 is the header unless `header: false` says otherwise -- the
    // same convention the Typst writer follows.
    let has_header = table_has_header(el);
    let (head, body) = if has_header {
        (&rows[..1], &rows[1..])
    } else {
        (&rows[..0], &rows[..])
    };

    let to_rows = |rows: &[TableRow]| -> Vec<Row> {
        rows.iter()
            .map(|row| {
                let cells = (0..col_count)
                    .map(|i| {
                        let content = row.cells.get(i).map(|c| c.content.as_slice());
                        // `Plain`, not `Para`: a table cell is not a
                        // paragraph, and Pandoc's writers space them
                        // differently.
                        let blocks = match content {
                            Some(inlines) if !inlines.is_empty() => {
                                vec![Block::Plain(inlines_to_pandoc(inlines))]
                            }
                            _ => Vec::new(),
                        };
                        Cell(Attr::empty(), Alignment::AlignDefault, 1, 1, blocks)
                    })
                    .collect();
                Row(Attr::empty(), cells)
            })
            .collect()
    };

    vec![Block::Table(Box::new(TableParts(
        // `align:`/`header:` are consumed above, so the attr comes from
        // `{value}` alone.
        attr_from_value(el, &[]),
        Caption(None, Vec::new()),
        colspecs,
        TableHead(Attr::empty(), to_rows(head)),
        vec![TableBody(
            Attr::empty(),
            RowHeadColumns(0),
            Vec::new(),
            to_rows(body),
        )],
        TableFoot(Attr::empty(), Vec::new()),
    )))]
}

/// Reads one of `@table`'s own arguments, after positional
/// normalization so `@table(center)` and `@table(align: center)` agree.
fn table_arg(el: &Element, key: &str) -> Option<Value> {
    let Value::Map(entries) = normalized_element_args(el)? else {
        return None;
    };
    entries.into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn table_has_header(el: &Element) -> bool {
    match table_arg(el, "header") {
        Some(Value::Bool(b)) => b,
        Some(_) => true,
        None => true,
    }
}

fn table_alignment(el: &Element) -> Alignment {
    match table_arg(el, "align").as_ref().and_then(|v| match v {
        Value::String(s) => Some(s.as_str().to_ascii_lowercase()),
        _ => None,
    }) {
        Some(a) if a == "left" => Alignment::AlignLeft,
        Some(a) if a == "center" || a == "centre" => Alignment::AlignCenter,
        Some(a) if a == "right" => Alignment::AlignRight,
        _ => Alignment::AlignDefault,
    }
}

// ---- attributes ----------------------------------------------------

/// The attribute key that records a sigil with no name of its own.
///
/// A `Sigil::Bare` entry has no name. Putting one in the `tomet-<name>`
/// class anyway -- `ElementKind::Bare::as_str()` is `"bare"` -- made
/// `from_pandoc` rebuild it as an element *named* `bare`, and `@bare` is
/// not a builtin, so a round-tripped document stopped validating.
///
/// A key rather than another class, because a class collides: a document
/// whose `@kind` names a vocabulary declaring `bare` writes `@bare`
/// unqualified, and `tomet-bare` would then mean two things. `tomet-data`
/// established the reserved-key convention this follows.
pub(crate) const SIGIL_KEY: &str = "tomet-sigil";

/// [`SIGIL_KEY`]'s value for `Sigil::Bare`.
pub(crate) const BARE_SIGIL: &str = "bare";

/// The `Attr` for an element Pandoc has no equivalent for: its data, plus
/// either the class carrying its name or the key saying it has none.
fn generic_attr(el: &Element, kind: &ElementKind) -> Attr {
    if matches!(el.sigil, Sigil::Bare) {
        let mut attr = attr_of(el, &[]);
        attr.2.push((SIGIL_KEY.to_string(), BARE_SIGIL.to_string()));
        return attr;
    }
    attr_of(el, &[&format!("tomet-{}", kind.as_str())])
}

/// Builds a Pandoc `Attr` from an element's data.
///
/// `id` is lifted out of the flattened pairs into the identifier slot,
/// which is where Pandoc's writers look for an anchor. Everything else
/// follows the shared flattening rule: readable pairs, plus the exact
/// JSON copy under `EXACT_DATA_KEY` when the projection would lose
/// something.
fn attr_of(el: &Element, classes: &[&str]) -> Attr {
    attr_from(flatten_element_data(el).into_pairs(), classes)
}

fn section_attr(sec: &tomet_ast::Section) -> Attr {
    let args = sec.args.as_ref();
    let value = sec.value.as_ref().and_then(|v| v.as_data());
    attr_from(flatten_data(args, value.as_ref()).into_pairs(), &[])
}

/// Like [`attr_of`], but ignoring `(args)`.
///
/// For the builtins whose args the mapping already consumed into a
/// dedicated field: a heading's level, a link's or image's target.
fn attr_from_value(el: &Element, classes: &[&str]) -> Attr {
    let value = el.value.as_ref().and_then(|v| v.as_data());
    attr_from(flatten_data(None, value.as_ref()).into_pairs(), classes)
}

fn attr_from(mut pairs: Vec<(String, String)>, classes: &[&str]) -> Attr {
    let mut id = String::new();
    if let Some(pos) = pairs.iter().position(|(k, _)| k == "id") {
        id = pairs.remove(pos).1;
    }
    Attr(id, classes.iter().map(|c| c.to_string()).collect(), pairs)
}

/// A code block's attr: the language becomes a class, which is how every
/// Pandoc writer decides how to highlight it.
fn code_attr(el: &Element) -> Attr {
    let mut attr = attr_of(el, &[]);
    if let Some(pos) = attr.2.iter().position(|(k, _)| k == "lang") {
        let (_, lang) = attr.2.remove(pos);
        if !lang.is_empty() {
            attr.1.push(lang);
        }
    }
    attr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::TableParts;
    use tomet_parser::parse_document;
    use tomet_semantics::EXACT_DATA_KEY;

    fn convert(src: &str) -> Vec<Block> {
        to_pandoc(&parse_document(src).unwrap()).blocks
    }

    #[test]
    fn a_paragraph_becomes_word_level_inlines() {
        assert_eq!(
            convert("hello world\n"),
            vec![Block::Para(vec![
                Inline::Str("hello".into()),
                Inline::Space,
                Inline::Str("world".into()),
            ])]
        );
    }

    #[test]
    fn a_wrapped_line_becomes_a_softbreak() {
        // `tomet_ast::Inline::SoftBreak` maps 1:1 to Pandoc's own
        // `SoftBreak` (`content_to_blocks`) -- no longer `Space`, now that
        // the parser keeps a source line break as its own node instead of
        // folding it away before the AST is even built.
        let blocks = convert("one\ntwo\n");
        assert_eq!(
            blocks,
            vec![Block::Para(vec![
                Inline::Str("one".into()),
                Inline::SoftBreak,
                Inline::Str("two".into()),
            ])]
        );
        assert_eq!(
            text_to_inlines("a\nb"),
            vec![
                Inline::Str("a".into()),
                Inline::SoftBreak,
                Inline::Str("b".into())
            ]
        );
    }

    #[test]
    fn a_backtick_span_becomes_a_code_inline() {
        // Tomet leaves the backticks in the text, so without this they
        // would travel as a `Str` and come back out of Pandoc's writers
        // escaped.
        assert_eq!(
            convert("`spec/` を見る\n"),
            vec![Block::Para(vec![
                Inline::Code(Attr::empty(), "spec/".into()),
                Inline::Space,
                Inline::Str("を見る".into()),
            ])]
        );
        // Unpaired: an ordinary character.
        assert_eq!(
            convert("100` です\n"),
            vec![Block::Para(vec![
                Inline::Str("100`".into()),
                Inline::Space,
                Inline::Str("です".into()),
            ])]
        );
    }

    #[test]
    fn a_heading_carries_its_level() {
        assert_eq!(
            convert("==[ Section ]\n"),
            vec![Block::Header(
                2,
                Attr::empty(),
                vec![Inline::Str("Section".into())]
            )]
        );
    }

    #[test]
    fn emphasis_maps_to_pandoc_nodes() {
        assert_eq!(
            convert("*a* **b** @mark[c]\n"),
            vec![Block::Para(vec![
                Inline::Emph(vec![Inline::Str("a".into())]),
                Inline::Space,
                Inline::Strong(vec![Inline::Str("b".into())]),
                Inline::Space,
                Inline::Span(Attr::with_class("mark"), vec![Inline::Str("c".into())]),
            ])]
        );
    }

    #[test]
    fn a_link_becomes_a_link_node() {
        assert_eq!(
            convert("@link(target: \"https://example.com\")[Wiki]\n"),
            vec![Block::Para(vec![Inline::Link(
                Attr::empty(),
                vec![Inline::Str("Wiki".into())],
                Target("https://example.com".into(), String::new()),
            )])]
        );
    }

    #[test]
    fn meta_feeds_the_document_metadata_map() {
        let doc =
            parse_document("@meta{title: Hello, draft: true, tags: list(a, b)}\n\n本文\n").unwrap();
        let pandoc = to_pandoc(&doc);
        assert_eq!(
            pandoc.meta.get("title"),
            Some(&MetaValue::MetaInlines(vec![Inline::Str("Hello".into())]))
        );
        assert_eq!(pandoc.meta.get("draft"), Some(&MetaValue::MetaBool(true)));
        assert_eq!(
            pandoc.meta.get("tags"),
            Some(&MetaValue::MetaList(vec![
                MetaValue::MetaInlines(vec![Inline::Str("a".into())]),
                MetaValue::MetaInlines(vec![Inline::Str("b".into())]),
            ]))
        );
        // ...and it still produces no body block.
        assert_eq!(pandoc.blocks.len(), 1);

        // list(...) call literal converts identically to [a, b]
        let doc_call =
            parse_document("@meta{title: Hello, draft: true, tags: list(a, b)}\n\n本文\n").unwrap();
        let pandoc_call = to_pandoc(&doc_call);
        assert_eq!(
            pandoc_call.meta.get("tags"),
            Some(&MetaValue::MetaList(vec![
                MetaValue::MetaInlines(vec![Inline::Str("a".into())]),
                MetaValue::MetaInlines(vec![Inline::Str("b".into())]),
            ]))
        );
    }

    #[test]
    fn a_meta_number_becomes_a_string() {
        // Pandoc metadata has no numeric type. Recorded rather than
        // hidden: this is the one lossy edge of the meta mapping.
        let doc = parse_document("@meta{year: 2026}\n").unwrap();
        assert_eq!(
            to_pandoc(&doc).meta.get("year"),
            Some(&MetaValue::MetaString("2026".into()))
        );
    }

    #[test]
    fn directives_produce_nothing() {
        assert!(convert("@meta{type: note}\n").is_empty());
        assert!(convert("@settings(file:project.settings.tmt)\n").is_empty());
        // ...and a paragraph that held only directives leaves no empty
        // `Para` behind either.
        assert!(convert("@meta{a: 1} @meta{b: 2}\n").is_empty());
    }

    #[test]
    fn a_custom_element_becomes_a_div_or_a_span_by_placement() {
        // Alone on its line: a block.
        let blocks = convert("@deck.card(a: 1)[ body ]\n");
        let Block::Div(attr, body) = &blocks[0] else {
            panic!("expected a Div, got {:?}", blocks[0]);
        };
        assert_eq!(attr.1, vec!["tomet-deck.card".to_string()]);
        assert_eq!(attr.2, vec![("a".to_string(), "1".to_string())]);
        assert_eq!(body, &vec![Block::Para(vec![Inline::Str("body".into())])]);

        // Mid-sentence: a span.
        let blocks = convert("文中の @deck.badge(2)[印] です。\n");
        let Block::Para(inlines) = &blocks[0] else {
            panic!("expected a Para, got {:?}", blocks[0]);
        };
        assert!(
            inlines
                .iter()
                .any(|i| matches!(i, Inline::Span(attr, _) if attr.1 == ["tomet-deck.badge"])),
            "got {inlines:?}"
        );
    }

    #[test]
    fn a_block_placed_element_inside_content_becomes_its_own_block() {
        // The case `Placement` was introduced for: `content` is a
        // `Vec<Inline>`, so only placement can say this is a block.
        let blocks = convert("@deck.list[\n  @deck.item(1)[ one ]\n]\n");
        let Block::Div(_, body) = &blocks[0] else {
            panic!("expected a Div, got {:?}", blocks[0]);
        };
        assert!(
            matches!(&body[0], Block::Div(attr, _) if attr.1 == ["tomet-deck.item"]),
            "got {body:?}"
        );
    }

    #[test]
    fn an_id_goes_in_the_identifier_slot_not_the_pairs() {
        let blocks = convert("@deck.card{ id: c1, tags: list(a, b) }\n");
        let Block::Div(attr, _) = &blocks[0] else {
            panic!("expected a Div, got {:?}", blocks[0]);
        };
        assert_eq!(attr.0, "c1");
        assert_eq!(attr.2, vec![("tags".to_string(), "a, b".to_string())]);
    }

    #[test]
    fn a_nested_value_is_carried_by_the_exact_copy() {
        let blocks = convert("@deck.card{ a: 1, m: { k: v } }\n");
        let Block::Div(attr, _) = &blocks[0] else {
            panic!("expected a Div, got {:?}", blocks[0]);
        };
        assert_eq!(attr.2[0], ("a".to_string(), "1".to_string()));
        assert_eq!(attr.2[1].0, EXACT_DATA_KEY);
        assert!(
            attr.2[1].1.contains(r#""m":{"k":"v"}"#),
            "got {:?}",
            attr.2[1].1
        );
    }

    #[test]
    fn a_code_block_puts_its_language_in_a_class() {
        let blocks = convert("```rust\nfn main() {}\n```\n");
        let Block::CodeBlock(attr, code) = &blocks[0] else {
            panic!("expected a CodeBlock, got {:?}", blocks[0]);
        };
        assert_eq!(attr.1, vec!["rust".to_string()]);
        assert_eq!(code.trim(), "fn main() {}");
    }

    #[test]
    fn lists_map_to_pandoc_lists() {
        assert_eq!(
            convert("- one\n- two\n"),
            vec![Block::BulletList(vec![
                vec![Block::Plain(vec![Inline::Str("one".into())])],
                vec![Block::Plain(vec![Inline::Str("two".into())])],
            ])]
        );
        assert!(matches!(convert("-. one\n")[0], Block::OrderedList(_, _)));
    }

    #[test]
    fn a_table_becomes_a_pandoc_table() {
        let blocks = convert("@table()[\n[ h1 ][ h2 ]\n[ a ][ b ]\n]{}\n");
        let Block::Table(parts) = &blocks[0] else {
            panic!("expected a Table, got {:?}", blocks[0]);
        };
        let TableParts(_, _, colspecs, head, bodies, foot) = parts.as_ref();

        assert_eq!(colspecs.len(), 2);
        // Row 0 is the header by default.
        assert_eq!(head.1.len(), 1);
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0].3.len(), 1);
        assert!(foot.1.is_empty());

        let Row(_, cells) = &head.1[0];
        assert_eq!(cells.len(), 2);
        assert_eq!(
            cells[0].4,
            vec![Block::Plain(vec![Inline::Str("h1".into())])]
        );
    }

    #[test]
    fn header_false_puts_every_row_in_the_body() {
        let blocks = convert("@table(header: false)[\n[ a ][ b ]\n[ c ][ d ]\n]{}\n");
        let Block::Table(parts) = &blocks[0] else {
            panic!("expected a Table, got {:?}", blocks[0]);
        };
        assert!(parts.3.1.is_empty(), "head should be empty");
        assert_eq!(parts.4[0].3.len(), 2, "both rows belong to the body");
    }

    #[test]
    fn align_reaches_the_column_spec() {
        // Pandoc has real column alignment, so `align:` survives here --
        // the Markdown and Typst writers both drop it.
        let blocks = convert("@table(align: \"center\")[\n[ a ][ b ]\n]{}\n");
        let Block::Table(parts) = &blocks[0] else {
            panic!("expected a Table, got {:?}", blocks[0]);
        };
        assert_eq!(parts.2[0].0, Alignment::AlignCenter);
    }

    #[test]
    fn a_short_row_is_padded_to_the_column_count() {
        let blocks = convert("@table()[\n[ h1 ][ h2 ][ h3 ]\n[ a ]\n]{}\n");
        let Block::Table(parts) = &blocks[0] else {
            panic!("expected a Table, got {:?}", blocks[0]);
        };
        let Row(_, cells) = &parts.4[0].3[0];
        assert_eq!(cells.len(), 3);
        assert!(cells[2].4.is_empty(), "the padding cell has no content");
    }

    #[test]
    fn an_empty_table_produces_no_block() {
        assert!(convert("@table()[]{}\n").is_empty());
    }

    #[test]
    fn a_thematic_break_is_a_horizontal_rule() {
        assert_eq!(convert("---\n"), vec![Block::HorizontalRule]);
    }

    #[test]
    fn the_whole_document_serializes_to_pandoc_json() {
        let doc = parse_document("#[ T ]\n\n本文です。\n").unwrap();
        let json = serde_json::to_string(&to_pandoc(&doc)).unwrap();
        assert!(json.starts_with(r#"{"pandoc-api-version":[1,23,1],"meta":{},"blocks":["#));
    }
}
