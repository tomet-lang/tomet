//! Generic recursive traversal of a [`tomet_ast::Document`]'s tree.

use std::ops::ControlFlow;
use tomet_ast::{
    Block, Connect, Data, Declaration, Document, Element, Entry, Group, Hole, Inline,
    Interpolation, List, ListItem, Value,
};

/// Called at every [`Element`] `walk_document` visits, in document order.
pub trait Visitor<B> {
    fn visit(&mut self, el: &Element) -> ControlFlow<B>;
}

impl<F, B> Visitor<B> for F
where
    F: FnMut(&Element) -> ControlFlow<B>,
{
    fn visit(&mut self, el: &Element) -> ControlFlow<B> {
        self(el)
    }
}

/// Traverses all elements in `doc` in document order and executes `f` for each.
pub fn for_each_element(doc: &Document, mut f: impl FnMut(&Element)) {
    let mut visitor = |el: &Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_document(doc, &mut visitor);
}

/// Traverses every [`Inline`] in `doc` in document order: paragraph
/// content, section titles, and every element's `[content]`.
///
/// It covers exactly the positions [`walk_document`] covers -- an element
/// reached there has its inlines reached here -- so a count taken over
/// elements and a count taken over text describe the same part of the tree.
/// An `Inline::Element` is itself passed to `f`, and then descended into.
pub fn for_each_inline(doc: &Document, mut f: impl FnMut(&Inline)) {
    for block in &doc.blocks {
        inlines_in_block(block, &mut f);
    }
}

fn inlines_in_block(block: &Block, f: &mut dyn FnMut(&Inline)) {
    match block {
        Block::Paragraph(paragraph) => inlines_in(&paragraph.content, f),
        Block::Element(element) => inlines_in_element(element, f),
        Block::Declaration(d) => inlines_in_groups(&d.data, d.content.as_deref(), f),
        Block::Hole(h) => inlines_in_groups(&h.data, h.content.as_deref(), f),
        Block::Section(section) => {
            inlines_in(&section.title, f);
            // `section.connects` is deliberately not walked here -- same
            // reason `inlines_in_element` never looks at an ordinary
            // element's own `.connects`: a connect is metadata about its
            // owner, not reachable content (see `for_each_descendant`'s
            // doc comment in this file).
            for child in &section.blocks {
                inlines_in_block(child, f);
            }
        }
        Block::List(list) => inlines_in_list_items(&list.items, f),
        Block::Interp(_) => {}
    }
}

fn inlines_in_list_items(items: &[ListItem], f: &mut dyn FnMut(&Inline)) {
    for item in items {
        inlines_in_element(&item.element, f);
        if let Some(sub) = &item.sublist {
            inlines_in_list_items(&sub.items, f);
        }
    }
}

fn inlines_in(inlines: &[Inline], f: &mut dyn FnMut(&Inline)) {
    for inline in inlines {
        f(inline);
        match inline {
            Inline::Element(element) => inlines_in_element(element, f),
            Inline::Hole(hole) => inlines_in_groups(&hole.data, hole.content.as_deref(), f),
            _ => {}
        }
    }
}

fn inlines_in_element(element: &Element, f: &mut dyn FnMut(&Inline)) {
    inlines_in_groups(&element.data, element.content.as_deref(), f);
}

/// `(args)`, `[content]` and `{value}`, in that order: what an element and a
/// declaration have in common.
fn inlines_in_groups(data: &Data, content: Option<&[Block]>, f: &mut dyn FnMut(&Inline)) {
    if let Some(args) = &data.args {
        inlines_in_value(args, f);
    }
    for block in content.into_iter().flatten() {
        inlines_in_block(block, f);
    }
    if let Some(Group(entries)) = &data.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => inlines_in_element(child, f),
                Entry::Hole(hole) => inlines_in_groups(&hole.data, hole.content.as_deref(), f),
                Entry::Pair(_, v) => inlines_in_value(v, f),
            }
        }
    }
}

