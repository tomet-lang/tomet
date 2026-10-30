use std::io::{self, Write};
use std::path::Path;

use rayon::prelude::*;
use tomet_lexer::LineIndex;
use tomet_load::{Findings, is_error};
use tomet_validator::Diagnostic;

use crate::util::{format_parse_error, read};

enum CheckFileResult {
    ReadError,
    ParseError {
        json_entry: Option<serde_json::Value>,
        text: String,
    },
    Checked {
        warnings: usize,
        has_errors: bool,
        stderr_lines: Vec<String>,
        json_entries: Vec<serde_json::Value>,
    },
}

/// Checks one file, or every `.tmt` and `.ctmt` under a directory.
///
/// A `.ctmt` is a standalone cotomet file, not a document: it is parsed as
/// cotomet and must be a map of `key: value` entries, with its header
/// declarations validated against [`tomet_declaration::Host::Ctmt`].
///
/// Checking is not parsing. This used to return OK the moment a document
/// parsed, so two `::meta`, or an element name that exists nowhere, passed
/// -- while the js/java/python bindings and `apps/web` all ran the
/// validator. Vocabularies have to be read from disk, which is why the
/// resolving lives here and not in the validator.
///
/// Multi-file checking runs in parallel across rayon worker threads governed
/// by the jobserver gate, and produces all output (stderr diagnostics, summary,
/// and JSON) in file order after every file has completed. Unlike the earlier
/// serial loop that printed diagnostics as each file was checked, nothing is
/// printed while files are being processed, and for a large vault with many
/// findings the full result is held in memory until the end.
pub(crate) fn check(path: &Path, data: bool, quiet: bool, json: bool) -> anyhow::Result<()> {
    if data {
        if !path.is_file() {
            return Err(anyhow::anyhow!(
                "--data takes one file, not a directory: {}",
                path.display()
            ));
        }
        let src = read(path)?;
        return match tomet_parser::parse_value(&src) {
            Ok(_) => {
                if !quiet {
                    println!("OK");
                }
                Ok(())
            }
            Err(err) => {
                report_parse_error(path, &src, &err, json);
                Err(anyhow::anyhow!("check failed"))
            }
        };
    }

    // Resolved once for the whole run. Per file it would re-read and
    // re-parse every declared vocabulary, which for this repository is
    // seven files times eighty-five documents -- which is why the front
    // door has a vault and a per-document call rather than one function.
    let vault = tomet_load::Vault::discover(path);
    if !vault.vocabulary_errors.is_empty() {
        let mut err_out = io::stderr().lock();
        for problem in &vault.vocabulary_errors {
            let _ = writeln!(err_out, "warning: {problem}");
        }
    }

    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        tomet_indexer::collect_tm_and_ctmt_files_with_config(path, &vault.config, &vault.root)
    } else {
        return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
    };

    let mut checked = 0usize;
    let mut failed = 0usize;
    let mut warned = 0usize;
    let mut json_files: Vec<serde_json::Value> =
        Vec::with_capacity(if json { files.len() } else { 0 });

    // Phase 1: Per-file, in parallel, pure
    let results: Vec<CheckFileResult> = files
        .par_iter()
        .map(|file| {
            let _token = jobserver_gate::acquire();
            let Ok(src) = std::fs::read_to_string(file) else {
                return CheckFileResult::ReadError;
            };

            if tomet_indexer::is_ctmt_file(file) {
                return check_ctmt(file, &src, json);
            }

            let doc = match tomet_parser::parse_document(&src) {
                Ok(doc) => doc,
                Err(err) => return parse_failure(file, &src, &err, json),
            };

            let findings = vault.findings(file, &doc);
            let index = tomet_lexer::LineIndex::new(&src);
            let warnings = findings.warnings();
            let has_errors = findings.has_errors();
            let Findings {
                diagnostics,
                unresolved,
            } = findings;

            let mut stderr_lines = Vec::new();
            let mut json_entries = Vec::new();

            if json {
                for u in &unresolved {
                    json_entries.push(serde_json::json!({
                        "file": file.display().to_string(),
                        "errors": [{
                            "message": u.message(),
                            "severity": "warning",
                        }],
                    }));
                }
            } else {
                for u in &unresolved {
                    stderr_lines.push(format!(
                        "{}:{}: warning: {}",
                        file.display(),
                        index.line_col(u.span.start).line,
                        u.message()
                    ));
                }
            }

            format_diagnostics(
                file,
                &diagnostics,
                &[],
                &index,
                json,
                &mut stderr_lines,
                &mut json_entries,
            );

            CheckFileResult::Checked {
                warnings,
                has_errors,
                stderr_lines,
                json_entries,
            }
        })
        .collect();
    jobserver_gate::reclaim_implicit_token();

    // Phase 2: In order, serial
    let mut err_out = io::stderr().lock();
    for (file, res) in files.iter().zip(results) {
        match res {
            CheckFileResult::ReadError => {
                let _ = writeln!(err_out, "could not read {}", file.display());
                failed += 1;
            }
            CheckFileResult::ParseError { json_entry, text } => {
                if let Some(entry) = json_entry {
                    json_files.push(entry);
                } else {
                    let _ = writeln!(err_out, "{text}");
                }
                failed += 1;
            }
            CheckFileResult::Checked {
                warnings,
                has_errors,
                stderr_lines,
                json_entries,
            } => {
                checked += 1;
                warned += warnings;
                if has_errors {
                    failed += 1;
                }
                if json {
                    json_files.extend(json_entries);
                } else {
                    for line in stderr_lines {
                        let _ = writeln!(err_out, "{line}");
                    }
                }
            }
        }
    }
    drop(err_out);

    if json {
        let mut out = io::stdout().lock();
        writeln!(out, "{}", serde_json::to_string_pretty(&json_files)?)?;
    }

    if failed > 0 {
        return Err(anyhow::anyhow!(
            "{failed} of {} file(s) failed",
            files.len()
        ));
    }
    if !quiet {
        // The count is printed on a green run too. A document with gaps
        // still passes, and saying so is the only way the marks stay
        // visible rather than becoming decoration.
        let mut out = io::stdout().lock();
        let gaps = if warned > 0 {
            format!(" ({warned} warning(s))")
        } else {
            String::new()
        };
        if files.len() == 1 {
            writeln!(out, "OK{gaps}")?;
        } else {
            writeln!(out, "OK: {checked} file(s){gaps}")?;
        }
    }
    Ok(())
}

