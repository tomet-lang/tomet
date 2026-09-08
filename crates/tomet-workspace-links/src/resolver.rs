use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tomet_config::PrinterConfig;

/// In-memory catalog of paths in a vault used to resolve `ref:` (wikilink) targets.
///
/// Supports two construction paths:
/// - [`VaultLinkIndex::from_paths`]: Pure in-memory constructor, ideal for tests and Wasm.
/// - [`VaultLinkIndex::from_vault`]: Walks the filesystem using vault configuration and ignore rules.
#[derive(Debug, Clone, Default)]
pub struct VaultLinkIndex {
    paths: Vec<PathBuf>,
    /// Map of lowercase file stem (e.g. "note" for "Note.tmt") -> indices in `paths`
    stem_map: HashMap<String, Vec<usize>>,
    /// Map of lowercase full filename (e.g. "note.tmt") -> indices in `paths`
    name_map: HashMap<String, Vec<usize>>,
}

impl VaultLinkIndex {
    /// Creates a new index from an in-memory list of paths (relative to vault/project root).
    pub fn from_paths<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Self {
        let mut index = Self::default();
        for path in paths {
            index.add_path(path.as_ref().to_path_buf());
        }
        index
    }

    /// Creates an index by walking the project root honoring `.gitignore` and `workspace.ignore`.
    pub fn from_vault(
        project_root: &Path,
        config: &PrinterConfig,
        config_root: &Path,
    ) -> Self {
        let all_paths =
            tomet_indexer::collect_all_paths_with_config(project_root, config, config_root);
        let mut index = Self::default();
        for path in all_paths {
            // Store as relative to project_root if possible for cleaner matches
            let rel = path.strip_prefix(project_root).unwrap_or(&path);
            index.add_path(rel.to_path_buf());
        }
        index
    }

    fn add_path(&mut self, path: PathBuf) {
        let idx = self.paths.len();
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            let lower_name = file_name.to_lowercase();
            self.name_map.entry(lower_name).or_default().push(idx);

            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let lower_stem = stem.to_lowercase();
                self.stem_map.entry(lower_stem).or_default().push(idx);
            }
        }
        self.paths.push(path);
    }

    /// Resolves a `ref:` target (e.g. "Note Name" or "folder/Note Name") to a matching path in the vault.
    ///
    /// If multiple candidate files match (e.g. `10-Journal/memo.tmt` and `50-Sandbox/memo.tmt`),
    /// this uses proximity matching: the candidate that shares the deepest common ancestor directory
    /// with `from_path` is chosen.
    pub fn resolve_ref(&self, target: &str, from_path: Option<&Path>) -> Option<&Path> {
        let clean_target = target.split('#').next().unwrap_or(target).trim();
        if clean_target.is_empty() {
            return None;
        }

        // 0. Relative path resolution from current document's folder (e.g. "./image.png", "../image.png")
        if (clean_target.starts_with("./") || clean_target.starts_with("../")) && from_path.is_some() {
            let from = from_path.unwrap();
            if let Some(parent) = from.parent() {
                let joined = parent.join(clean_target);
                let mut normalized = PathBuf::new();
                for comp in joined.components() {
                    match comp {
                        std::path::Component::CurDir => {}
                        std::path::Component::ParentDir => {
                            normalized.pop();
                        }
                        std::path::Component::Normal(c) => {
                            normalized.push(c);
                        }
                        _ => {}
                    }
                }
                let norm_str = normalized.to_string_lossy().replace('\\', "/");
                if let Some(pos) = self.paths.iter().position(|p| p.to_string_lossy().replace('\\', "/") == norm_str) {
                    return Some(&self.paths[pos]);
                }
            }
        }

        // 1. Direct relative or path-like match (e.g. "subfolder/note" or "note.tmt")
        let target_path = Path::new(clean_target);
        if target_path.components().count() > 1 {
            let clean_normalized = clean_target.replace('\\', "/");
            let mut path_candidates = Vec::new();
            for (idx, p) in self.paths.iter().enumerate() {
                let p_str = p.to_string_lossy().replace('\\', "/");
                if p_str == clean_normalized
                    || p_str.ends_with(&format!("/{clean_normalized}"))
                    || p.file_stem().and_then(|s| s.to_str()) == Some(clean_target)
                {
                    path_candidates.push(idx);
                }
            }
            if !path_candidates.is_empty() {
                return self.pick_best_candidate(&path_candidates, from_path);
            }
        }

        // 2. Lookup by exact filename or stem (case-insensitive)
        let lower = clean_target.to_lowercase();
        let candidates = self
            .name_map
            .get(&lower)
            .or_else(|| self.stem_map.get(&lower));

        let candidates = match candidates {
            Some(c) if !c.is_empty() => c,
            _ => return None,
        };

        self.pick_best_candidate(candidates, from_path)
    }

    fn pick_best_candidate(&self, candidate_indices: &[usize], from_path: Option<&Path>) -> Option<&Path> {
        if candidate_indices.is_empty() {
            return None;
        }
        if candidate_indices.len() == 1 {
            return Some(&self.paths[candidate_indices[0]]);
        }

        // Multiple candidates: pick closest by common directory depth
        let mut best_idx = candidate_indices[0];
        let mut max_depth = 0;

        for &idx in candidate_indices {
            let candidate = &self.paths[idx];
            let depth = match from_path {
                Some(from) => common_dir_depth(from, candidate),
                None => 0,
            };

            if depth > max_depth {
                max_depth = depth;
                best_idx = idx;
            } else if depth == max_depth {
                // Tie breaker: lexicographically earlier path
                if candidate < &self.paths[best_idx] {
                    best_idx = idx;
                }
            }
        }

        Some(&self.paths[best_idx])
    }
}