fn inlines_in_value(value: &Value, f: &mut dyn FnMut(&Inline)) {
    match value {
        Value::Element(el) => inlines_in_element(el, f),
        Value::Hole(hole) => inlines_in_groups(&hole.data, hole.content.as_deref(), f),
        Value::Seq(items) => items.iter().for_each(|item| inlines_in_value(item, f)),
        Value::Map(entries) => entries.iter().for_each(|(_, v)| inlines_in_value(v, f)),
        Value::Call(_, args) => args.iter().for_each(|a| inlines_in_value(a, f)),
        Value::Blocks(blocks) => blocks.iter().for_each(|b| inlines_in_block(b, f)),
        Value::Null
        | Value::Interp(_)
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Number(_)
        | Value::String(_) => {}
    }
}

/// [`for_each_element`] over one [`Block`], for callers that walk the
/// document's own top level themselves and need the elements below it.
pub fn for_each_element_in_block(block: &Block, mut f: impl FnMut(&Element)) {
    let mut visitor = |el: &Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_block(block, &mut visitor);
}

/// Visits every element a reader would find by scanning only the top of the
/// document, in document order: every `Block::Element`, and every
/// `Inline::Element` sitting directly in a `Block::Paragraph`'s own content
/// (an element joined into a paragraph by an explicit `\` continuation
/// trigger is still at the top; something nested inside another element is
/// not).
///
/// This is the one place that decision lives. A caller that wants "the
/// elements at the top of this document" should call this instead of walking
/// `doc.blocks` by hand. Declarations are not elements: see
/// [`for_each_top_level_declaration`].
pub fn for_each_top_level_element(doc: &Document, mut f: impl FnMut(&Element)) {
    for block in &doc.blocks {
        match block {
            Block::Element(el) => f(el),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(el);
                    }
                }
            }
            Block::Section(_)
            | Block::List(_)
            | Block::Declaration(_)
            | Block::Hole(_)
            | Block::Interp(_) => {}
        }
    }
}

/// Visits every declaration at the top level of `doc`, in order.
///
/// A declaration is always a [`Block`] of its own, so "the declarations at
/// the top of this document" is `doc.blocks` and nothing else: one in a
/// section's body or an element's content is misplaced, and the validator
/// reports it (`OutsidePreamble`). A caller that wants the preamble should
/// call this instead of walking `doc.blocks` by hand.
pub fn for_each_top_level_declaration(doc: &Document, mut f: impl FnMut(&Declaration)) {
    for block in &doc.blocks {
        if let Block::Declaration(declaration) = block {
            f(declaration);
        }
    }
}

/// [`for_each_top_level_declaration`], mutably -- for a caller that rewrites a
/// top-level declaration in place (`::blueprint(x)` becoming `::kind(x)`,
/// say) without changing how many there are.
pub fn for_each_top_level_declaration_mut(doc: &mut Document, mut f: impl FnMut(&mut Declaration)) {
    for block in &mut doc.blocks {
        if let Block::Declaration(declaration) = block {
            f(declaration);
        }
    }
}

/// [`for_each_top_level_declaration`], removing every one `keep` rejects.
pub fn retain_top_level_declarations(
    doc: &mut Document,
    mut keep: impl FnMut(&Declaration) -> bool,
) {
    doc.blocks.retain(|block| match block {
        Block::Declaration(declaration) => keep(declaration),
        _ => true,
    });
}

macro_rules! propagate {
    ($e:expr) => {
        match $e {
            ControlFlow::Continue(()) => {}
            broke @ ControlFlow::Break(_) => return broke,
        }
    };
}

