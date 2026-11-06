//! One definition of what sits inside what in a [`Tome`], and the walk built on it.

use std::ops::ControlFlow;

use tomet_ast::{Block, Connect, Data, Declaration, File, Id, Inline, Section};
use tomet_tree::Visit;

use crate::model::{Child, Folder, NamedFile, Tome, Vault};

/// A borrowed reference to any node of a [`Tome`]. Not to be confused with `tomet_tree::Node`,
/// which is a vertex inside one file's AST.
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
    Vault(&'a Vault),
    Folder(&'a Folder),
    File(&'a NamedFile),
    /// A section of a document, borrowed from that document's AST.
    Section(&'a Section),
}

/// The heading part of a [`Node`]: what is said about it, as opposed to what sits inside it.
///
/// A borrowed view, assembled by [`Node::heading`]. A Vault, a Folder and a File say nothing but
/// declarations, and a Section says everything but declarations, so the parts that a level does
/// not have are empty.
#[derive(Debug, Clone, Copy)]
pub struct Heading<'a> {
    /// The `::name(...)` declarations at the top of the node's document, in source order.
    pub declarations: &'a [Declaration],
    /// The title of a section.
    pub title: &'a [Inline],
    pub data: &'a Data,
    pub id: Option<&'a Id>,
    pub connects: &'a [Connect],
}

static NO_DATA: Data = Data {
    args: None,
    value: None,
};

impl<'a> Heading<'a> {
    /// A heading that is declarations alone.
    fn declared(declarations: &'a [Declaration]) -> Self {
        Heading {
            declarations,
            title: &[],
            data: &NO_DATA,
            id: None,
            connects: &[],
        }
    }

    fn of_file(file: Option<&'a NamedFile>) -> Self {
        Self::declared(file.map_or(&[], |document| &document.file.header))
    }
}

impl Tome {
    /// The node the whole tree hangs from.
    pub fn node(&self) -> Node<'_> {
        Node::Vault(&self.vault)
    }
}

impl<'a> Node<'a> {
    /// The heading part of this node: for a Vault its config document's declarations, for a
    /// Folder its entry document's, for a File its own, and for a Section its title and groups.
    #[deny(clippy::wildcard_enum_match_arm)]
    pub fn heading(self) -> Heading<'a> {
        match self {
            Node::Vault(vault) => Heading::of_file(vault.config.as_ref()),
            Node::Folder(folder) => Heading::of_file(folder.entry.as_ref()),
            Node::File(document) => Heading::declared(&document.file.header),
            Node::Section(section) => Heading {
                declarations: &[],
                title: &section.title,
                data: &section.data,
                id: section.id.as_ref(),
                connects: &section.connects,
            },
        }
    }
}

/// Calls `f` for every node sitting directly in `node`.
///
/// This is the one place that says what is inside what. A Vault holds the sections of its config
/// document and then its root Folder. A Folder holds the sections of its entry document and then
/// its children, in their order. A File holds its top-level sections, and a Section the sections
/// nested in it. A paragraph, a list or any other block is not a node of a Tome.
#[deny(clippy::wildcard_enum_match_arm)]
pub fn children<'a>(node: Node<'a>, f: &mut dyn FnMut(Node<'a>)) {
    match node {
        Node::Vault(vault) => {
            own_sections(vault.config.as_ref(), f);
            f(Node::Folder(&vault.root));
        }
        Node::Folder(folder) => {
            own_sections(folder.entry.as_ref(), f);
            for child in &folder.children {
                f(match child {
                    Child::Folder(folder) => Node::Folder(folder),
                    Child::File(document) => Node::File(document),
                });
            }
        }
        Node::File(document) => sections(&document.file.body, f),
        Node::Section(section) => sections(&section.blocks, f),
    }
}

/// The sections of a document that is a node's own, if it has one.
fn own_sections<'a>(document: Option<&'a NamedFile>, f: &mut dyn FnMut(Node<'a>)) {
    if let Some(NamedFile {
        file: File { body, .. },
        ..
    }) = document
    {
        sections(body, f);
    }
}

fn sections<'a>(blocks: &'a [Block], f: &mut dyn FnMut(Node<'a>)) {
    for block in blocks {
        if let Block::Section(section) = block {
            f(Node::Section(section));
        }
    }
}

/// Shows `f` `node` and every node under it, each before what it holds, in the order of
/// [`children`], and goes into what `f` answers [`Visit::Descend`] to. [`Visit::Break`] stops the
/// walk and is handed back.
pub fn walk<'a, B>(node: Node<'a>, f: &mut impl FnMut(Node<'a>) -> Visit<B>) -> ControlFlow<B> {
    match f(node) {
        Visit::Break(b) => return ControlFlow::Break(b),
        Visit::Skip => return ControlFlow::Continue(()),
        Visit::Descend => {}
    }
    let mut flow = ControlFlow::Continue(());
    children(node, &mut |child| {
        if flow.is_continue() {
            flow = walk(child, f);
        }
    });
    flow
}
