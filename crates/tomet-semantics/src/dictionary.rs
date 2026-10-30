//! Dictionaries of terms: reading a `::dictionary` document, and looking a term up.
//!
//! A dictionary defines words a vault's documents use, once, so that a document mentions one with
//! `@term` instead of writing it out. It is a document of its own:
//!
//! ```text
//! ::dictionary(tomet){
//!   "vault.config.tmt": @term[ The file that marks a vault. ]#(vault-config)
//! }
//! ```
//!
//! The key is the term, quoted, so a word in any language fits. The content of the `@term` is its
//! description. `#(vault-config)` is an optional stable identifier: the term may be renamed and a
//! mention by identifier does not change. A term and an identifier are each unique within their
//! dictionary, the way an id is unique within its document.
//!
//! This is the pure half. Finding and reading the files a vault registers with `::dictionaries` is
//! `tomet-load`'s, the same split as for vocabularies ([`crate::vocabulary`]).

use std::collections::BTreeMap;

use tomet_ast::{Block, Document, Value};
use tomet_declaration::{DeclKind, dictionary_name};

use crate::kind::{ElementKind, classify_std_lenient};

/// One term a dictionary defines.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// The term itself, as written between the quotes.
    pub name: String,
    /// The stable identifier, `#(id)` after the `@term`, when it has one.
    pub id: Option<String>,
    /// What the term means: the `[content]` of its `@term`. Empty when it has none.
    pub description: Vec<Block>,
}

/// A dictionary: a name and its terms, in the order written.
#[derive(Debug, Clone, PartialEq)]
pub struct Dictionary {
    /// What `::dictionary(name)` calls it; a mention names it on the left of `#`.
    pub name: String,
    pub terms: Vec<Term>,
}

impl Dictionary {
    /// The term with this name, if the dictionary defines it.
    pub fn by_name(&self, name: &str) -> Option<&Term> {
        self.terms.iter().find(|term| term.name == name)
    }

    /// The term with this identifier, if the dictionary defines it.
    pub fn by_id(&self, id: &str) -> Option<&Term> {
        self.terms
            .iter()
            .find(|term| term.id.as_deref() == Some(id))
    }
}

/// What a `@term` mention names, read from its one argument.
///
/// ```text
/// @term(tomet#vault-config)               by identifier, in a dictionary
/// @term(tomet#name("vault.config.tmt"))   by name, in a dictionary
/// @term("vault.config.tmt")               by name, in any dictionary
/// ```
///
/// This is the shape `<document>#<place>` with the dictionary as the document, and `name("...")` as a
/// step like `item(2)`. Nothing on the left of the `#` means this document, as it does everywhere: inside
/// a dictionary, `#vault-config` is a term of that dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermRef {
    /// A term by name, with no dictionary named: searched in all of them.
    Name(String),
    /// A term by its identifier. Unique in its dictionary only, so a dictionary has to be named, or
    /// the mention has to be written inside the dictionary itself (`dictionary` is `None`).
    Id {
        dictionary: Option<String>,
        id: String,
    },
    /// A term by name in a named dictionary, or in this one (`dictionary` is `None`).
    NameIn {
        dictionary: Option<String>,
        name: String,
    },
}

impl TermRef {
    /// Reads the argument of a `@term`.
    ///
    /// A target that does not have one of the address shapes is a term name as it stands, so a word
    /// with a `#` in it (`"C# language"`) is still a term. The one thing that cannot be told apart is a
    /// word that looks exactly like an address, such as `C#sharp`; write that one by `tomet#name("...")`.
    pub fn parse(target: &str) -> Self {
        let Some((left, right)) = target.split_once('#') else {
            return TermRef::Name(target.to_string());
        };
        let dictionary = if left.is_empty() {
            Some(None)
        } else if is_plain(left) {
            Some(Some(left.to_string()))
        } else {
            None
        };
        let Some(dictionary) = dictionary else {
            return TermRef::Name(target.to_string());
        };
        if let Some(name) = right
            .strip_prefix("name(\"")
            .and_then(|rest| rest.strip_suffix("\")"))
        {
            return TermRef::NameIn {
                dictionary,
                name: name.to_string(),
            };
        }
        if is_plain(right) {
            return TermRef::Id {
                dictionary,
                id: right.to_string(),
            };
        }
        TermRef::Name(target.to_string())
    }
}

