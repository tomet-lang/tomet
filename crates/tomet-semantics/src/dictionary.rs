//! Dictionaries of terms: reading a `::dictionary` document, and looking a term up.
//!
//! A dictionary defines words a vault's documents use, once, so that a document mentions one with
//! `@term` instead of writing it out. It is a document of its own, and the terms are its body:
//!
//! ```text
//! ::kind(dictionary)
//! ::id(dictionary.tomet)
//! ::dictionary(tomet)
//!
//! ;@term("vault.config.tmt")#(vault-config)
//! | The file that marks a vault.
//!
//! ;@term("prose")[ The text written in `[]` and `|`. ]
//! ```
//!
//! A `@term` block with a name is a definition. The name is the term, quoted, so a word in any
//! language fits. Its meaning is its `[content]` (a `|` line is that content too) followed by the
//! blocks written after it, up to the next definition. `#(vault-config)` is an optional stable
//! identifier: the term may be renamed and a mention by identifier does not change. A term and an
//! identifier are each unique within their dictionary, the way an id is unique within its document.
//!
//! The same `@term` written inline, anywhere else, is a mention ([`TermRef`]). In a dictionary the
//! definitions are the `@term` blocks of the body, at the top or under a heading; [`Definitions`]
//! says which elements those are, so that nothing checks a definition as if it were a mention.
//!
//! This is the pure half. Finding and reading the files a vault registers with `::dictionary` is
//! `tomet-load`'s, the same split as for vocabularies ([`crate::vocabulary`]).

use std::collections::BTreeMap;

use tomet_ast::{Block, Element, File, Placement};
use tomet_declaration::{DeclKind, dictionary_name};
use tomet_span::Span;
use tomet_tree::ValueExt;

use crate::kind::{ElementKind, classify_std_lenient};

/// One term a dictionary defines.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// The term itself, as written between the quotes.
    pub name: String,
    /// The stable identifier, `#(id)` after the `@term`, when it has one.
    pub id: Option<String>,
    /// What the term means: the `[content]` of its `@term`, then the blocks written after it up to
    /// the next definition. Empty when it has neither.
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

/// The `@term` blocks of a document that define terms, rather than mention them.
///
/// Only a document that is a dictionary has any. A definition is a `@term` block with the document
/// body (or a heading of it) for its parent; a `@term` inside another element, or inline, is a
/// mention even in a dictionary, such as the `@term(#vault-config)` in a description.
#[derive(Debug, Clone, Default)]
pub struct Definitions {
    spans: Vec<Span>,
}

impl Definitions {
    /// The definitions of `doc`: none when it declares no `::dictionary(name)`.
    pub fn of(doc: &File) -> Self {
        let mut found = Vec::new();
        if tomet_declaration::document_dictionary(doc).is_some() {
            definitions_in(&doc.body, &mut found);
        }
        Definitions {
            spans: found
                .iter()
                .map(|definition| definition.element.span)
                .collect(),
        }
    }

    /// Whether `element` is one of the definitions, as against a mention.
    pub fn contains(&self, element: &Element) -> bool {
        // `Span` compares equal to every other span, by design; a position is `exact_eq`.
        self.spans.iter().any(|span| span.exact_eq(&element.span))
    }
}

/// One definition in the body: the `@term` block, and the blocks that follow it.
struct Definition<'a> {
    element: &'a Element,
    after: &'a [Block],
}

fn is_definition(block: &Block) -> Option<&Element> {
    match block {
        Block::Element(el)
            if el.placement == Placement::Block
                && classify_std_lenient(el.as_view()) == ElementKind::Term =>
        {
            Some(el)
        }
        _ => None,
    }
}