/// Depth-first, document-order walk of every `Element` in `doc`.
pub fn walk_document<B>(doc: &Document, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for block in &doc.blocks {
        propagate!(walk_block(block, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_block<B>(block: &Block, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    match block {
        Block::Paragraph(paragraph) => walk_inlines(&paragraph.content, visitor),
        Block::Element(element) => walk_element(element, visitor),
        // A declaration is not an element, so the visitor does not see it, but
        // the elements inside its groups are the document's like any other.
        Block::Declaration(d) => walk_groups(&d.data, d.content.as_deref(), visitor),
        // Neither is a hole, in a pattern: not an element, but its groups hold some.
        Block::Hole(h) => walk_groups(&h.data, h.content.as_deref(), visitor),
        Block::Section(section) => {
            propagate!(walk_inlines(&section.title, visitor));
            // `section.connects` is deliberately not walked here -- same
            // reason `walk_element` never descends into an ordinary
            // element's own `.connects` (see `for_each_descendant`'s doc
            // comment): a connect is metadata about its owner, not a
            // document element in its own right, and feeding it through
            // here is what let `rule` in `=[ h ]:rule(...)` reach the same
            // "is this name bound" classification a real element gets.
            for child in &section.blocks {
                propagate!(walk_block(child, visitor));
            }
            ControlFlow::Continue(())
        }
        // `list.connects` deliberately not walked here either -- same
        // reason as `section.connects` above.
        Block::List(list) => walk_list_items(list, visitor),
        // An interpolation is not an element: no element walker visits it.
        Block::Interp(_) => ControlFlow::Continue(()),
    }
}

fn walk_list_items<B>(list: &List, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for item in &list.items {
        propagate!(walk_element(&item.element, visitor));
        if let Some(sub) = &item.sublist {
            propagate!(walk_list_items(sub, visitor));
        }
    }
    ControlFlow::Continue(())
}

fn walk_inlines<B>(inlines: &[Inline], visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for inline in inlines {
        match inline {
            Inline::Element(element) => propagate!(walk_element(element, visitor)),
            Inline::Hole(hole) => {
                propagate!(walk_groups(&hole.data, hole.content.as_deref(), visitor))
            }
            _ => {}
        }
    }
    ControlFlow::Continue(())
}

fn walk_element<B>(element: &Element, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    propagate!(visitor.visit(element));
    walk_groups(&element.data, element.content.as_deref(), visitor)
}

/// `(args)`, `[content]` and `{value}`, in that order: what an element and a
/// declaration have in common.
fn walk_groups<B>(
    data: &Data,
    content: Option<&[Block]>,
    visitor: &mut impl Visitor<B>,
) -> ControlFlow<B> {
    if let Some(args) = &data.args {
        propagate!(walk_value(args, visitor));
    }
    for block in content.into_iter().flatten() {
        propagate!(walk_block(block, visitor));
    }
    if let Some(Group(entries)) = &data.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => propagate!(walk_element(child, visitor)),
                Entry::Hole(hole) => {
                    propagate!(walk_groups(&hole.data, hole.content.as_deref(), visitor))
                }
                Entry::Pair(_, v) => propagate!(walk_value(v, visitor)),
            }
        }
    }
    ControlFlow::Continue(())
}

/// Descends into a [`Value`] for any [`Value::Element`] nested inside it --
/// directly, or under a [`Value::Seq`]/[`Value::Map`]/[`Value::Call`] that
/// contains one. Every other position an element can sit in (`[content]`,
/// `{value}`'s bare [`Entry::Element`], `children`) already had a walker
/// before `Value::Element` existed; this is the one `element.args` and
/// `Entry::Pair`'s value needed, since neither was ever visited at all
/// while `Value` had no element-shaped variant to visit.
fn walk_value<B>(value: &Value, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    match value {
        Value::Element(el) => walk_element(el, visitor),
        Value::Hole(hole) => walk_groups(&hole.data, hole.content.as_deref(), visitor),
        Value::Seq(items) => {
            for item in items {
                propagate!(walk_value(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                propagate!(walk_value(v, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Call(_, args) => {
            for a in args {
                propagate!(walk_value(a, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Blocks(blocks) => {
            for block in blocks {
                propagate!(walk_block(block, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Null
        | Value::Interp(_)
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Number(_)
        | Value::String(_) => ControlFlow::Continue(()),
    }
}

/// Mutable counterpart of [`Visitor`] -- called at every `&mut Element`
/// in document order.
pub trait VisitorMut<B> {
    fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<B>;
}

impl<F, B> VisitorMut<B> for F
where
    F: FnMut(&mut Element) -> ControlFlow<B>,
{
    fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<B> {
        self(el)
    }
}

/// Traverses all elements in `doc` mutably in document order and executes `f` for each in place.
pub fn for_each_element_mut(doc: &mut Document, mut f: impl FnMut(&mut Element)) {
    let mut visitor = |el: &mut Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_document_mut(doc, &mut visitor);
}

/// Mutable counterpart of [`walk_document`].
pub fn walk_document_mut<B>(
    doc: &mut Document,
    visitor: &mut impl VisitorMut<B>,
) -> ControlFlow<B> {
    for block in &mut doc.blocks {
        propagate!(walk_block_mut(block, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_block_mut<B>(block: &mut Block, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    match block {
        Block::Paragraph(paragraph) => walk_inlines_mut(&mut paragraph.content, visitor),
        Block::Element(element) => walk_element_mut(element, visitor),
        Block::Declaration(d) => walk_groups_mut(&mut d.data, d.content.as_deref_mut(), visitor),
        Block::Hole(h) => walk_groups_mut(&mut h.data, h.content.as_deref_mut(), visitor),
        Block::Section(section) => {
            propagate!(walk_inlines_mut(&mut section.title, visitor));
            // See the immutable `walk_block`'s matching comment:
            // `section.connects` stays unvisited by the generic walker.
            for child in &mut section.blocks {
                propagate!(walk_block_mut(child, visitor));
            }
            ControlFlow::Continue(())
        }
        Block::List(list) => walk_list_items_mut(list, visitor),
        Block::Interp(_) => ControlFlow::Continue(()),
    }
}

fn walk_list_items_mut<B>(list: &mut List, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    for item in &mut list.items {
        propagate!(walk_element_mut(&mut item.element, visitor));
        if let Some(sub) = &mut item.sublist {
            propagate!(walk_list_items_mut(sub, visitor));
        }
    }
    ControlFlow::Continue(())
}

fn walk_inlines_mut<B>(inlines: &mut [Inline], visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    for inline in inlines {
        match inline {
            Inline::Element(element) => propagate!(walk_element_mut(element, visitor)),
            Inline::Hole(hole) => propagate!(walk_groups_mut(
                &mut hole.data,
                hole.content.as_deref_mut(),
                visitor
            )),
            _ => {}
        }
    }
    ControlFlow::Continue(())
}

fn walk_element_mut<B>(element: &mut Element, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    propagate!(visitor.visit_mut(element));
    walk_groups_mut(&mut element.data, element.content.as_deref_mut(), visitor)
}

/// Mutable counterpart of [`walk_groups`].
fn walk_groups_mut<B>(
    data: &mut Data,
    content: Option<&mut [Block]>,
    visitor: &mut impl VisitorMut<B>,
) -> ControlFlow<B> {
    if let Some(args) = &mut data.args {
        propagate!(walk_value_mut(args, visitor));
    }
    for block in content.into_iter().flatten() {
        propagate!(walk_block_mut(block, visitor));
    }
    if let Some(Group(entries)) = &mut data.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => propagate!(walk_element_mut(child, visitor)),
                Entry::Hole(hole) => propagate!(walk_groups_mut(
                    &mut hole.data,
                    hole.content.as_deref_mut(),
                    visitor
                )),
                Entry::Pair(_, v) => propagate!(walk_value_mut(v, visitor)),
            }
        }
    }
    ControlFlow::Continue(())
}

/// Mutable counterpart of [`walk_value`].
fn walk_value_mut<B>(value: &mut Value, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    match value {
        Value::Element(el) => walk_element_mut(el, visitor),
        Value::Hole(hole) => walk_groups_mut(&mut hole.data, hole.content.as_deref_mut(), visitor),
        Value::Seq(items) => {
            for item in items {
                propagate!(walk_value_mut(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                propagate!(walk_value_mut(v, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Call(_, args) => {
            for a in args {
                propagate!(walk_value_mut(a, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Blocks(blocks) => {
            for block in blocks {
                propagate!(walk_block_mut(block, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Null
        | Value::Interp(_)
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Number(_)
        | Value::String(_) => ControlFlow::Continue(()),
    }
}

/// Every descendant of `el` (not `el` itself), in document order --
/// `content`, `children`, and `Group` entries. `:rule`'s own
/// use is the only caller, so "descendant" means what `:rule(allow:...)`
/// means by it: content and children the reader encounters *underneath*
/// `el`, not configuration carried *on* `el`.
///
/// That is also why this deliberately never looks at `el.args`, even
/// though [`walk_element`] does (`args` can hold a [`Value::Element`]
/// since elements were allowed to sit in value position): an element
/// embedded in `el`'s own `(args)` -- `@thing(icon: @doc.icon("x"))` --
/// describes `thing`, the same way `el.connects` does, so it is exempt
/// for the same reason. `el.connects` itself is skipped for a related but
/// distinct reason: a connect (`Sigil::Named` carrying `:name(...)`'s own
/// args/content/value, e.g. `:rule(...)`) is not a descendant of the
/// document `el` sits in at all -- it is metadata about `el` itself,
/// consumed only by the dedicated check that understands the closed set
/// of connect names. Feeding either through here would let it reach the
/// same classification a real document element gets, which is exactly the
/// confusion a `:rule(...)` element (or an `@doc.icon` describing `thing`)
/// being reported as a disallowed *descendant* would be.
///
/// `direct_only` restricts to immediate children only -- one level of
/// `content`/`children`/`Group`, not recursed further --
/// for `:rule(direct:true)`; `false` walks every descendant at any
/// depth, same as [`walk_element`].
pub fn for_each_descendant(el: &Element, direct_only: bool, mut f: impl FnMut(&Element)) {
    if let Some(content) = &el.content {
        for_each_descendant_in_blocks(content, direct_only, &mut f);
    }
    if direct_only {
        if let Some(Group(entries)) = &el.data.value {
            for entry in entries {
                if let Entry::Element(child) = entry {
                    f(child);
                }
            }
        }
        return;
    }
    let mut visitor = |child: &Element| -> ControlFlow<()> {
        f(child);
        ControlFlow::Continue(())
    };
    if let Some(Group(entries)) = &el.data.value {
        for entry in entries {
            if let Entry::Element(child) = entry {
                let _ = walk_element(child, &mut visitor);
            }
        }
    }
}

/// Every descendant element reachable from `blocks`, in document order --
/// [`for_each_descendant`]'s own `content` handling factored out, since a
/// [`tomet_ast::Section`]'s `blocks` is exactly the same shape (a general
/// block sequence that may hold a `Paragraph`, an `Element`, or a nested
/// `Section`) and needs the identical walk: a heading's own `:rule(...)`
/// has to check what sits *under* the heading, which is this.
pub fn for_each_descendant_in_blocks(
    blocks: &[Block],
    direct_only: bool,
    mut f: impl FnMut(&Element),
) {
    if direct_only {
        for block in blocks {
            match block {
                Block::Element(child) => f(child),
                // See `for_each_descendant`'s own doc comment: a
                // `Paragraph`'s inline items are still direct children
                // for this purpose.
                Block::Paragraph(p) => {
                    for inline in &p.content {
                        if let Inline::Element(child) = inline {
                            f(child);
                        }
                    }
                }
                Block::Section(_) => {}
                Block::List(_) => {}
                Block::Declaration(_) | Block::Hole(_) => {}
                Block::Interp(_) => {}
            }
        }
        return;
    }
    let mut visitor = |child: &Element| -> ControlFlow<()> {
        f(child);
        ControlFlow::Continue(())
    };
    for block in blocks {
        let _ = walk_block(block, &mut visitor);
    }
}

/// Walks `items` as descendants, mirroring [`for_each_descendant_in_blocks`]
/// for a list's own items instead of a block sequence -- what a list's own
/// `:rule(allow:...)` checks against. `direct_only` means "just the items
/// themselves, not what's inside each one's own content" -- same meaning
/// `for_each_descendant`'s `direct_only` already has.
///
/// Deliberately never reads `list.connects` or any `ListItem`'s own
/// `element.connects` -- see [`for_each_descendant`]'s doc comment for why
/// that must stay true structurally, not just by convention.
pub fn for_each_item_descendant(
    items: &[ListItem],
    direct_only: bool,
    mut f: impl FnMut(&Element),
) {
    if direct_only {
        for item in items {
            f(&item.element);
        }
        return;
    }
    item_descendants(items, &mut f);
}

fn item_descendants(items: &[ListItem], f: &mut dyn FnMut(&Element)) {
    for item in items {
        let mut visitor = |child: &Element| -> ControlFlow<()> {
            f(child);
            ControlFlow::Continue(())
        };
        let _ = walk_element(&item.element, &mut visitor);
        if let Some(sub) = &item.sublist {
            item_descendants(&sub.items, f);
        }
    }
}

/// Visits every [`List`] in `doc`, in document order, including ones
/// reachable through an element's own `content`/`value` entries, a
/// section's `blocks`, and a list item's own `sublist` -- every place a
/// `Block::List` or a `ListItem.sublist` can occur. The counterpart to
/// [`for_each_section`] for the other non-`Element` `Block` variant.
pub fn for_each_list(doc: &Document, mut f: impl FnMut(&List)) {
    for block in &doc.blocks {
        lists_in_block(block, &mut f);
    }
}

fn lists_in_block(block: &Block, f: &mut dyn FnMut(&List)) {
    match block {
        Block::Paragraph(_) => {}
        Block::Element(el) => lists_in_element(el, f),
        Block::Declaration(d) => lists_in_groups(&d.data, d.content.as_deref(), f),
        Block::Hole(h) => lists_in_groups(&h.data, h.content.as_deref(), f),
        Block::Section(sec) => {
            for b in &sec.blocks {
                lists_in_block(b, f);
            }
            // sec.connects deliberately not visited, same invariant.
        }
        Block::List(list) => lists_in_list(list, f),
        Block::Interp(_) => {}
    }
}

fn lists_in_list(list: &List, f: &mut dyn FnMut(&List)) {
    f(list);
    for item in &list.items {
        lists_in_element(&item.element, f);
        if let Some(sub) = &item.sublist {
            lists_in_list(sub, f);
        }
    }
}

fn lists_in_element(el: &Element, f: &mut dyn FnMut(&List)) {
    lists_in_groups(&el.data, el.content.as_deref(), f);
    // el.connects deliberately not visited, same invariant.
}

fn lists_in_groups(data: &Data, content: Option<&[Block]>, f: &mut dyn FnMut(&List)) {
    for b in content.into_iter().flatten() {
        lists_in_block(b, f);
    }
    if let Some(Group(entries)) = &data.value {
        for entry in entries {
            if let Entry::Element(child) = entry {
                lists_in_element(child, f);
            }
        }
    }
}

/// Visits every [`tomet_ast::Section`] in `doc`, in document order,
/// including ones nested under another section's own `blocks` -- the
/// counterpart to [`for_each_element`] for the one `Block` variant that
/// is not an [`Element`], needed so a heading's own `:name(...)` connect
/// can be checked against `section.blocks`
/// ([`for_each_descendant_in_blocks`]) the same way an element's connect
/// is checked against its own content.
pub fn for_each_section(doc: &Document, mut f: impl FnMut(&tomet_ast::Section)) {
    fn walk(blocks: &[Block], f: &mut dyn FnMut(&tomet_ast::Section)) {
        for block in blocks {
            if let Block::Section(sec) = block {
                f(sec);
                walk(&sec.blocks, f);
            }
        }
    }
    walk(&doc.blocks, &mut f);
}

/// Mutates all elements in `doc` matching `filter` by applying `transform`.
/// Returns the number of elements transformed.
pub fn transform_elements<F, T>(doc: &mut Document, mut filter: F, mut transform: T) -> usize
where
    F: FnMut(&Element) -> bool,
    T: FnMut(&mut Element) -> bool,
{
    let mut count = 0;
    for_each_element_mut(doc, |el| {
        if filter(el) && transform(el) {
            count += 1;
        }
    });
    count
}

/// What [`for_each_node`] hands over: a thing in the tree that is not an
/// element, together with the position it was found at.
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
    Block(&'a Block),
    Inline(&'a Inline),
    Value(&'a Value),
    /// An entry of a `{...}` group: a `key: value` pair, an element or a hole.
    Entry(&'a Entry),
}

/// Visits every [`Block`], [`Inline`], [`Value`] and [`Entry`] in `doc`, in document
/// order, wherever it sits, each before what it holds.
///
/// Unlike [`walk_document`] and the walkers written beside it, this one has no
/// position it skips: it reaches running text and section titles, blocks
/// standing alone, an element's, a section's, a declaration's and a list
/// item's `(args)` / `{value}` / `[content]`, every `:connect`, and inside a
/// [`Value`] (`Seq`, `Map`, `Call`, a nested element, `[...]` blocks). It is
/// what the walkers for things that are not elements are written on:
/// [`for_each_interpolation`], [`for_each_declaration`] and [`for_each_hole`].
pub fn for_each_node(doc: &Document, mut f: impl FnMut(Node<'_>)) {
    for block in &doc.blocks {
        nodes_in_block(block, &mut f);
    }
}

/// Visits every [`Interpolation`] in `doc`, in document order, wherever it sits.
///
/// An interpolation is not an [`Element`], so the element walkers never pass
/// one to their visitor; this is the walker to ask for "every `${...}` in the
/// document".
pub fn for_each_interpolation(doc: &Document, mut f: impl FnMut(&Interpolation)) {
    for_each_node(doc, |node| match node {
        Node::Block(Block::Interp(i))
        | Node::Inline(Inline::Interp(i))
        | Node::Value(Value::Interp(i)) => f(i),
        _ => {}
    });
}

/// Visits every [`Declaration`] in `doc`, in document order, wherever it sits:
/// a declaration is a [`Block`], and a block can be in a section's body or an
/// element's content as well as at the top, where only it belongs.
/// [`for_each_top_level_declaration`] is the preamble.
pub fn for_each_declaration(doc: &Document, mut f: impl FnMut(&Declaration)) {
    for_each_node(doc, |node| {
        if let Node::Block(Block::Declaration(declaration)) = node {
            f(declaration);
        }
    });
}

/// Visits every [`Hole`] in `doc`, in document order, wherever it sits: as a
/// block, in running text, as an entry of a `{...}` group or as a value. A
/// document has none; a pattern read by `parse_pattern` has them.
pub fn for_each_hole(doc: &Document, mut f: impl FnMut(&Hole)) {
    for_each_node(doc, |node| match node {
        Node::Block(Block::Hole(hole))
        | Node::Inline(Inline::Hole(hole))
        | Node::Entry(Entry::Hole(hole)) => f(hole),
        Node::Value(Value::Hole(hole)) => f(hole),
        _ => {}
    });
}

fn nodes_in_block(block: &Block, f: &mut dyn FnMut(Node<'_>)) {
    f(Node::Block(block));
    match block {
        Block::Paragraph(paragraph) => nodes_in_inlines(&paragraph.content, f),
        Block::Element(element) => nodes_in_element(element, f),
        Block::Declaration(d) => {
            nodes_in_data(&d.data, f);
            nodes_in_content(d.content.as_deref(), f);
        }
        Block::Hole(h) => {
            nodes_in_data(&h.data, f);
            nodes_in_content(h.content.as_deref(), f);
        }
        Block::Section(section) => {
            nodes_in_inlines(&section.title, f);
            nodes_in_data(&section.data, f);
            for connect in &section.connects {
                nodes_in_connect(connect, f);
            }
            for child in &section.blocks {
                nodes_in_block(child, f);
            }
        }
        Block::List(list) => nodes_in_list(list, f),
        Block::Interp(_) => {}
    }
}

fn nodes_in_list(list: &List, f: &mut dyn FnMut(Node<'_>)) {
    for connect in &list.connects {
        nodes_in_connect(connect, f);
    }
    for item in &list.items {
        nodes_in_element(&item.element, f);
        if let Some(sublist) = &item.sublist {
            nodes_in_list(sublist, f);
        }
    }
}

fn nodes_in_inlines(inlines: &[Inline], f: &mut dyn FnMut(Node<'_>)) {
    for inline in inlines {
        f(Node::Inline(inline));
        match inline {
            Inline::Element(element) => nodes_in_element(element, f),
            Inline::Hole(hole) => {
                nodes_in_data(&hole.data, f);
                nodes_in_content(hole.content.as_deref(), f);
            }
            _ => {}
        }
    }
}

fn nodes_in_element(element: &Element, f: &mut dyn FnMut(Node<'_>)) {
    nodes_in_data(&element.data, f);
    nodes_in_content(element.content.as_deref(), f);
    for connect in &element.connects {
        nodes_in_connect(connect, f);
    }
}

fn nodes_in_connect(connect: &Connect, f: &mut dyn FnMut(Node<'_>)) {
    nodes_in_data(&connect.data, f);
    nodes_in_content(connect.content.as_deref(), f);
}

fn nodes_in_content(content: Option<&[Block]>, f: &mut dyn FnMut(Node<'_>)) {
    for block in content.into_iter().flatten() {
        nodes_in_block(block, f);
    }
}

/// `(args)` and `{value}`: the part every node that carries data shares.
fn nodes_in_data(data: &Data, f: &mut dyn FnMut(Node<'_>)) {
    if let Some(args) = &data.args {
        nodes_in_value(args, f);
    }
    if let Some(Group(entries)) = &data.value {
        for entry in entries {
            f(Node::Entry(entry));
            match entry {
                Entry::Pair(_, value) => nodes_in_value(value, f),
                Entry::Element(element) => nodes_in_element(element, f),
                Entry::Hole(hole) => {
                    nodes_in_data(&hole.data, f);
                    nodes_in_content(hole.content.as_deref(), f);
                }
            }
        }
    }
}

fn nodes_in_value(value: &Value, f: &mut dyn FnMut(Node<'_>)) {
    f(Node::Value(value));
    match value {
        Value::Element(element) => nodes_in_element(element, f),
        Value::Hole(hole) => {
            nodes_in_data(&hole.data, f);
            nodes_in_content(hole.content.as_deref(), f);
        }
        Value::Seq(items) => items.iter().for_each(|v| nodes_in_value(v, f)),
        Value::Map(entries) => entries.iter().for_each(|(_, v)| nodes_in_value(v, f)),
        Value::Call(_, args) => args.iter().for_each(|v| nodes_in_value(v, f)),
        Value::Blocks(blocks) => blocks.iter().for_each(|b| nodes_in_block(b, f)),
        Value::Interp(_)
        | Value::Null
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Number(_)
        | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Block;

    fn element_of(doc: &Document) -> &Element {
        match &doc.blocks[0] {
            Block::Element(el) => el,
            other => panic!("expected an element, got {other:?}"),
        }
    }

    fn names(el: &Element, direct_only: bool) -> Vec<String> {
        let mut out = Vec::new();
        for_each_descendant(el, direct_only, |child| {
            out.push(
                child
                    .sigil
                    .name()
                    .map(|n| n.name.clone())
                    .unwrap_or_default(),
            );
        });
        out
    }

    #[test]
    fn direct_only_stops_at_the_first_level() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, true), vec!["mid"]);
    }

    #[test]
    fn every_descendant_walks_every_depth() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, false), vec!["mid", "inner"]);
    }

    #[test]
    fn descendant_walk_never_visits_the_element_itself() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ text ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert!(!names(outer, false).contains(&"outer".to_string()));
    }

    #[test]
    fn every_interpolation_is_found_wherever_it_sits() {
        let src = "=[ ${title} ]:rule(allow: ${conn})\n\
                   \n\
                   ${block}\n\
                   \n\
                   text ${inline} more\n\
                   \n\
                   @x(a: ${arg}, b: list(${nested})){ k: ${val} }[ ${content} ]:as(${connect})\n\
                   \n\
                   - item ${item}\n";
        let doc = tomet_parser::parse_document(src).unwrap();
        let mut found = Vec::new();
        for_each_interpolation(&doc, |i| found.push(i.expr.to_string()));
        found.sort();
        let mut expected = vec![
            "title", "conn", "block", "inline", "arg", "nested", "val", "content", "connect",
            "item",
        ];
        expected.sort();
        assert_eq!(found, expected);
    }
}