/// An identifier or a dictionary name as written bare: no space, quote, bracket or `#`.
fn is_plain(text: &str) -> bool {
    !text.is_empty()
        && !text
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '#' | '(' | ')'))
}

/// Why a mention names no term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermProblem {
    /// An identifier written outside a dictionary with none named: `@term(#vault-config)`.
    NeedsDictionary,
    /// The dictionary is not one the vault registered.
    UnknownDictionary(String),
    /// The dictionary has no such term, or none of them has.
    NotDefined,
    /// A name with no dictionary named that several dictionaries define.
    Ambiguous(Vec<String>),
}

impl std::fmt::Display for TermProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TermProblem::NeedsDictionary => write!(
                f,
                "an identifier is unique only in its dictionary; name the dictionary, as `tomet#id`"
            ),
            TermProblem::UnknownDictionary(name) => {
                write!(f, "`{name}` is not a dictionary this vault registers")
            }
            TermProblem::NotDefined => write!(f, "no dictionary defines it"),
            TermProblem::Ambiguous(dictionaries) => write!(
                f,
                "it is defined in {}; name one, as `{}#name(\"...\")`",
                dictionaries.join(", "),
                dictionaries[0]
            ),
        }
    }
}

/// The dictionaries a vault registered, by the name each gives itself.
///
/// A dictionary that cannot be accepted is reported in `errors` and left out, not fatal: one bad
/// file must not hide the state of every document.
#[derive(Debug, Clone, Default)]
pub struct LoadedDictionaries {
    by_name: BTreeMap<String, (String, Dictionary)>,
    /// Why a dictionary, or a term of one, was not accepted, each led by the label of its source.
    pub errors: Vec<String>,
}

impl LoadedDictionaries {
    /// Reads `doc`, which was read from `label` (a path, as the vault registers it), and accepts
    /// the dictionary it declares.
    ///
    /// Not accepted: a document that declares no `::dictionary(name)`, and a name another dictionary
    /// already has. Only the entry is left out for an identifier used twice and for an entry that is
    /// not an `@term`. (A term written twice is a parse error: a map has no repeated key.)
    pub fn add(&mut self, label: &str, doc: &Document) {
        let Some(found) = tomet_declaration::declarations(doc)
            .into_iter()
            .find(|found| found.kind == DeclKind::Dictionary)
        else {
            self.errors.push(format!(
                "declared dictionary {label}: it does not declare a ::dictionary(name)"
            ));
            return;
        };
        let Some(name) = dictionary_name(&found.declaration) else {
            self.errors.push(format!(
                "declared dictionary {label}: ::dictionary(name) names no dictionary"
            ));
            return;
        };
        if let Some((held, _)) = self.by_name.get(&name) {
            self.errors.push(format!(
                "declared dictionary {label}: `{name}` is already the dictionary of {held}"
            ));
            return;
        }

        let mut terms: Vec<Term> = Vec::new();
        if let Some(Value::Map(entries)) = &found.declaration.data {
            for (key, value) in entries {
                let problem = |what: &str| format!("dictionary {label}: `{key}` {what}");
                let Value::Element(el) = value else {
                    self.errors.push(problem("is not an @term"));
                    continue;
                };
                if classify_std_lenient(el) != ElementKind::Term {
                    self.errors.push(problem("is not an @term"));
                    continue;
                }
                let id = el.id.as_ref().map(|id| id.0.clone());
                if let Some(id) = &id
                    && terms.iter().any(|term| term.id.as_ref() == Some(id))
                {
                    self.errors.push(problem(&format!(
                        "uses the identifier `{id}` of another term"
                    )));
                    continue;
                }
                terms.push(Term {
                    name: key.clone(),
                    id,
                    description: el.content.clone().unwrap_or_default(),
                });
            }
        }
        self.by_name.insert(
            name.clone(),
            (label.to_string(), Dictionary { name, terms }),
        );
    }

