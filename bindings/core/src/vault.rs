//! The vault a document is read in.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use tomet::ast::File;
use tomet::config::PrinterConfig;
use tomet_position::LineIndex;
use tomet_transform::{LinkCatalog, LinkSite, SourceUrl, resolve_site_links};

use crate::process::outline;
use crate::{Diagnostic, ProcessOptions, ProcessedFile};

/// Where the source of the project is on the web, for what a page has no page for: a `file:` or `dir:`
/// link to a file that is not a published document.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SourceUrls {
    /// The URL of a file, with `{path}` where its path from the root of the vault goes (each segment
    /// percent-encoded): `https://github.com/org/repo/blob/<revision>/{path}`.
    pub file: String,
    /// The URL of a directory, with `{path}` as in `file`: `https://github.com/org/repo/tree/<revision>/{path}`.
    pub dir: String,
}

/// What a vault is made of. Everything is optional: `{}` is a vault of nothing, in which a document is
/// read with `std` alone and links go nowhere.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct VaultSources {
    /// The path of every file of the vault, from its root, with `/` between the segments. A `ref:` link
    /// and an embed name one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// The documents the site has a page for, a part of `files`. Defaults to every `.tmt`, `.tm` and
    /// `.md` of `files`. A link to a document that is not here is a link to nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documents: Option<Vec<String>>,
    /// The settings, as the text of a config document (`vault.config.tmt`): macros, variables, and
    /// the `type:` of `::meta` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// The vocabularies, as the text of the documents the config registers, by the path the config
    /// gives them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vocabularies: BTreeMap<String, String>,
    /// The dictionaries, as the text of the documents the config registers, by the path the config gives them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dictionaries: BTreeMap<String, String>,
    /// The blueprints, as text. A document is checked against the one for its `::kind`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blueprints: Vec<String>,
    /// Where the pages of the site are: `/docs` gives `/docs/spec/syntax`. Defaults to `/docs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_prefix: Option<String>,
    /// Where the files that are not documents are: `/vault` gives `/vault/img/x.png`. Defaults to `/vault`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_prefix: Option<String>,
    /// Where the files given are in the vault, from its root, when they are a part of it: `tmtroot`
    /// if the files are what is under `tmtroot/`. A path in a document is written from the root of the
    /// vault, and this is what it is read from. Defaults to `""`: the files are the vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
    /// Where the source of the project is on the web. Without it, a link to a file that has no page is
    /// a diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<SourceUrls>,
}

/// The files the vault registers, which a host reads to make the [`VaultSources`] of it.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Registry {
    /// The vocabulary documents, as paths from the directory of the config.
    pub vocabularies: Vec<String>,
    /// The dictionary documents, as paths from the directory of the config.
    pub dictionaries: Vec<String>,
    /// The blueprint documents, as paths from the directory of the config.
    pub blueprints: Vec<String>,
}

/// The files `config` registers: what a host reads, so that it does not read a config itself.
pub fn registry(config: &str) -> Result<Registry, String> {
    let config = tomet::config::load_config_from_str(config).map_err(|e| format!("config: {e}"))?;
    Ok(Registry {
        vocabularies: config.registry.vocabularies,
        dictionaries: config.registry.dictionaries,
        blueprints: config.registry.blueprints,
    })
}

/// Reads `text` in a vault of nothing: what a vocabulary, a dictionary or a blueprint is read as, since
/// none of them is read in the vault it is a part of.
pub(crate) fn parse_plain(text: &str) -> Result<File, String> {
    tomet::vault::Vault::from_sources(PrinterConfig::default(), ".", &[])
        .parse(text)
        .map(|(doc, _)| doc)
        .map_err(|e| e.to_string())
}

/// A document to read: its text, or its tree.
pub enum Input<'a> {
    Source(&'a str),
    File(File),
}

