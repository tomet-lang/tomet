//! What a [`Tome`] owns.

use tomet_ast::File;

/// A vault's tree of parsed documents. Built by [`Tome::build`]; read through [`Tome::node`].
#[derive(Debug, Clone, PartialEq)]
pub struct Tome {
    pub vault: Vault,
}

/// The top of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Vault {
    /// The vault's config document, whose `::` declarations are the vault's heading part. A vault
    /// found with no config document has none, and an empty heading part.
    pub config: Option<NamedFile>,
    /// The vault's root directory, which is a [`Folder`] like any other.
    pub root: Folder,
}

/// A directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    /// The directory's own name. Empty for the vault's root, whose name is the vault's.
    pub name: String,
    /// The Folder's own node: the document in this directory with the vault's entry name. It is
    /// not one of the [`Folder::children`].
    pub entry: Option<NamedFile>,
    /// The directories and documents in this directory, in the order given to [`Tome::build`]'s
    /// collator. Names are unique here, and a directory appears only if a document is in it.
    pub children: Vec<Child>,
}

/// What a [`Folder`] holds besides its entry document.
#[derive(Debug, Clone, PartialEq)]
pub enum Child {
    Folder(Folder),
    File(NamedFile),
}

impl Child {
    /// The name of the directory or file in its Folder.
    pub fn name(&self) -> &str {
        match self {
            Child::Folder(folder) => &folder.name,
            Child::File(document) => &document.name,
        }
    }
}

/// A parsed [`File`] and the name of the file it was read from: what a [`crate::Node::File`] stands for,
/// and what a Vault's config and a Folder's entry are.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedFile {
    /// The file name, without its directory.
    pub name: String,
    pub file: File,
}