    /// The dictionary with this name.
    pub fn get(&self, name: &str) -> Option<&Dictionary> {
        self.by_name.get(name).map(|(_, dictionary)| dictionary)
    }

    /// The term a mention names. `own` is the dictionary the mentioning document is itself, when it is one,
    /// which is what a mention with nothing on the left of its `#` means.
    pub fn resolve(
        &self,
        mention: &TermRef,
        own: Option<&str>,
    ) -> Result<(&Dictionary, &Term), TermProblem> {
        let in_dictionary = |dictionary: &Option<String>| -> Result<&Dictionary, TermProblem> {
            let name = dictionary
                .as_deref()
                .or(own)
                .ok_or(TermProblem::NeedsDictionary)?;
            self.get(name)
                .ok_or_else(|| TermProblem::UnknownDictionary(name.to_string()))
        };
        match mention {
            TermRef::Id { dictionary, id } => {
                let found = in_dictionary(dictionary)?;
                found
                    .by_id(id)
                    .map(|term| (found, term))
                    .ok_or(TermProblem::NotDefined)
            }
            TermRef::NameIn { dictionary, name } => {
                let found = in_dictionary(dictionary)?;
                found
                    .by_name(name)
                    .map(|term| (found, term))
                    .ok_or(TermProblem::NotDefined)
            }
            TermRef::Name(name) => {
                let mut found = self.iter().filter_map(|dictionary| {
                    dictionary.by_name(name).map(|term| (dictionary, term))
                });
                let first = found.next().ok_or(TermProblem::NotDefined)?;
                let rest: Vec<String> = found
                    .map(|(dictionary, _)| dictionary.name.clone())
                    .collect();
                if rest.is_empty() {
                    Ok(first)
                } else {
                    let mut all = vec![first.0.name.clone()];
                    all.extend(rest);
                    Err(TermProblem::Ambiguous(all))
                }
            }
        }
    }