fn format_diagnostics(
    file: &Path,
    diagnostics: &[Diagnostic],
    eval_errors: &[cotomet_eval::Error],
    index: &LineIndex,
    json: bool,
    stderr_lines: &mut Vec<String>,
    json_entries: &mut Vec<serde_json::Value>,
) {
    if diagnostics.is_empty() && eval_errors.is_empty() {
        return;
    }
    if json {
        let mut error_list: Vec<serde_json::Value> = diagnostics
            .iter()
            .map(|d| {
                serde_json::json!({
                    "message": d.message_in(index),
                    "severity": if is_error(d) { "error" } else { "warning" },
                })
            })
            .collect();
        for e in eval_errors {
            error_list.push(serde_json::json!({
                "message": e.message,
                "line": e.line,
                "column": e.column,
                "severity": "error",
            }));
        }
        json_entries.push(serde_json::json!({
            "file": file.display().to_string(),
            "errors": error_list,
        }));
    } else {
        for d in diagnostics {
            let label = if is_error(d) { "" } else { "warning: " };
            stderr_lines.push(format!(
                "{}: {label}{}",
                file.display(),
                d.message_in(index)
            ));
        }
        for err in eval_errors {
            stderr_lines.push(format!(
                "{}:{}:{}: {}",
                file.display(),
                err.line,
                err.column,
                err.message
            ));
        }
    }
}