/// The files of the vault, as a site sees them.
struct Catalog {
    files: HashSet<String>,
    directories: HashSet<String>,
    documents: HashSet<String>,
    index: tomet_links::VaultLinkIndex,
}

fn is_document(path: &str) -> bool {
    [".tmt", ".tm", ".md"].iter().any(|ext| path.ends_with(ext))
}

impl Catalog {
    fn new(files: &[String], documents: Option<&[String]>) -> Self {
        let mut directories = HashSet::new();
        for file in files {
            let mut end = 0;
            while let Some(at) = file[end..].find('/') {
                end += at;
                directories.insert(file[..end].to_string());
                end += 1;
            }
        }
        let documents = match documents {
            Some(given) => given.iter().cloned().collect(),
            None => files.iter().filter(|f| is_document(f)).cloned().collect(),
        };
        Self {
            files: files.iter().cloned().collect(),
            directories,
            documents,
            index: tomet_links::VaultLinkIndex::from_paths(files),
        }
    }
}

impl LinkCatalog for Catalog {
    fn resolve_ref(&self, name: &str, from: Option<&str>) -> Option<String> {
        self.index
            .resolve_ref(name, from.map(Path::new))
            .map(|path| path.to_string_lossy().replace('\\', "/"))
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains(path) || self.directories.contains(path)
    }

    fn is_published(&self, path: &str) -> bool {
        self.documents.contains(path)
    }
}

/// A vault, ready to read documents in: its settings, vocabularies and dictionaries, and its files.
///
/// It is made once and read many times, and holds its own state: two vaults never see each other's files.
pub struct Vault {
    core: tomet::vault::Vault,
    blueprints: Vec<File>,
    catalog: Catalog,
    url_prefix: String,
    asset_prefix: String,
    base_path: String,
    source_url: Option<SourceUrl>,
}