/// Collects the definitions of `blocks` and of the headings in them, in document order. What follows a
/// definition belongs to it until the next definition or the next heading.
fn definitions_in<'a>(blocks: &'a [Block], out: &mut Vec<Definition<'a>>) {
    for (index, block) in blocks.iter().enumerate() {
        if let Block::Section(section) = block {
            definitions_in(&section.blocks, out);
        }
        let Some(element) = is_definition(block) else {
            continue;
        };
        let rest = &blocks[index + 1..];
        let end = rest
            .iter()
            .position(|next| is_definition(next).is_some() || matches!(next, Block::Section(_)))
            .unwrap_or(rest.len());
        out.push(Definition {
            element,
            after: &rest[..end],
        });
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
    /// the dictionary it declares, with the terms its body defines.
    ///
    /// Not accepted: a document that declares no `::dictionary(name)`, one that writes its terms
    /// in the retired `{...}` of that declaration, and a name another dictionary already has. Only
    /// the term is left out for a `@term` block with no name, a name defined twice and an
    /// identifier used twice.
    pub fn add(&mut self, label: &str, doc: &File) {
        let Some(found) = tomet_declaration::declarations(doc)
            .into_iter()
            .find(|found| found.kind == DeclKind::Dictionary && found.declaration.is_own_header())
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
        if found.declaration.data.is_some() {
            self.errors.push(format!(
                "declared dictionary {label}: `::dictionary({name}){{...}}` is retired; write each term \
                 as a block of the document, `;@term(\"word\")[meaning]#(id)`"
            ));
            return;
        }
        if let Some((held, _)) = self.by_name.get(&name) {
            self.errors.push(format!(
                "declared dictionary {label}: `{name}` is already the dictionary of {held}"
            ));
            return;
        }

        let mut defined = Vec::new();
        definitions_in(&doc.body, &mut defined);
        let mut terms: Vec<Term> = Vec::new();
        for definition in defined {
            let el = definition.element;
            let Some(term) = term_name(el) else {
                self.errors.push(format!(
                    "dictionary {label}: a @term block names no term; write `;@term(\"word\")`"
                ));
                continue;
            };
            let problem = |what: &str| format!("dictionary {label}: `{term}` {what}");
            if terms.iter().any(|held| held.name == term) {
                self.errors.push(problem("is defined twice"));
                continue;
            }
            let id = el.id.as_ref().map(|id| id.0.clone());
            if let Some(id) = &id
                && terms.iter().any(|held| held.id.as_ref() == Some(id))
            {
                self.errors.push(problem(&format!(
                    "uses the identifier `{id}` of another term"
                )));
                continue;
            }
            let mut description = el.content.clone().unwrap_or_default();
            description.extend(definition.after.iter().cloned());
            terms.push(Term {
                name: term,
                id,
                description,
            });
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

/// The word a `@term` definition defines: its one argument, as it is written.
fn term_name(el: &Element) -> Option<String> {
    crate::normalized_element_args(el.as_view())?
        .get("target")?
        .as_str()
        .filter(|name| !name.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(sources: &[(&str, &str)]) -> LoadedDictionaries {
        let mut loaded = LoadedDictionaries::default();
        for (label, src) in sources {
            loaded.add(label, &tomet_ast_parser::parse_file(src).expect("parses"));
        }
        loaded
    }

    const TOMET: &str = "::kind(dictionary)\n::dictionary(tomet)\n\n;@term(\"vault.config.tmt\")#(vault-config)\n| The file that marks a vault.\n\n;@term(\"ボールト設定\")[ Same word. ]\n";

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
    fn a_term_unnamed_defined_twice_or_reusing_an_identifier_is_left_out() {
        let loaded = loaded(&[(
            "d.tmt",
            "::dictionary(d)\n\n;@term(\"a\")[ A ]#(x)\n;@term[ no word ]\n;@term(\"a\")[ again ]\n;@term(\"d\")[ D ]#(x)\n;@term(\"e\")[ E ]\n",
        )]);
        let terms: Vec<&str> = loaded
            .get("d")
            .expect("d")
            .terms
            .iter()
            .map(|term| term.name.as_str())
            .collect();
        assert_eq!(terms, ["a", "e"]);
        assert_eq!(loaded.errors.len(), 3, "{:?}", loaded.errors);
        assert!(loaded.errors.iter().any(|e| e.contains("names no term")));
        assert!(
            loaded
                .errors
                .iter()
                .any(|e| e.contains("`a`") && e.contains("twice"))
        );
        assert!(
            loaded
                .errors
                .iter()
                .any(|e| e.contains("`d`") && e.contains("`x`"))
        );
    }

    #[test]
    fn the_meaning_of_a_term_is_its_content_then_the_blocks_after_it() {
        let loaded = loaded(&[(
            "d.tmt",
            "::dictionary(d)\n\nintroduction, belongs to no term\n\n;@term(\"a\")\n| from the pipe\n\nand a paragraph\n\n;@term(\"b\")[ bracketed ]\n\nmore of b\n\n;@term(\"c\")\n",
        )]);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        let d = loaded.get("d").expect("d");
        let blocks = |name: &str| d.by_name(name).expect(name).description.len();
        assert_eq!(blocks("a"), 2);
        assert_eq!(blocks("b"), 2);
        assert_eq!(blocks("c"), 0);
    }

    #[test]
    fn a_term_under_a_heading_is_defined_and_a_heading_ends_the_meaning_before_it() {
        let loaded = loaded(&[(
            "d.tmt",
            "::dictionary(d)\n\n;@term(\"a\")[ A ]\n\n=[ Part two ]\n\n;@term(\"b\")[ B ]\n",
        )]);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        let d = loaded.get("d").expect("d");
        assert!(d.by_name("a").is_some() && d.by_name("b").is_some());
        assert_eq!(d.by_name("a").expect("a").description.len(), 1);
    }

    #[test]
    fn the_retired_brace_body_is_refused_and_says_where_the_terms_go() {
        let loaded = loaded(&[("old.tmt", "::dictionary(old){\n  \"a\": @term[ A ]\n}\n")]);
        assert!(loaded.is_empty());
        assert_eq!(loaded.errors.len(), 1, "{:?}", loaded.errors);
        assert!(loaded.errors[0].contains("old.tmt"));
        assert!(loaded.errors[0].contains("retired"), "{:?}", loaded.errors);
        assert!(loaded.errors[0].contains(";@term("), "{:?}", loaded.errors);
    }

    #[test]
    fn a_registration_is_not_a_dictionary_header() {
        let loaded = loaded(&[(
            "vault.config.tmt",
            "::dictionary.tomet(@file(tomet.dict.tmt))\n",
        )]);
        assert!(loaded.is_empty());
        assert!(
            loaded.errors[0].contains("does not declare"),
            "{:?}",
            loaded.errors
        );
    }

    #[test]
    fn definitions_are_the_term_blocks_of_a_dictionary_and_not_the_mentions_in_them() {
        let doc = tomet_ast_parser::parse_file(
            "::dictionary(d)\n\n;@term(\"a\")#(a)\n| see @term(#b)\n\n;@term(\"b\")#(b)\n",
        )
        .expect("parses");
        let definitions = Definitions::of(&doc);
        let mut defined = 0;
        let mut mentioned = 0;
        tomet_tree::for_each_element(&doc, |el| {
            if classify_std_lenient(el.as_view()) != ElementKind::Term {
                return;
            }
            if definitions.contains(el) {
                defined += 1;
            } else {
                mentioned += 1;
            }
        });
        assert_eq!((defined, mentioned), (2, 1));

        // A document that is no dictionary defines nothing: its block `@term` is a mention.
        let note = tomet_ast_parser::parse_file("::kind(note)\n\n;@term(\"a\")\n").expect("parses");
        let definitions = Definitions::of(&note);
        tomet_tree::for_each_element(&note, |el| assert!(!definitions.contains(el)));
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
        let other = "::dictionary(user)\n\n;@term(\"vault.config.tmt\")[ Theirs. ]\n";
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
