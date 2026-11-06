//! Assembling a [`Tome`] from documents.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

use tomet_ast::File;
use tomet_sort::Collator;

use crate::model::{Child, Folder, NamedFile, Tome, Vault};

/// Why documents cannot be assembled into a [`Tome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// A path that is empty or has an empty, `.` or `..` segment.
    InvalidPath(String),
    /// Two documents at one path.
    Duplicate(String),
    /// A path that is a document in one place and a directory in another.
    FileAndFolder(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::InvalidPath(path) => {
                write!(f, "`{path}` is not a path inside the vault")
            }
            BuildError::Duplicate(path) => write!(f, "`{path}` is given twice"),
            BuildError::FileAndFolder(path) => {
                write!(f, "`{path}` is both a document and a directory")
            }
        }
    }
}

impl std::error::Error for BuildError {}

impl Tome {
    /// Assembles `files` into a tree.
    ///
    /// Each of `files` is a parsed document with the path it has in the vault: names separated by
    /// `/`, relative to the vault's root, in no particular order. They are grouped by directory,
    /// and in a directory the document named `entry` is the Folder's [`Folder::entry`] rather than
    /// one of its children. The other children are put in the order of `collator` over their
    /// names, a directory by its name and a document by its file name; names that the collator
    /// does not tell apart are put in the order of their code points, so the order never depends
    /// on the order `files` came in.
    ///
    /// `config` is the vault's config document, which belongs to no directory.
    pub fn build<P: AsRef<str>>(
        entry: &str,
        collator: &Collator,
        config: Option<NamedFile>,
        files: impl IntoIterator<Item = (P, File)>,
    ) -> Result<Tome, BuildError> {
        let mut root = Draft::default();
        for (path, file) in files {
            let path = path.as_ref();
            let segments: Vec<&str> = path.split('/').collect();
            if segments.iter().any(|s| matches!(*s, "" | "." | "..")) {
                return Err(BuildError::InvalidPath(path.to_string()));
            }
            root.insert(entry, &segments, path, file)?;
        }
        Ok(Tome {
            vault: Vault {
                config,
                root: root.finish(String::new(), collator),
            },
        })
    }
}

/// A directory before its children are ordered. Names key a map, which is what makes a duplicate
/// visible and gives the code point order that ties are left in.
#[derive(Default)]
struct Draft {
    entry: Option<NamedFile>,
    members: BTreeMap<String, Member>,
}

enum Member {
    Folder(Draft),
    File(File),
}

impl Draft {
    /// Puts `file`, whose path is `path` and whose segments from this directory on are `rest`.
    fn insert(
        &mut self,
        entry: &str,
        rest: &[&str],
        path: &str,
        file: File,
    ) -> Result<(), BuildError> {
        let duplicate = || BuildError::Duplicate(path.to_string());
        let conflict = || BuildError::FileAndFolder(path.to_string());
        match rest {
            [] => unreachable!("a path has at least one segment"),
            [name] if *name == entry => {
                if self.entry.is_some() {
                    return Err(duplicate());
                }
                self.entry = Some(NamedFile {
                    name: name.to_string(),
                    file,
                });
                Ok(())
            }
            [name] => match self.members.entry(name.to_string()) {
                Entry::Vacant(slot) => {
                    slot.insert(Member::File(file));
                    Ok(())
                }
                Entry::Occupied(slot) => Err(match slot.get() {
                    Member::File(_) => duplicate(),
                    Member::Folder(_) => conflict(),
                }),
            },
            [dir, rest @ ..] => {
                let member = self
                    .members
                    .entry(dir.to_string())
                    .or_insert_with(|| Member::Folder(Draft::default()));
                match member {
                    Member::Folder(folder) => folder.insert(entry, rest, path, file),
                    Member::File(_) => Err(conflict()),
                }
            }
        }
    }

    fn finish(self, name: String, collator: &Collator) -> Folder {
        // The map yields names in code point order, and the sort is stable.
        let mut children: Vec<Child> = self
            .members
            .into_iter()
            .map(|(name, member)| match member {
                Member::Folder(draft) => Child::Folder(draft.finish(name, collator)),
                Member::File(file) => Child::File(NamedFile { name, file }),
            })
            .collect();
        collator.sort_by_key(&mut children, Child::name);
        Folder {
            name,
            entry: self.entry,
            children,
        }
    }
}
