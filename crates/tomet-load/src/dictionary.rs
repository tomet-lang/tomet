//! Reading the dictionaries a vault registers.
//!
//! The half of `tomet_semantics::dictionary` that touches the filesystem, split the way
//! [`crate::vocabulary`] is: what makes a dictionary acceptable is
//! [`LoadedDictionaries::add`](tomet_semantics::LoadedDictionaries::add), pure, and finding and
//! reading the files is here, because the semantics layer may not do I/O.
//!
//! Nothing is discovered. A vault lists its dictionaries by path (`::dictionaries{ name:
//! @file(path) }`) and each file names itself with `::dictionary(name)`.

use std::path::Path;

use tomet_semantics::LoadedDictionaries;

/// Reads every dictionary in `declared`, as paths relative to `root`.
pub fn load_dictionaries(root: &Path, declared: &[String]) -> LoadedDictionaries {
    let mut loaded = LoadedDictionaries::default();

    for declared in declared {
        match std::fs::read_to_string(root.join(declared)) {
            Ok(src) => match tomet_parser::parse_document(&src) {
                Ok(doc) => loaded.add(declared, &doc),
                Err(e) => loaded
                    .errors
                    .push(format!("declared dictionary {declared}: {e}")),
            },
            Err(e) => loaded
                .errors
                .push(format!("declared dictionary {declared}: {e}")),
        }
    }

    loaded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_registered_dictionary_is_read_and_a_missing_one_is_reported() {
        let root = std::env::temp_dir().join("tomet_test_load_dictionary");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(
            root.join("tomet.tmt"),
            "::dictionary(tomet){ \"vault.config.tmt\": @term[ x ]#(vault-config) }\n",
        )
        .expect("dictionary");

        let loaded = load_dictionaries(&root, &["tomet.tmt".to_string(), "gone.tmt".to_string()]);
        assert!(
            loaded
                .get("tomet")
                .and_then(|d| d.by_id("vault-config"))
                .is_some()
        );
        assert_eq!(loaded.errors.len(), 1, "{:?}", loaded.errors);
        assert!(loaded.errors[0].contains("gone.tmt"));

        let _ = fs::remove_dir_all(&root);
    }
}