impl Vault {
    /// A vault of `sources`. Fails if a part of it is not usable: a config, a vocabulary, a
    /// dictionary or a blueprint that does not read, or that the vault does not accept.
    pub fn new(sources: &VaultSources) -> Result<Self, String> {
        let config = match &sources.config {
            Some(text) => {
                tomet::config::load_config_from_str(text).map_err(|e| format!("config: {e}"))?
            }
            None => PrinterConfig::default(),
        };
        let vocabularies: Vec<(&str, &str)> = sources
            .vocabularies
            .iter()
            .map(|(label, text)| (label.as_str(), text.as_str()))
            .collect();
        let mut dictionaries = tomet::semantics::LoadedDictionaries::default();
        for (label, text) in &sources.dictionaries {
            match parse_plain(text) {
                Ok(doc) => dictionaries.add(label, &doc),
                Err(e) => dictionaries
                    .errors
                    .push(format!("declared dictionary {label}: {e}")),
            }
        }
        let core = tomet::vault::Vault::from_sources(config, ".", &vocabularies)
            .with_dictionaries(dictionaries);
        let problems: Vec<&String> = core.setup_problems().collect();
        if !problems.is_empty() {
            return Err(problems
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        }
        let blueprints = sources
            .blueprints
            .iter()
            .enumerate()
            .map(|(i, text)| parse_plain(text).map_err(|e| format!("blueprints[{i}]: {e}")))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            core,
            blueprints,
            catalog: Catalog::new(&sources.files, sources.documents.as_deref()),
            url_prefix: sources.url_prefix.clone().unwrap_or_else(|| "/docs".into()),
            asset_prefix: sources
                .asset_prefix
                .clone()
                .unwrap_or_else(|| "/vault".into()),
            base_path: sources.base_path.clone().unwrap_or_default(),
            source_url: sources.source_url.as_ref().map(|urls| SourceUrl {
                file: urls.file.clone(),
                dir: urls.dir.clone(),
            }),
        })
    }

    /// A vault of nothing: `std` alone, and no file to link to.
    pub fn empty() -> Self {
        Self::new(&VaultSources::default()).expect("a vault of nothing is usable")
    }

    fn parse(&self, input: Input<'_>) -> Result<(File, Option<String>), String> {
        match input {
            Input::Source(src) => {
                let (doc, _) = self.core.parse(src).map_err(|e| e.to_string())?;
                Ok((doc, Some(src.to_string())))
            }
            Input::File(doc) => Ok((doc, None)),
        }
    }

    /// What `tomet check` finds in a document: nothing is read but the vault, so what needs another
    /// document is not reported.
    pub fn check(&self, input: Input<'_>) -> Result<Vec<Diagnostic>, String> {
        let (doc, text) = self.parse(input)?;
        let index = text.as_deref().map(LineIndex::new);
        let mut found: Vec<Diagnostic> = self
            .core
            .diagnostics(&doc, &self.blueprints)
            .iter()
            .map(|d| Diagnostic::from_check(d, index.as_ref()))
            .collect();
        found.sort_by_key(|d| d.span.start);
        Ok(found)
    }

    /// Reads a document for a page: its expressions resolved, its links turned into the URLs they have
    /// on the site, its body rendered, and everything found along the way as diagnostics.
    ///
    /// Fails only if the document does not parse. A document that is wrong is still rendered.
    pub fn process(
        &self,
        input: Input<'_>,
        options: &ProcessOptions,
    ) -> Result<ProcessedFile, String> {
        let (mut doc, text) = self.parse(input)?;
        let index = text.as_deref().map(LineIndex::new);
        let index = index.as_ref();

        // The document as written is what is checked, so the spans are the ones in the text.
        let mut diagnostics: Vec<Diagnostic> = self
            .core
            .diagnostics(&doc, &self.blueprints)
            .iter()
            .map(|d| Diagnostic::from_check(d, index))
            .collect();

        let here = options.current_path.as_deref();
        let scope = self.core.scope_for(
            &doc,
            Path::new(here.unwrap_or("document.tmt")),
            &tomet::vault::NoResolver,
        );
        let blueprint = tomet::declaration::document_kind(&doc).as_deref() == Some("blueprint");
        let unresolved = tomet_eval::resolve_expressions(&mut doc, &scope.context());
        diagnostics.extend(
            unresolved
                .iter()
                // A blueprint is written to be filled in: a variable with no value yet is as it should be.
                .filter(|u| !(blueprint && u.awaits_input))
                .map(|u| Diagnostic::from_unresolved(u, index)),
        );
        self.core.fill_terms(&mut doc);

        let site = LinkSite {
            url_prefix: &self.url_prefix,
            asset_prefix: &self.asset_prefix,
            base_path: &self.base_path,
            source_url: self.source_url.as_ref(),
            catalog: &self.catalog,
        };
        diagnostics.extend(
            resolve_site_links(&mut doc, here, &site)
                .iter()
                .map(|l| Diagnostic::from_link(l, index)),
        );
        diagnostics.sort_by_key(|d| d.span.start);

        let html = tomet::html::render_body_with(
            &doc,
            &self.core.bindings(&doc),
            &options.to_render_options(),
        );
        let is_data_only = html.trim().is_empty();

        let meta_value = tomet::declaration::document_meta(&doc);
        let meta = match &meta_value {
            Some(tomet::ast::Value::Tuple(m)) => {
                Some(tomet::ast::PlainValue::Object(m.plain_entries().collect()))
            }
            _ => None,
        };
        let meta_title = meta_value.as_ref().and_then(|v| match v {
            tomet::ast::Value::Tuple(m) => m
                .get("title")
                .and_then(|val| val.as_scalar().and_then(|s| s.text()))
                .map(|t| t.into_owned()),
            _ => None,
        });
        let (first_h1, toc) = outline(&html);

        Ok(ProcessedFile {
            title: meta_title.or(first_h1),
            html,
            meta,
            header: doc.header,
            toc,
            is_data_only,
            diagnostics,
        })
    }
}