/// Check a standalone cotomet file (`.ctmt`).
///
/// Declarations at the top are validated by `check_ctmt_declarations`. If the
/// evaluator also reports errors within header declarations (such as a malformed
/// `::vars` or `::macros` entry), those duplicates are dropped in favor of the
/// declaration check's diagnostic so each problem is reported once.
fn check_ctmt(file: &Path, src: &str, json: bool) -> CheckFileResult {
    let mut hook = tomet_parser::CotometValueHook;
    let parsed = match cotomet::syntax::parse_file_with(src, &mut hook) {
        Ok(parsed) => parsed,
        Err(err) => return parse_failure(file, src, &err.into(), json),
    };

    let header_spans: Vec<_> = parsed.header.iter().map(|d| d.span).collect();

    let declared: Vec<_> = parsed
        .header
        .iter()
        .map(|d| tomet_validator::Declared::new(tomet_declaration::Declaration::from(d), d.span))
        .collect();
    let mut diagnostics = Vec::new();
    tomet_validator::check_ctmt_declarations(&declared, &mut diagnostics);

    let eval_res = cotomet_eval::eval_file(parsed, src, &[]);
    let eval_errors: Vec<_> = match eval_res {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .into_iter()
            .filter(|e| !header_spans.iter().any(|s| s.contains_offset(e.span.start)))
            .collect(),
    };

    let index = tomet_lexer::LineIndex::new(src);
    let warnings = diagnostics.iter().filter(|d| !is_error(d)).count();
    let has_errors = diagnostics.iter().any(is_error) || !eval_errors.is_empty();

    let mut stderr_lines = Vec::new();
    let mut json_entries = Vec::new();
    format_diagnostics(
        file,
        &diagnostics,
        &eval_errors,
        &index,
        json,
        &mut stderr_lines,
        &mut json_entries,
    );

    CheckFileResult::Checked {
        warnings,
        has_errors,
        stderr_lines,
        json_entries,
    }
}

/// One file that did not parse, as the sweep reports it: JSON when asked for,
/// else the source line with a caret.
fn parse_failure(file: &Path, src: &str, err: &tomet_parser::Error, json: bool) -> CheckFileResult {
    let (json_entry, text) = if json {
        (
            Some(serde_json::json!({
                "file": file.display().to_string(),
                "error": {
                    "message": err.message,
                    "line": err.line,
                    "column": err.column,
                    "offset": err.offset,
                }
            })),
            String::new(),
        )
    } else {
        (None, format_parse_error(file, src, err))
    };
    CheckFileResult::ParseError { json_entry, text }
}