/// Computes the depth of common ancestor directories between two paths.
fn common_dir_depth(a: &Path, b: &Path) -> usize {
    let a_parent = a.parent();
    let b_parent = b.parent();
    match (a_parent, b_parent) {
        (Some(ap), Some(bp)) => {
            let mut count = 0;
            for (ca, cb) in ap.components().zip(bp.components()) {
                if ca == cb {
                    count += 1;
                } else {
                    break;
                }
            }
            count
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_candidate_resolution() {
        let index = VaultLinkIndex::from_paths(&[
            "30-39 Knowledge/Linux.tmt",
            "10-19 Journal/daily/2026-09-09.tmt",
        ]);

        assert_eq!(
            index.resolve_ref("Linux", None),
            Some(Path::new("30-39 Knowledge/Linux.tmt"))
        );
        // Case-insensitive
        assert_eq!(
            index.resolve_ref("linux", None),
            Some(Path::new("30-39 Knowledge/Linux.tmt"))
        );
        // With anchor
        assert_eq!(
            index.resolve_ref("Linux#kernel", None),
            Some(Path::new("30-39 Knowledge/Linux.tmt"))
        );
    }

    #[test]
    fn test_proximity_matching_for_duplicate_names() {
        let index = VaultLinkIndex::from_paths(&[
            "00-09 System/memo.tmt",
            "10-19 Journal/daily/memo.tmt",
            "50-59 Sandbox/memo.tmt",
        ]);

        let from_journal = Path::new("10-19 Journal/daily/2026-09-09.tmt");
        assert_eq!(
            index.resolve_ref("memo", Some(from_journal)),
            Some(Path::new("10-19 Journal/daily/memo.tmt"))
        );

        let from_system = Path::new("00-09 System/config.tmt");
        assert_eq!(
            index.resolve_ref("memo", Some(from_system)),
            Some(Path::new("00-09 System/memo.tmt"))
        );
    }

    #[test]
    fn test_unresolved_returns_none() {
        let index = VaultLinkIndex::from_paths(&["30-39 Knowledge/Linux.tmt"]);
        assert_eq!(index.resolve_ref("NonExistentNote", None), None);
    }

    #[test]
    fn test_image_and_proximity_resolution() {
        let index = VaultLinkIndex::from_paths(&[
            "30-39 Knowledge/38 Geograph/ミーム/大沢たかお祭り.tmt",
            "30-39 Knowledge/38 Geograph/ミーム/-/+8c3002a8a891b78137b6547f600a88141a828640.png",
            "50-59 Sandbox/51 Database/ゲーム/Dorfromantik.tmt",
            "50-59 Sandbox/51 Database/ゲーム/-/+700fe7be15805a34ad1044c351f7dde08c010ec1.png",
        ]);

        let note = Path::new("30-39 Knowledge/38 Geograph/ミーム/大沢たかお祭り.tmt");
        assert_eq!(
            index.resolve_ref("+8c3002a8a891b78137b6547f600a88141a828640.png", Some(note)),
            Some(Path::new("30-39 Knowledge/38 Geograph/ミーム/-/+8c3002a8a891b78137b6547f600a88141a828640.png"))
        );
    }

    #[test]
    fn test_relative_path_resolution() {
        let index = VaultLinkIndex::from_paths(&[
            "docs/guide/index.tmt",
            "docs/guide/images/diagram.png",
            "docs/assets/logo.svg",
        ]);

        let from_guide = Path::new("docs/guide/index.tmt");
        assert_eq!(
            index.resolve_ref("./images/diagram.png", Some(from_guide)),
            Some(Path::new("docs/guide/images/diagram.png"))
        );
        assert_eq!(
            index.resolve_ref("../assets/logo.svg", Some(from_guide)),
            Some(Path::new("docs/assets/logo.svg"))
        );
    }
}