    /// Every dictionary, in the order of their names.
    pub fn iter(&self) -> impl Iterator<Item = &Dictionary> {
        self.by_name.values().map(|(_, dictionary)| dictionary)
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(sources: &[(&str, &str)]) -> LoadedDictionaries {
        let mut loaded = LoadedDictionaries::default();
        for (label, src) in sources {
            loaded.add(label, &tomet_parser::parse_document(src).expect("parses"));
        }
        loaded
    }

    const TOMET: &str = "::kind(dictionary)\n::dictionary(tomet){\n  \"vault.config.tmt\": @term[ The file that marks a vault. ]#(vault-config)\n  \"ボールト設定\": @term[ Same word. ]\n}\n";

    #[test]
    fn a_term_is_found_by_its_name_and_by_its_identifier() {
        let loaded = loaded(&[("tomet.tmt", TOMET)]);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        let tomet = loaded.get("tomet").expect("tomet");
        let by_name = tomet.by_name("vault.config.tmt").expect("by name");
        assert_eq!(by_name.id.as_deref(), Some("vault-config"));
        assert!(!by_name.description.is_empty());
        assert_eq!(tomet.by_id("vault-config"), Some(by_name));
        // A term needs no identifier, and a Japanese word is a term like any other.
        assert_eq!(tomet.by_name("ボールト設定").expect("ja").id, None);
        assert_eq!(tomet.by_name("nope"), None);
        assert_eq!(tomet.by_id("nope"), None);
    }

    #[test]
    fn a_document_that_is_no_dictionary_is_reported_and_adds_nothing() {
        let loaded = loaded(&[("note.tmt", "::kind(note)\n\nbody\n")]);
        assert!(loaded.is_empty());
        assert!(loaded.errors[0].contains("note.tmt"), "{:?}", loaded.errors);
    }

    #[test]
    fn a_name_two_files_both_claim_belongs_to_the_first() {
        let loaded = loaded(&[("a.tmt", TOMET), ("b.tmt", TOMET)]);
        assert_eq!(loaded.iter().count(), 1);
        assert!(
            loaded
                .errors
                .iter()
                .any(|e| e.contains("b.tmt") && e.contains("a.tmt")),
            "{:?}",
            loaded.errors
        );
    }

    #[test]
    fn an_entry_that_is_not_a_term_and_a_reused_identifier_are_left_out() {
        let loaded = loaded(&[(
            "d.tmt",
            "::dictionary(d){\n  \"a\": @term[ A ]#(x)\n  \"b\": @link(x)\n  \"c\": 3\n  \"d\": @term[ D ]#(x)\n}\n",
        )]);
        let terms: Vec<&str> = loaded
            .get("d")
            .expect("d")
            .terms
            .iter()
            .map(|term| term.name.as_str())
            .collect();
        assert_eq!(terms, ["a"]);
        assert_eq!(loaded.errors.len(), 3, "{:?}", loaded.errors);
        assert!(loaded.errors.iter().any(|e| e.contains("`b`")));
        assert!(loaded.errors.iter().any(|e| e.contains("`c`")));
        assert!(
            loaded
                .errors
                .iter()
                .any(|e| e.contains("`d`") && e.contains("`x`"))
        );
    }

    #[test]
    fn a_mention_is_read_in_its_three_spellings_and_a_plain_word_is_a_name() {
        let id = |d: Option<&str>, i: &str| TermRef::Id {
            dictionary: d.map(str::to_string),
            id: i.to_string(),
        };
        assert_eq!(
            TermRef::parse("tomet#vault-config"),
            id(Some("tomet"), "vault-config")
        );
        assert_eq!(TermRef::parse("#vault-config"), id(None, "vault-config"));
        assert_eq!(
            TermRef::parse("tomet#name(\"vault.config.tmt\")"),
            TermRef::NameIn {
                dictionary: Some("tomet".to_string()),
                name: "vault.config.tmt".to_string()
            }
        );
        assert_eq!(
            TermRef::parse("vault.config.tmt"),
            TermRef::Name("vault.config.tmt".to_string())
        );
        // A word with a `#` in it that is not an address is still a word.
        assert_eq!(
            TermRef::parse("C# language"),
            TermRef::Name("C# language".to_string())
        );
        assert_eq!(TermRef::parse("C#"), TermRef::Name("C#".to_string()));
    }

    #[test]
    fn a_mention_resolves_by_identifier_by_name_and_inside_its_own_dictionary() {
        let loaded = loaded(&[("tomet.tmt", TOMET)]);
        let name = |found: Result<(&Dictionary, &Term), TermProblem>| {
            found.map(|(_, term)| term.name.clone())
        };
        let ok = Ok("vault.config.tmt".to_string());
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("tomet#vault-config"), None)),
            ok
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("tomet#name(\"vault.config.tmt\")"), None)),
            ok
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("vault.config.tmt"), None)),
            ok
        );
        // Written inside the dictionary, a bare `#` is that dictionary.
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("#vault-config"), Some("tomet"))),
            ok
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("#vault-config"), None)),
            Err(TermProblem::NeedsDictionary)
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("nope#vault-config"), None)),
            Err(TermProblem::UnknownDictionary("nope".to_string()))
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("tomet#nope"), None)),
            Err(TermProblem::NotDefined)
        );
        assert_eq!(
            name(loaded.resolve(&TermRef::parse("nope"), None)),
            Err(TermProblem::NotDefined)
        );
    }

    #[test]
    fn a_name_two_dictionaries_define_is_ambiguous_until_one_is_named() {
        let other = "::dictionary(user){ \"vault.config.tmt\": @term[ Theirs. ] }\n";
        let loaded = loaded(&[("tomet.tmt", TOMET), ("user.tmt", other)]);
        assert_eq!(
            loaded
                .resolve(&TermRef::parse("vault.config.tmt"), None)
                .map(|(d, _)| d.name.clone()),
            Err(TermProblem::Ambiguous(vec![
                "tomet".to_string(),
                "user".to_string()
            ]))
        );
        let (dictionary, _) = loaded
            .resolve(&TermRef::parse("user#name(\"vault.config.tmt\")"), None)
            .expect("named");
        assert_eq!(dictionary.name, "user");
    }
}