fn report_parse_error(file: &Path, src: &str, err: &tomet_parser::Error, json: bool) {
    if json {
        let value = serde_json::json!({
            "file": file.display().to_string(),
            "error": {
                "message": err.message,
                "line": err.line,
                "column": err.column,
                "offset": err.offset,
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        eprintln!("{}", format_parse_error(file, src, err));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn vault(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tomet_test_check_{name}_{}", nanoid::nanoid!()));
        fs::create_dir_all(dir.join(".tomet/vocabularies")).unwrap();
        fs::write(
            dir.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ vocabularies: list(\".tomet/vocabularies/deck.vocabulary.tmt\") }\n",
        )
        .unwrap();
        fs::write(
            dir.join(".tomet/vocabularies/deck.vocabulary.tmt"),
            "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{ card: {} }\n",
        )
        .unwrap();
        dir
    }

    /// A directory is swept, and one bad file does not hide the rest --
    /// every failure is reported and the run ends non-zero.
    #[test]
    fn a_directory_is_swept_and_reports_every_failure() {
        let root = vault("sweep");
        fs::write(root.join("good.tmt"), "::kind(deck)\n\n@card{}\n").unwrap();
        fs::write(root.join("bad.tmt"), "::kind(deck)\n\n@nonesuch{}\n").unwrap();
        fs::write(
            root.join("worse.tmt"),
            "::kind(deck)\n::meta{a:1}\n::meta{b:2}\n",
        )
        .unwrap();

        let err = check(&root, false, true, false).expect_err("two files are bad");
        let message = err.to_string();
        assert!(message.contains("2 of"), "{message}");

        // And the good one alone passes, so the sweep is not failing
        // everything indiscriminately.
        check(&root.join("good.tmt"), false, true, false).expect("the good file passes");

        let _ = fs::remove_dir_all(&root);
    }

    /// A `.ctmt` is checked next to the `.tmt` files: parsed as cotomet, and a
    /// file is a map of `key: value` entries. One bad file does not hide
    /// the others, and a directory run reaches it.
    #[test]
    fn a_cotomet_file_is_checked_beside_the_documents() {
        let root = vault("ctmt");
        fs::write(root.join("good.tmt"), "::kind(deck)\n\n@card{}\n").unwrap();
        fs::write(
            root.join("ok.ctmt"),
            "// header\n\"tmtroot/a.tmt\": \"../a.tmt\"\nm: { x: 1 }\n",
        )
        .unwrap();
        fs::write(root.join("empty.ctmt"), "").unwrap();
        check(&root, false, true, false).expect("documents and good cotomet files pass");
        check(&root.join("ok.ctmt"), false, true, false).expect("a file named outright");

        // a syntax error, a lone scalar, a bare value among the entries
        for (name, src) in [
            ("syntax.ctmt", "a: \"never closed\n"),
            ("scalar.ctmt", "123\n"),
            ("bare.ctmt", "a: 1\nstray\n"),
            ("dup.ctmt", "a: 1\na: 2\n"),
        ] {
            fs::write(root.join(name), src).unwrap();
            let err = check(&root.join(name), false, true, false)
                .expect_err(&format!("{name} is not a cotomet file"));
            assert!(err.to_string().contains("1 of"), "{name}: {err}");
            fs::remove_file(root.join(name)).unwrap();
        }

        // a bad one in a directory is counted, and the good files around it are not
        fs::write(root.join("bad.ctmt"), "a: 1\nstray\n").unwrap();
        let err = check(&root, false, true, false).expect_err("one bad cotomet file");
        assert!(err.to_string().starts_with("1 of"), "{err}");

        let _ = fs::remove_dir_all(&root);
    }

    /// The config's exclusions apply to a `.ctmt` as they do to a `.tmt`.
    #[test]
    fn a_cotomet_file_the_config_leaves_alone_is_not_swept() {
        let root = vault("ctmt_unswept");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ workspace: { unswept: list(\"frozen\") } }\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("frozen")).unwrap();
        fs::write(root.join("frozen/bad.ctmt"), "stray\n").unwrap();
        check(&root, false, true, false).expect("the frozen directory is not swept");

        let _ = fs::remove_dir_all(&root);
    }

    /// A field the settings declare `type: uuid` has to hold a UUID. The
    /// declaration is the vault's, so it reaches the check from the config,
    /// not from anything the document says.
    #[test]
    fn a_uuid_field_is_checked_against_the_settings_declaration() {
        let root = vault("uuid");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n",
        )
        .unwrap();
        fs::write(
            root.join("good.tmt"),
            "::id(123e4567-e89b-42d3-a456-426614174000)\n",
        )
        .unwrap();
        fs::write(root.join("bad.tmt"), "::id(not-a-uuid)\n").unwrap();

        check(&root.join("good.tmt"), false, true, false).expect("a UUID passes");
        let err = check(&root.join("bad.tmt"), false, true, false).expect_err("not a UUID");
        assert!(err.to_string().contains("1 of"), "{err}");

        // The same document is fine where the settings declare nothing.
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{}\n",
        )
        .unwrap();
        check(&root.join("bad.tmt"), false, true, false).expect("nothing declares `id`");

        let _ = fs::remove_dir_all(&root);
    }

    /// What stays unresolved in an included file is reported in that file,
    /// in full. The file that includes it says only that it brought some
    /// in, once per `include`, so the same fault is not counted again for
    /// every file above it.
    #[test]
    fn an_include_says_it_brought_in_what_the_included_file_left_unresolved() {
        let root = vault("include");
        fs::write(root.join("b.tmt"), "One ${nope}, two ${#missing}.\n").unwrap();
        fs::write(root.join("a.tmt"), "${include(tmt(\"b.tmt\"))}\n").unwrap();
        fs::write(root.join("c.tmt"), "${include(tmt(\"a.tmt\"))}\n").unwrap();
        fs::write(root.join("clean.tmt"), "Nothing to resolve.\n").unwrap();
        fs::write(root.join("d.tmt"), "${include(tmt(\"clean.tmt\"))}\n").unwrap();

        let vault = tomet_load::Vault::discover(&root);
        let unresolved = |name: &str| {
            let path = root.join(name);
            let doc = tomet_parser::parse_document(&fs::read_to_string(&path).unwrap()).unwrap();
            vault.findings(&path, &doc).unresolved
        };

        // The included file itself: both, each with its own reason.
        assert_eq!(unresolved("b.tmt").len(), 2);

        // The file that includes it: one, naming the file, not the two again.
        let a = unresolved("a.tmt");
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(
            a[0].reason.contains("`b.tmt` has 2 unresolved items"),
            "{a:?}"
        );

        // And the one above that counts the one it was told of, not the two.
        let c = unresolved("c.tmt");
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(
            c[0].reason.contains("`a.tmt` has 1 unresolved item,"),
            "{c:?}"
        );

        // An include of a file with nothing left unresolved says nothing.
        assert!(unresolved("d.tmt").is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    /// The vault's `::kind` binding reaches every file in the sweep, not
    /// just the first -- the vocabularies are loaded once for the run.
    #[test]
    fn the_vocabulary_reaches_every_file_in_the_sweep() {
        let root = vault("shared");
        for name in ["a.tmt", "b.tmt", "c.tmt"] {
            fs::write(root.join(name), "::kind(deck)\n\n@card{}\n").unwrap();
        }
        check(&root, false, true, false).expect("all three resolve `card`");
        let _ = fs::remove_dir_all(&root);
    }

    /// `--data` parses one document with the value grammar; a directory
    /// of those is not a thing, so it says so rather than sweeping.
    #[test]
    fn data_mode_refuses_a_directory() {
        let root = vault("data");
        let err = check(&root, true, true, false).expect_err("a directory is refused");
        assert!(err.to_string().contains("--data takes one file"));
        let _ = fs::remove_dir_all(&root);
    }

    /// Dangling intra-document anchor links are caught and fail the check.
    #[test]
    fn dangling_anchor_link_fails_check() {
        let root = vault("dangling_link");
        fs::write(root.join("bad.tmt"), "@link(#nonexistent)\n").unwrap();
        fs::write(
            root.join("good.tmt"),
            "=[ Intro ]#(intro)\n\n@link(#intro)\n",
        )
        .unwrap();

        let err = check(&root.join("bad.tmt"), false, true, false).expect_err("bad file fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        check(&root.join("good.tmt"), false, true, false).expect("good file passes");

        let _ = fs::remove_dir_all(&root);
    }

    /// Blueprint conformance (required meta and section :rule constraints) is checked automatically.
    #[test]
    fn blueprint_conformance_fails_check() {
        let root = vault("blueprint_check");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(settings)\n::settings{\n  blueprints: list(\"bp.tmt\")\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("bp.tmt"),
            "::kind(blueprint)\n::blueprint(note)\n::meta{\n  date: \"2026\"\n}\n\n=[ Items ]#(items):rule(allow: list(task))\n",
        )
        .unwrap();

        // bad.tmt has a disallowed heading inside the items section
        fs::write(
            root.join("bad.tmt"),
            "::kind(note)\n::meta{\n  date: \"2026-10-13\"\n}\n\n=[ Items ]#(items)\n@heading[ Disallowed ]\n",
        )
        .unwrap();

        // good.tmt adheres to the blueprint
        fs::write(
            root.join("good.tmt"),
            "::kind(note)\n::meta{\n  date: \"2026-10-13\"\n}\n\n=[ Items ]#(items)\n@task[ Clean room ]\n",
        )
        .unwrap();

        let err = check(&root.join("bad.tmt"), false, true, false).expect_err("bad file fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        check(&root.join("good.tmt"), false, true, false).expect("good file passes");

        let _ = fs::remove_dir_all(&root);
    }

    /// Declarations at the top of a .ctmt file are validated against Host::Ctmt.
    #[test]
    fn cotomet_header_declarations_are_checked() {
        let root = vault("ctmt_declarations");

        fn ctmt_messages(file: &Path, src: &str) -> Vec<String> {
            match check_ctmt(file, src, false) {
                CheckFileResult::Checked { stderr_lines, .. } => stderr_lines
                    .into_iter()
                    .map(|l| {
                        let prefix = format!("{}: ", file.display());
                        l.strip_prefix(&prefix).unwrap_or(&l).to_string()
                    })
                    .collect(),
                _ => panic!("expected CheckFileResult::Checked"),
            }
        }

        // 1. Valid standard declarations pass
        let good_src = "::kind(catalog, 1)\n::version(1.0.0)\n::meta{\n  author: \"me\"\n}\n::macros{\n  name: \"val\"\n}\n::vars{\n  n: { type: int, default: 1 }\n}\nkey: 42\n";
        let good_file = root.join("good.ctmt");
        fs::write(&good_file, good_src).unwrap();
        assert_eq!(ctmt_messages(&good_file, good_src), Vec::<String>::new());
        check(&good_file, false, true, false).expect("valid ctmt passes");

        // 2. A file that opens with non-standard ::upstreams{ ... } checks clean
        let upstreams_src = "::upstreams{\n  vocabularies: { git: \"https://github.com/tomet-lang/vocabularies.git\", rev: main, root: vocabularies }\n  commonfiles: { git: \"https://github.com/tomet-lang/commonfiles.git\", rev: main, root: base }\n}\n\"tmtroot/schema/vocabularies/rust@1.0.0.vocabulary.tmt\": { source: { upstream: vocabularies, path: \"...\" } }\n";
        let upstreams_file = root.join("upstreams.ctmt");
        fs::write(&upstreams_file, upstreams_src).unwrap();
        assert_eq!(
            ctmt_messages(&upstreams_file, upstreams_src),
            Vec::<String>::new()
        );
        check(&upstreams_file, false, true, false).expect("::upstreams checks clean");

        // 3. The same file with ::kind(catalog, 1.0) added reports version problem only
        let bad_kind_src = "::kind(catalog, 1.0)\n::upstreams{\n  vocabularies: { git: \"https://github.com/tomet-lang/vocabularies.git\", rev: main, root: vocabularies }\n}\nkey: 1\n";
        let bad_kind_file = root.join("upstreams_bad_kind.ctmt");
        fs::write(&bad_kind_file, bad_kind_src).unwrap();
        let msgs = ctmt_messages(&bad_kind_file, bad_kind_src);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0],
            "the version in `::kind(...)` must be an integer of 1 or more, found 1.0"
        );
        assert!(!msgs[0].contains("upstreams"));
        let err = check(&bad_kind_file, false, true, false).expect_err("bad ::kind version fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        // 4. NotForThisHost (::use in .ctmt) fails
        let not_for_host_src = "::use(deck)\nkey: 1\n";
        let not_for_host_file = root.join("not_for_host.ctmt");
        fs::write(&not_for_host_file, not_for_host_src).unwrap();
        let msgs = ctmt_messages(&not_for_host_file, not_for_host_src);
        assert_eq!(msgs, vec!["`::use` is not permitted in this host"]);
        let err = check(&not_for_host_file, false, true, false).expect_err("::use in .ctmt fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        // 5. Duplicate singleton (two ::meta) fails
        let dup_src = "::meta{ a: 1 }\n::meta{ b: 2 }\nkey: 1\n";
        let dup_file = root.join("duplicate.ctmt");
        fs::write(&dup_file, dup_src).unwrap();
        let msgs = ctmt_messages(&dup_file, dup_src);
        assert_eq!(
            msgs,
            vec!["`meta` may appear once in a document; the first is at 1:1"]
        );
        let err = check(&dup_file, false, true, false).expect_err("duplicate ::meta fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        let _ = fs::remove_dir_all(&root);
    }

    /// Expressions in a .ctmt file are evaluated and reported with line and column.
    #[test]
    fn cotomet_eval_expressions_are_checked() {
        let root = vault("ctmt_eval_expressions");

        fn ctmt_errors(file: &Path, src: &str) -> Vec<String> {
            match check_ctmt(file, src, false) {
                CheckFileResult::Checked { stderr_lines, .. } => stderr_lines,
                _ => panic!("expected CheckFileResult::Checked"),
            }
        }

        // 1. .ctmt with ${macro.vocab("writ", "1.0.0")} passes check
        let good_src = "::macros{ vocab: \"vocabularies/${1}/${1}@${2}.vocabulary.tmt\" }\npath: ${macro.vocab(\"writ\", \"1.0.0\")}\n";
        let good_file = root.join("good.ctmt");
        fs::write(&good_file, good_src).unwrap();
        assert_eq!(ctmt_errors(&good_file, good_src), Vec::<String>::new());
        check(&good_file, false, true, false).expect("valid expression passes");

        // 2. .ctmt with ${macro.nosuch(1)} fails check, message contains line, col, and "unknown macro `macro.nosuch`"
        let bad_macro_src = "path: ${macro.nosuch(1)}\n";
        let bad_macro_file = root.join("bad_macro.ctmt");
        fs::write(&bad_macro_file, bad_macro_src).unwrap();
        let errs = ctmt_errors(&bad_macro_file, bad_macro_src);
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].contains("1:7: unknown macro `macro.nosuch`"),
            "got {}",
            errs[0]
        );
        let err = check(&bad_macro_file, false, true, false).expect_err("unknown macro fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        // 3. .ctmt with ${macro.vocab("writ")} fails check, message contains arity error
        let bad_arity_src = "::macros{ vocab: \"vocabularies/${1}/${1}@${2}.vocabulary.tmt\" }\npath: ${macro.vocab(\"writ\")}\n";
        let bad_arity_file = root.join("bad_arity.ctmt");
        fs::write(&bad_arity_file, bad_arity_src).unwrap();
        let errs = ctmt_errors(&bad_arity_file, bad_arity_src);
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].contains("2:7: `macro.vocab` expects 2 argument(s), got 1"),
            "got {}",
            errs[0]
        );
        let err = check(&bad_arity_file, false, true, false).expect_err("wrong arity fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        // 4. .ctmt with ${tmt("a.tmt")} fails check with clear error (address checking out of scope)
        let addr_src = "path: ${tmt(\"a.tmt\")}\n";
        let addr_file = root.join("addr.ctmt");
        fs::write(&addr_file, addr_src).unwrap();
        let errs = ctmt_errors(&addr_file, addr_src);
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].contains("1:7: a .ctmt has no vault, so it cannot refer to a document, a directory or an element"),
            "got {}",
            errs[0]
        );
        let err = check(&addr_file, false, true, false).expect_err("address expr fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        // 5. .ctmt with bad ::vars entry reports exactly 1 message
        let bad_var_src = "::vars{ bad: 123 }\npath: \"ok\"\n";
        let bad_var_file = root.join("bad_var.ctmt");
        fs::write(&bad_var_file, bad_var_src).unwrap();
        let errs = ctmt_errors(&bad_var_file, bad_var_src);
        assert_eq!(errs.len(), 1, "expected 1 error, got {:?}", errs);
        let err = check(&bad_var_file, false, true, false).expect_err("bad vars fails");
        assert!(err.to_string().contains("1 of 1 file(s) failed"));

        let _ = fs::remove_dir_all(&root);
    }
}
