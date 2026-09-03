//! File difference model and safe disk saving operations.

use std::fs;
use std::path::PathBuf;

/// Represents the modification state and diff of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub changes_count: usize,
}

impl FileDiff {
    pub fn new(
        path: PathBuf,
        original_src: String,
        modified_src: String,
        changes_count: usize,
    ) -> Self {
        Self {
            path,
            original_src,
            modified_src,
            changes_count,
        }
    }

    /// Returns `true` if a refactoring pass actually rewrote something.
    ///
    /// Deliberately not `original_src != modified_src`. Producing
    /// `modified_src` re-prints and re-formats the whole document, so any
    /// file that was not already in canonical printer form differs
    /// textually even when no pass touched it. Comparing the text made
    /// `refactor --check` report nearly every file as needing work with a
    /// change count of zero, and made `refactor -i` rewrite files no pass
    /// had anything to say about -- reformatting is `tomet format`'s job.
    pub fn is_changed(&self) -> bool {
        self.changes_count > 0
    }

    /// Saves the modified content to disk if changed.
    pub fn save(&mut self) -> anyhow::Result<bool> {
        if self.is_changed() {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&self.path, &self.modified_src)?;
            self.original_src = self.modified_src.clone();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Saves all modified `FileDiff`s to disk, returning the count of saved files.
pub fn save_file_diffs(diffs: &mut [FileDiff]) -> anyhow::Result<usize> {
    let mut count = 0;
    for diff in diffs {
        if diff.save()? {
            count += 1;
        }
    }
    Ok(count)
}
