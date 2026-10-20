//! CLI command reference generator from `clap::Command` definitions.

use clap::{Arg, Command};
use tomet_ast::{Block, Document, Element, ElementValue, Placement, Section, Sigil, Value};
use tomet_config::PrinterConfig;
use tomet_tree::{ElementExt, element_new};

use crate::util::{code, code_block, dummy, inlines, list, paragraph, paragraphs, text};

/// Renders a complete formatted CLI specification `.tmt` string from a `clap::Command`.
pub fn render(root: &mut Command, doc_id: &str, notice: &str, config: &PrinterConfig) -> String {
    root.build();
    let doc = document(root, doc_id, notice);
    let printed = tomet_printer::document_to_tm_with_config(&doc, config);
    tomet_formatter::format_source_with_config(&printed, config)
}

/// Builds a Tomet [`Document`] AST representing the CLI command hierarchy and options.
pub fn document(root: &mut Command, doc_id: &str, notice: &str) -> Document {
    let mut meta = element_new(Sigil::named("meta")).with_placement(Placement::Block);
    meta.value = Some(ElementValue::from_map(Value::Map(vec![(
        "id".to_string(),
        Value::String(doc_id.to_string()),
    )])));

    let mut blocks = vec![Block::Element(meta)];
    push_command(root, 1, &mut blocks);
    if !notice.is_empty() {
        let insert_idx = blocks.len().min(2);
        blocks.insert(insert_idx, paragraph(inlines(notice)));
    }

    Document::new(blocks, dummy())
}

/// Appends sections, usage, arguments, options, and subcommands for `cmd` into `out`.
pub fn push_command(cmd: &mut Command, level: usize, out: &mut Vec<Block>) {
    let name = cmd
        .get_bin_name()
        .map_or_else(|| cmd.get_name().to_string(), str::to_string);
    out.push(Block::Section(Section::new(
        level,
        vec![text(&name)],
        dummy(),
    )));

    let about = cmd
        .get_long_about()
        .or_else(|| cmd.get_about())
        .map(|s| s.to_string())
        .unwrap_or_default();
    for para in paragraphs(&about) {
        out.push(paragraph(inlines(&para)));
    }

    let usage = cmd.render_usage().to_string();
    let usage = usage.trim().strip_prefix("Usage:").unwrap_or(&usage).trim();
    out.push(Block::Element(code_block(usage)));

    let positional: Vec<&Arg> = cmd
        .get_arguments()
        .filter(|a| listed(a) && a.is_positional())
        .collect();
    let options: Vec<&Arg> = cmd
        .get_arguments()
        .filter(|a| listed(a) && !a.is_positional())
        .collect();
    if !positional.is_empty() {
        out.push(paragraph(vec![text("Arguments:")]));
        out.push(Block::List(list(positional.iter().map(|a| item(cmd, a)))));
    }
    if !options.is_empty() {
        out.push(paragraph(vec![text("Options:")]));
        out.push(Block::List(list(options.iter().map(|a| item(cmd, a)))));
    }

    let subs: Vec<String> = cmd
        .get_subcommands()
        .filter(|s| s.get_name() != "help" && !s.is_hide_set())
        .map(|s| s.get_name().to_string())
        .collect();
    for sub in subs {
        if let Some(sub) = cmd.find_subcommand_mut(&sub) {
            push_command(sub, level + 1, out);
        }
    }
}

fn listed(arg: &Arg) -> bool {
    !arg.is_hide_set() && !matches!(arg.get_id().as_str(), "help" | "version")
}

fn item(cmd: &Command, arg: &Arg) -> Element {
    let mut content = vec![code(&label(arg))];

    let help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
    let mentions_default = help.to_lowercase().contains("default");
    if !help.is_empty() {
        let end = if help.ends_with(['.', '!', '?']) {
            ""
        } else {
            "."
        };
        content.extend(inlines(&format!(": {help}{end}")));
    }

    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    if arg.get_action().takes_values() && !defaults.is_empty() && !mentions_default {
        content.push(text(" Default: "));
        content.push(code(&defaults.join(", ")));
        content.push(text("."));
    }

    let mut conflicts: Vec<String> = cmd
        .get_arg_conflicts_with(arg)
        .into_iter()
        .filter(|c| listed(c))
        .map(label_short)
        .collect();
    conflicts.sort();
    if !conflicts.is_empty() {
        content.push(text(" Conflicts with "));
        for (i, c) in conflicts.iter().enumerate() {
            if i > 0 {
                content.push(text(", "));
            }
            content.push(code(c));
        }
        content.push(text("."));
    }

    let mut el = element_new(Sigil::Bare).with_placement(Placement::Block);
    el.content = Some(vec![paragraph(content)]);
    el
}

fn label(arg: &Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|names| names.first().map(|n| n.to_string()))
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
    if arg.is_positional() {
        let many = arg.get_num_args().is_some_and(|r| r.max_values() > 1);
        let dots = if many { "..." } else { "" };
        return if arg.is_required_set() {
            format!("<{value}>{dots}")
        } else {
            format!("[{value}]{dots}")
        };
    }
    let mut names = Vec::new();
    if let Some(short) = arg.get_short() {
        names.push(format!("-{short}"));
    }
    if let Some(long) = arg.get_long() {
        names.push(format!("--{long}"));
    }
    let mut out = names.join(", ");
    if arg.get_action().takes_values() {
        out.push_str(&format!(" <{value}>"));
    }
    out
}

fn label_short(arg: &Arg) -> String {
    match (arg.get_long(), arg.get_short()) {
        (Some(long), _) => format!("--{long}"),
        (None, Some(short)) => format!("-{short}"),
        _ => label(arg),
    }
}
