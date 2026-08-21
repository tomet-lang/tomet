//! Printer module to serialize `typedmark_ast::Document` back to `.tm` text syntax.

use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, InterpExpr, InterpExprKind, List,
    Literal, Sigil, Value,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldConfig {
    pub field_type: Option<String>,
    pub format: Option<String>,
    pub offset: Option<String>,
    pub always_newline: bool,
    pub length: Option<usize>,
    pub prefix: Option<String>,
    pub force: Option<bool>,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrinterConfig {
    pub meta_always_newline: bool,
    pub heading_space_inside_brackets: bool,
    pub meta_format: Option<String>,
    pub meta_fields: std::collections::BTreeMap<String, FieldConfig>,
    pub wikilink_no_space: bool,
    pub link_no_space: bool,
    pub ignore_files: Vec<String>,
    pub callout_content_style: Option<String>,
    pub list_multiline_style_content: Option<String>,
}

pub fn is_path_ignored(path: &std::path::Path, root: Option<&std::path::Path>, ignore_patterns: &[String]) -> bool {
    if ignore_patterns.is_empty() {
        return false;
    }
    let path_str = path.to_string_lossy().replace('\\', "/");
    let rel_str = if let Some(r) = root {
        path.strip_prefix(r)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path_str.clone())
    } else {
        path_str.clone()
    };
    let rel_clean = rel_str.trim_start_matches('/');

    for pat in ignore_patterns {
        let pat_clean = pat.trim().replace('\\', "/");
        let pat_clean = pat_clean.trim_matches('/');
        if pat_clean.is_empty() {
            continue;
        }
        if rel_clean == pat_clean
            || rel_clean.starts_with(&format!("{pat_clean}/"))
            || path_str.ends_with(&format!("/{pat_clean}"))
            || path_str.ends_with(pat_clean)
            || path_str.contains(&format!("/{pat_clean}/"))
        {
            return true;
        }
    }
    false
}

impl PrinterConfig {
    pub fn from_doc(doc: &Document) -> Self {
        let config = typedmark_semantics::document_config(doc);
        let mut cfg = PrinterConfig::default();

        for (k, v) in &config.entries {
            if k == "format" {
                if let Value::Map(map) = v {
                    Self::apply_format_map(&mut cfg, map);
                }
            } else if let Some(sub_k) = k.strip_prefix("format.") {
                Self::apply_format_key_value(&mut cfg, sub_k, v);
            } else {
                Self::apply_format_key_value(&mut cfg, k, v);
            }
        }

        for block in &doc.blocks {
            if let Block::Element(el) = block {
                if typedmark_semantics::classify(el)
                    == typedmark_semantics::ElementKind::Custom("settings".to_string())
                    || matches!(&el.sigil, Sigil::At(Some(name)) if name == "settings")
                {
                    Self::apply_settings_element(&mut cfg, el);
                }
            }
        }

        cfg
    }

    fn apply_settings_element(cfg: &mut Self, el: &Element) {
        let Some(ElementValue::Data(Value::Map(entries))) = &el.value else {
            return;
        };
        for (k, v) in entries {
            if k == "meta" {
                if let Value::Map(fields) = v {
                    for (field_name, field_val) in fields {
                        if let Value::Map(props) = field_val {
                            let mut field_cfg =
                                cfg.meta_fields.get(field_name).cloned().unwrap_or_default();
                            for (pk, pv) in props {
                                if pk == "type" {
                                    if let Value::String(s) = pv {
                                        field_cfg.field_type = Some(s.clone());
                                    }
                                } else if pk == "format" {
                                    if let Value::String(s) = pv {
                                        field_cfg.format = Some(s.clone());
                                    }
                                } else if pk == "offset" {
                                    if let Value::String(s) = pv {
                                        field_cfg.offset = Some(s.clone());
                                    }
                                } else if pk == "always_newline" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.always_newline = *b;
                                    }
                                } else if pk == "length" {
                                    if let Value::Int(n) = pv {
                                        if *n > 0 {
                                            field_cfg.length = Some(*n as usize);
                                        }
                                    } else if let Value::String(s) = pv {
                                        field_cfg.length = s.parse::<usize>().ok();
                                    }
                                } else if pk == "prefix" {
                                    if let Value::String(s) = pv {
                                        field_cfg.prefix = Some(s.clone());
                                    }
                                } else if pk == "force" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.force = Some(*b);
                                    } else if let Value::String(s) = pv {
                                        field_cfg.force = Some(s == "true" || s == "1");
                                    }
                                } else if pk == "overwrite" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.overwrite = Some(*b);
                                    } else if let Value::String(s) = pv {
                                        field_cfg.overwrite = Some(s == "true" || s == "1");
                                    }
                                }
                            }
                            cfg.meta_fields.insert(field_name.clone(), field_cfg);
                        }
                    }
                }
            }
            if k == "wikilink" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "no_space" {
                            if let Value::Bool(b) = pv {
                                cfg.wikilink_no_space = *b;
                            } else if let Value::String(s) = pv {
                                cfg.wikilink_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            if k == "link" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "no_space" {
                            if let Value::Bool(b) = pv {
                                cfg.link_no_space = *b;
                            } else if let Value::String(s) = pv {
                                cfg.link_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            if k == "callout" {
                if let Value::Map(props) = v {
                    Self::parse_callout_props(props, cfg);
                }
            }
            if k == "list" {
                if let Value::Map(props) = v {
                    Self::parse_list_props(props, cfg);
                }
            }
            if k == "elements" {
                if let Value::Map(elems) = v {
                    for (ek, ev) in elems {
                        if ek == "callout" {
                            if let Value::Map(props) = ev {
                                Self::parse_callout_props(props, cfg);
                            }
                        } else if ek == "list" {
                            if let Value::Map(props) = ev {
                                Self::parse_list_props(props, cfg);
                            }
                        }
                    }
                }
            }
            if k == "ignore" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "files" {
                            if let Value::Seq(items) = pv {
                                for item in items {
                                    if let Value::String(s) = item {
                                        if !cfg.ignore_files.contains(s) {
                                            cfg.ignore_files.push(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_callout_props(props: &[(String, Value)], cfg: &mut Self) {
        for (pk, pv) in props {
            if pk == "style" {
                if let Value::Map(sprops) = pv {
                    for (sk, sv) in sprops {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.callout_content_style = Some(s.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_list_props(props: &[(String, Value)], cfg: &mut Self) {
        for (pk, pv) in props {
            if pk == "multiline" {
                if let Value::Map(mprops) = pv {
                    for (mk, mv) in mprops {
                        if mk == "style" {
                            if let Value::Map(sprops) = mv {
                                for (sk, sv) in sprops {
                                    if sk == "content" {
                                        if let Value::String(s) = sv {
                                            cfg.list_multiline_style_content = Some(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn apply_format_map(cfg: &mut Self, map: &[(String, Value)]) {
        for (k, v) in map {
            Self::apply_format_key_value(cfg, k, v);
        }
    }

    fn apply_format_key_value(cfg: &mut Self, key: &str, value: &Value) {
        match key {
            "meta" => {
                if let Value::Map(map) = value {
                    for (mk, mv) in map {
                        if mk == "always_newline" {
                            if let Value::Bool(b) = mv {
                                cfg.meta_always_newline = *b;
                            }
                        } else if mk == "format" {
                            if let Value::String(s) = mv {
                                cfg.meta_format = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "meta.always_newline" => {
                if let Value::Bool(b) = value {
                    cfg.meta_always_newline = *b;
                }
            }
            "meta.format" => {
                if let Value::String(s) = value {
                    cfg.meta_format = Some(s.clone());
                }
            }
            "heading" => {
                if let Value::Map(map) = value {
                    for (hk, hv) in map {
                        if hk == "space_inside_brackets" {
                            if let Value::Bool(b) = hv {
                                cfg.heading_space_inside_brackets = *b;
                            }
                        }
                    }
                }
            }
            "heading.space_inside_brackets" => {
                if let Value::Bool(b) = value {
                    cfg.heading_space_inside_brackets = *b;
                }
            }
            "wikilink" => {
                if let Value::Map(map) = value {
                    for (wk, wv) in map {
                        if wk == "no_space" {
                            if let Value::Bool(b) = wv {
                                cfg.wikilink_no_space = *b;
                            } else if let Value::String(s) = wv {
                                cfg.wikilink_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            "wikilink.no_space" => {
                if let Value::Bool(b) = value {
                    cfg.wikilink_no_space = *b;
                } else if let Value::String(s) = value {
                    cfg.wikilink_no_space = s == "true" || s == "1";
                }
            }
            "link" => {
                if let Value::Map(map) = value {
                    for (lk, lv) in map {
                        if lk == "no_space" {
                            if let Value::Bool(b) = lv {
                                cfg.link_no_space = *b;
                            } else if let Value::String(s) = lv {
                                cfg.link_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            "link.no_space" => {
                if let Value::Bool(b) = value {
                    cfg.link_no_space = *b;
                } else if let Value::String(s) = value {
                    cfg.link_no_space = s == "true" || s == "1";
                }
            }
            "callout" => {
                if let Value::Map(map) = value {
                    Self::parse_callout_props(map, cfg);
                }
            }
            "callout.style.content" => {
                if let Value::String(s) = value {
                    cfg.callout_content_style = Some(s.clone());
                }
            }
            "callout.style" => {
                if let Value::Map(map) = value {
                    for (sk, sv) in map {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.callout_content_style = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "list" => {
                if let Value::Map(map) = value {
                    Self::parse_list_props(map, cfg);
                }
            }
            "list.multiline.style.content" => {
                if let Value::String(s) = value {
                    cfg.list_multiline_style_content = Some(s.clone());
                }
            }
            "list.multiline.style" => {
                if let Value::Map(map) = value {
                    for (sk, sv) in map {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.list_multiline_style_content = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "list.multiline" => {
                if let Value::Map(map) = value {
                    for (mk, mv) in map {
                        if mk == "style" {
                            if let Value::Map(sprops) = mv {
                                for (sk, sv) in sprops {
                                    if sk == "content" {
                                        if let Value::String(s) = sv {
                                            cfg.list_multiline_style_content = Some(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "ignore" => {
                if let Value::Map(map) = value {
                    for (ik, iv) in map {
                        if ik == "files" {
                            if let Value::Seq(items) = iv {
                                for item in items {
                                    if let Value::String(s) = item {
                                        if !cfg.ignore_files.contains(s) {
                                            cfg.ignore_files.push(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "ignore.files" => {
                if let Value::Seq(items) = value {
                    for item in items {
                        if let Value::String(s) = item {
                            if !cfg.ignore_files.contains(s) {
                                cfg.ignore_files.push(s.clone());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn generate_id_for_field(cfg: &FieldConfig) -> String {
    let prefix = cfg.prefix.as_deref().unwrap_or("");
    match cfg.field_type.as_deref() {
        Some("nanoid") => {
            let len = cfg.length.unwrap_or(8);
            let id = nanoid::nanoid!(len);
            format!("{prefix}{id}")
        }
        Some("uuid") => {
            let id = uuid::Uuid::new_v4().to_string();
            format!("{prefix}{id}")
        }
        _ => {
            let len = cfg.length.unwrap_or(8);
            let id = nanoid::nanoid!(len);
            format!("{prefix}{id}")
        }
    }
}

pub fn is_valid_id_format(existing: &str, cfg: &FieldConfig) -> bool {
    let prefix = cfg.prefix.as_deref().unwrap_or("");
    if !existing.starts_with(prefix) {
        return false;
    }
    let rest = &existing[prefix.len()..];
    let expected_len = cfg.length.unwrap_or(8);
    if rest.len() != expected_len {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn ensure_document_id_with_config(doc: &mut Document, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta_fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    let mut meta_found = false;
    for block in &mut doc.blocks {
        if let Block::Element(el) = block {
            if typedmark_semantics::classify(el) == typedmark_semantics::ElementKind::Meta
                || matches!(&el.sigil, Sigil::At(Some(name)) if name == "meta")
            {
                meta_found = true;
                if let Some(ElementValue::Data(Value::Map(entries))) = &mut el.value {
                    let mut existing_idx = None;
                    for (idx, (k, v)) in entries.iter().enumerate() {
                        if k == "id" {
                            existing_idx = Some((idx, v.clone()));
                            break;
                        }
                    }

                    if let Some((idx, val)) = existing_idx {
                        if overwrite {
                            let existing_str = match &val {
                                Value::String(s) => s.as_str(),
                                _ => "",
                            };
                            if !is_valid_id_format(existing_str, id_cfg) {
                                let new_id = generate_id_for_field(id_cfg);
                                entries[idx].1 = Value::String(new_id);
                            }
                        }
                    } else if force || overwrite {
                        let new_id = generate_id_for_field(id_cfg);
                        entries.insert(0, ("id".to_string(), Value::String(new_id)));
                    }
                }
                break;
            }
        }
    }

    if !meta_found && (force || overwrite) {
        let new_id = generate_id_for_field(id_cfg);
        let mut meta_el = Element::new(Sigil::At(Some("meta".to_string())));
        meta_el.value = Some(ElementValue::Data(Value::Map(vec![(
            "id".to_string(),
            Value::String(new_id),
        )])));
        doc.blocks.insert(0, Block::Element(meta_el));
    }
}

pub fn load_config_from_file(path: &std::path::Path) -> Result<PrinterConfig, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    load_config_from_str(&src)
}

pub fn load_config_from_str(src: &str) -> Result<PrinterConfig, String> {
    let doc = typedmark_parser::parse_document(src).map_err(|e| format!("{e:?}"))?;
    Ok(PrinterConfig::from_doc(&doc))
}

/// Searches `start` and its parent directories for `default.config.tm` or `typedmark.config.tm`.
/// Returns `(PrinterConfig, config_file_path, config_directory_path)`.
pub fn find_config_file(start: &std::path::Path) -> Option<(PrinterConfig, std::path::PathBuf, std::path::PathBuf)> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidates = [
            current.join("default.config.tm"),
            current.join("typedmark.config.tm"),
        ];
        for candidate in candidates {
            if candidate.exists() {
                if let Ok(cfg) = load_config_from_file(&candidate) {
                    return Some((cfg, candidate, current));
                }
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Serialize a [`Document`] AST into TypedMark (`.tm`) source string using document-level config or defaults.
pub fn document_to_tm(doc: &Document) -> String {
    let config = PrinterConfig::from_doc(doc);
    document_to_tm_with_config(doc, &config)
}

pub fn document_to_tm_with_config(doc: &Document, config: &PrinterConfig) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_block(block, config, &mut out);
    }
    typedmark_formatter::format_source(&out)
}

fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        Block::Heading(h) => render_heading(h, config, out),
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content, config);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::List(list) => render_list(list, config, out),
        Block::Element(el) => {
            out.push_str(&render_element(el, config));
            out.push('\n');
        }
    }
}

fn render_heading(h: &Heading, config: &PrinterConfig, out: &mut String) {
    let level = h.level.clamp(1, 6) as usize;
    out.push_str(&"#".repeat(level));
    if config.heading_space_inside_brackets {
        out.push_str("[ ");
        out.push_str(&render_inlines(&h.content, config));
        out.push_str(" ]");
    } else {
        out.push('[');
        out.push_str(&render_inlines(&h.content, config));
        out.push(']');
    }
    if let Some(attrs) = &h.attrs {
        out.push(' ');
        out.push_str(&render_value(attrs));
    }
    out.push('\n');
}

fn render_list(list: &List, config: &PrinterConfig, out: &mut String) {
    render_list_with_indent(list, 0, config, out);
}

fn render_list_with_indent(list: &List, indent: usize, config: &PrinterConfig, out: &mut String) {
    let indent_str = "  ".repeat(indent);
    for item in &list.items {
        let prefix = if list.ordered {
            "-. ".to_string()
        } else {
            "- ".to_string()
        };
        let mut head_prefix = String::new();
        head_prefix.push_str(&indent_str);
        head_prefix.push_str(&prefix);
        if let Some(marker) = &item.marker {
            head_prefix.push_str(&format!("({marker}) "));
        }

        let content_str = render_inlines(&item.content, config);
        let lines: Vec<&str> = content_str.lines().collect();

        if lines.len() > 1 && config.list_multiline_style_content.is_some() {
            let style = config.list_multiline_style_content.as_deref().unwrap();
            let prefix_width = head_prefix.chars().count();
            let pad = " ".repeat(prefix_width + 2);

            out.push_str(&head_prefix);
            if style == "expanded" {
                out.push_str("[\n");
                for line in &lines {
                    out.push_str(&pad);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            } else if style == "block" {
                if lines.len() == 1 {
                    out.push_str("[ ");
                    out.push_str(lines[0]);
                    out.push_str(" ]");
                } else {
                    out.push_str("[ ");
                    for (idx, line) in lines.iter().enumerate() {
                        if idx == 0 {
                            out.push_str(line);
                            out.push('\n');
                        } else {
                            out.push_str(&pad);
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    out.push_str(&pad);
                    out.push(']');
                }
            } else if style == "box" {
                out.push_str("[ ");
                for (idx, line) in lines.iter().enumerate() {
                    if idx == 0 {
                        out.push_str(line);
                        out.push('\n');
                    } else if idx == lines.len() - 1 {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push_str(" ]");
                    } else {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            } else {
                out.push_str(&content_str);
            }
            if let Some(attrs) = &item.attrs {
                out.push(' ');
                out.push_str(&render_value(attrs));
            }
            out.push('\n');
        } else {
            out.push_str(&head_prefix);
            out.push_str(&content_str);
            if let Some(attrs) = &item.attrs {
                out.push(' ');
                out.push_str(&render_value(attrs));
            }
            out.push('\n');
        }
        for child in &item.children {
            if let Block::List(sub) = child {
                render_list_with_indent(sub, indent + 1, config, out);
            }
        }
    }
}

fn render_inlines(inlines: &[Inline], config: &PrinterConfig) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Element(el) => s.push_str(&render_element(el, config)),
        }
    }
    s
}

/// Render an [`Element`] AST node into TypedMark syntax: `<sigil>(args)[content]{value}`
pub fn render_element(el: &Element, config: &PrinterConfig) -> String {
    if let Sigil::Type(name) = &el.sigil {
        if name == "hr" && el.args.is_none() && el.value.is_none() {
            if let Some(content) = &el.content {
                return format!("---[{}]---", render_inlines(content, config));
            } else {
                return "---".to_string();
            }
        }
    }

    if let Sigil::At(Some(name)) = &el.sigil {
        if name == "meta" {
            let effective_format = el
                .args
                .as_ref()
                .and_then(|args| {
                    if let Value::Map(entries) = args {
                        entries.iter().find_map(|(k, v)| {
                            if k == "format" {
                                if let Value::String(fmt) = v {
                                    return Some(fmt.clone());
                                }
                            }
                            None
                        })
                    } else {
                        None
                    }
                })
                .or_else(|| config.meta_format.clone());

            if config.meta_always_newline || effective_format.is_some() {
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                    let mut out = if let Some(ref fmt) = effective_format {
                        format!("@meta(format:{fmt}){{\n")
                    } else {
                        String::from("@meta{\n")
                    };
                    for (k, v) in entries {
                        let field_cfg = config.meta_fields.get(k);
                        let val_str = render_meta_field_value(v, field_cfg, config);
                        out.push_str("  ");
                        out.push_str(k);
                        if val_str.starts_with('\n') || val_str.is_empty() {
                            out.push(':');
                        } else {
                            out.push_str(": ");
                        }
                        out.push_str(&val_str);
                        out.push('\n');
                    }
                    out.push('}');
                    return out;
                }
            }
        }
    }

    if let Sigil::At(None) = &el.sigil {
        if el.content.is_some() && el.args.is_some() && el.value.is_none() {
            let mut out = String::from("@");
            if let Some(content) = &el.content {
                out.push('[');
                out.push_str(&render_inlines(content, config));
                out.push(']');
            }
            if let Some(args) = &el.args {
                out.push('(');
                out.push_str(&render_args_with_config(args, config));
                out.push(')');
            }
            return out;
        }
    }

    if matches!(&el.sigil, Sigil::Type(name) if name == "codeblock") {
        let mut out = String::from("<codeblock>");
        if let Some(args) = &el.args {
            out.push('(');
            out.push_str(&render_args_with_config(args, config));
            out.push(')');
        }
        if let Some(content) = &el.content {
            let text = render_inlines(content, config);
            out.push_str("[\n");
            for line in text.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            out.push(']');
        }
        if let Some(value) = &el.value {
            out.push('{');
            match value {
                ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
                ElementValue::Children(children) => {
                    for (i, child) in children.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        out.push_str(&render_element(child, config));
                    }
                }
                ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
            }
            out.push('}');
        }
        return out;
    }

    if matches!(&el.sigil, Sigil::Type(name) if name == "callout") {
        if let Some(style) = config.callout_content_style.as_deref() {
            if style == "expanded" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    out.push_str("[\n");
                    for line in inlines_text.lines() {
                        out.push_str("  ");
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push(']');
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            } else if style == "block" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    let lines: Vec<&str> = inlines_text.lines().collect();
                    out.push('\n');
                    if lines.is_empty() {
                        out.push_str("[ ]");
                    } else if lines.len() == 1 {
                        out.push_str("[ ");
                        out.push_str(lines[0]);
                        out.push_str(" ]");
                    } else {
                        for (idx, line) in lines.iter().enumerate() {
                            if idx == 0 {
                                out.push_str("[ ");
                                out.push_str(line);
                                out.push('\n');
                            } else {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push('\n');
                            }
                        }
                        out.push(']');
                    }
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            } else if style == "box" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    let lines: Vec<&str> = inlines_text.lines().collect();
                    out.push('\n');
                    if lines.is_empty() {
                        out.push_str("[ ]");
                    } else if lines.len() == 1 {
                        out.push_str("[ ");
                        out.push_str(lines[0]);
                        out.push_str(" ]");
                    } else {
                        for (idx, line) in lines.iter().enumerate() {
                            if idx == 0 {
                                out.push_str("[ ");
                                out.push_str(line);
                                out.push('\n');
                            } else if idx == lines.len() - 1 {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push_str(" ]");
                            } else {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push('\n');
                            }
                        }
                    }
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            }
        }
    }

    let mut out = String::new();

    match &el.sigil {
        Sigil::Type(name) => {
            out.push('<');
            out.push_str(name);
            out.push('>');
        }
        Sigil::At(Some(name)) => {
            out.push('@');
            out.push_str(name);
        }
        Sigil::At(None) => {
            out.push('@');
        }
        Sigil::Bare => {}
        Sigil::Dollar => {
            out.push('$');
        }
    }

    if let Some(args) = &el.args {
        out.push('(');
        out.push_str(&render_args_with_config(args, config));
        out.push(')');
    }

    if let Some(content) = &el.content {
        out.push('[');
        out.push_str(&render_inlines(content, config));
        out.push(']');
    }

    if let Some(value) = &el.value {
        out.push('{');
        match value {
            ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
            ElementValue::Children(children) => {
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&render_element(child, config));
                }
            }
            ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
        }
        out.push('}');
    }

    out
}

/// Re-renders an `InterpExpr` back to source text for round-tripping
/// (the interior of `${...}`, no surrounding braces -- the caller already
/// adds those as part of `Sigil::Dollar`'s generic `{value}` rendering).
/// `String` is always quoted, unlike `render_value_inner`'s conditional
/// quoting: unlike a `Value::String`, an `InterpExprKind::Literal(Literal::String(_))`
/// only ever comes from an explicitly `"..."`-quoted source token
/// (`document.rs::parse_interp_primary`), never a bare identifier, so it
/// always needs the quotes back.
pub fn render_interp_expr(expr: &InterpExpr) -> String {
    match &expr.kind {
        InterpExprKind::Identifier(name) => name.clone(),
        InterpExprKind::Literal(Literal::Int(i)) => i.to_string(),
        InterpExprKind::Literal(Literal::Float(f)) => f.to_string(),
        InterpExprKind::Literal(Literal::String(s)) => format!("\"{s}\""),
        InterpExprKind::Call { callee, args } => {
            let rendered: Vec<_> = args.iter().map(render_interp_expr).collect();
            format!("{}({})", render_interp_expr(callee), rendered.join(", "))
        }
        InterpExprKind::Member { object, member } => {
            format!("{}.{member}", render_interp_expr(object))
        }
    }
}

fn is_iso8601(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-' {
        if bytes.len() == 10 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit());
        }
        if (bytes[10] == b'T' || bytes[10] == b' ') && bytes.len() >= 19 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit())
                && bytes[13] == b':'
                && bytes[16] == b':';
        }
    }
    false
}

fn format_rfc3339(s: &str, offset: Option<&str>) -> String {
    let trimmed = s.trim();
    if is_iso8601(trimmed) {
        if !trimmed.ends_with('Z')
            && !trimmed.contains('+')
            && trimmed.len() >= 19
            && !trimmed[10..].contains('-')
        {
            let off = offset.unwrap_or("Z");
            format!("{trimmed}{off}")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    }
}

fn render_meta_field_value(
    v: &Value,
    field_cfg: Option<&FieldConfig>,
    config: &PrinterConfig,
) -> String {
    if let Value::Null = v {
        return String::new();
    }
    if let Some(cfg) = field_cfg {
        if cfg.format.as_deref() == Some("rfc3339") || cfg.field_type.as_deref() == Some("datetime")
        {
            if let Value::String(s) = v {
                return format_rfc3339(s, cfg.offset.as_deref());
            }
        }
        if cfg.always_newline {
            if let Value::Seq(items) = v {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let mut s = String::new();
                for item in items {
                    s.push_str("\n    - ");
                    s.push_str(&render_value_inner_with_config(item, config));
                }
                return s;
            }
        }
    }
    match v {
        Value::String(s) if is_iso8601(s) => s.clone(),
        _ => render_value_inner_with_config(v, config),
    }
}

fn render_args_with_config(v: &Value, config: &PrinterConfig) -> String {
    if let Value::Map(entries) = v {
        if entries.len() == 1 {
            let key = &entries[0].0;
            if key == "wiki" {
                let space = if config.wikilink_no_space { "" } else { " " };
                return format!("wiki:{space}{}", render_value_inner_with_config(&entries[0].1, config));
            } else if key == "url" || key == "link" {
                let space = if config.link_no_space { "" } else { " " };
                return format!("{key}:{space}{}", render_value_inner_with_config(&entries[0].1, config));
            }
        }
    }
    render_value_inner_with_config(v, config)
}

pub fn render_value(v: &Value) -> String {
    format!("{{{}}}", render_value_inner(v))
}

pub fn render_value_inner(v: &Value) -> String {
    render_value_inner_with_config(v, &PrinterConfig::default())
}

pub fn render_value_inner_with_config(v: &Value, config: &PrinterConfig) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => {
            if is_iso8601(s) {
                s.clone()
            } else if s.contains(' ') || s.contains(':') || s.contains(',') || s.contains('[') || s.contains(']') || s.is_empty() {
                format!("\"{s}\"")
            } else {
                s.clone()
            }
        }
        Value::Seq(items) => {
            let rendered: Vec<_> = items
                .iter()
                .map(|item| render_value_inner_with_config(item, config))
                .collect();
            format!("[{}]", rendered.join(", "))
        }
        Value::Map(entries) => {
            if entries.len() == 1 {
                let key = &entries[0].0;
                if key == "wiki" {
                    let space = if config.wikilink_no_space { "" } else { " " };
                    format!("@(wiki:{space}{})", render_value_inner_with_config(&entries[0].1, config))
                } else if key == "url" || key == "link" {
                    let space = if config.link_no_space { "" } else { " " };
                    format!("@({key}:{space}{})", render_value_inner_with_config(&entries[0].1, config))
                } else {
                    let mut parts = Vec::new();
                    for (idx, (k, val)) in entries.iter().enumerate() {
                        let val_str = render_value_inner_with_config(val, config);
                        if idx == 0 && (k == "variant" || k == "lang" || k == "src" || k == "format") {
                            parts.push(val_str);
                        } else {
                            parts.push(format!("{k}: {val_str}"));
                        }
                    }
                    parts.join(", ")
                }
            } else {
                let mut parts = Vec::new();
                for (idx, (k, val)) in entries.iter().enumerate() {
                    let val_str = render_value_inner_with_config(val, config);
                    if idx == 0 && (k == "variant" || k == "lang" || k == "src" || k == "format") {
                        parts.push(val_str);
                    } else {
                        parts.push(format!("{k}: {val_str}"));
                    }
                }
                parts.join(", ")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::{Heading, Inline, Span, Text};

    #[test]
    fn test_printer_config_space_inside_brackets() {
        let doc = Document {
            blocks: vec![Block::Heading(Heading::new(
                1,
                vec![Inline::Text(Text::new("Title", Span::dummy()))],
                None,
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        let cfg = PrinterConfig {
            heading_space_inside_brackets: true,
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("#[ Title ]"));
    }

    #[test]
    fn test_printer_config_meta_format() {
        let config_src = r#"@config(format:json){
          {
            "format": {
              "meta": {
                "format": "yaml"
              }
            }
          }
        }"#;
        let cfg = load_config_from_str(config_src).unwrap();
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
    }

    #[test]
    fn test_markdown_meta_export_with_meta_format() {
        let md = "---\ntitle: Hello\n---\n\n# World\n";
        let doc = typedmark_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("@meta(format:yaml){"));
    }

    #[test]
    fn test_iso8601_timestamp_rendering_in_meta() {
        let md = "---\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("modified: 2026-06-17T05:52:44"));
        assert!(!printed.contains("modified: \"2026-06-17T05:52:44\""));
    }

    #[test]
    fn test_wikilink_rendering() {
        let md = "Check [[name]] and [[name|display]] here.\n";
        let doc = typedmark_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@(wiki: name)") || printed.contains("@(wiki:name)"));
        assert!(
            printed.contains("@[display](wiki: name)") || printed.contains("@[display](wiki:name)")
        );
    }

    #[test]
    fn test_settings_field_config_parsing() {
        let settings_src = r#"@settings(format:json){
          {
            "meta": {
              "aliases": {
                "type": "list",
                "always_newline": true
              },
              "created": {
                "type": "datetime",
                "format": "rfc3339"
              },
              "modified": {
                "type": "datetime",
                "format": "rfc3339"
              }
            }
          }
        }"#;
        let cfg = load_config_from_str(settings_src).unwrap();
        assert_eq!(cfg.meta_fields.get("aliases").unwrap().always_newline, true);
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta_fields.get("modified").unwrap().format.as_deref(),
            Some("rfc3339")
        );
    }

    #[test]
    fn test_rfc3339_and_aliases_always_newline_meta_field_formatting() {
        let md = "---\naliases:\n  - rust\n  - typedmark\ncreated: 2026-06-17T05:52:44\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "aliases".to_string(),
            FieldConfig {
                field_type: Some("list".to_string()),
                always_newline: true,
                ..Default::default()
            },
        );
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );
        meta_fields.insert(
            "modified".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("modified: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("aliases:\n    - rust\n    - typedmark"));

        let md_empty = "---\naliases: []\nflags: []\n---\n\n# Document\n";
        let doc_empty = typedmark_markdown::from_markdown(md_empty);
        let printed_empty = document_to_tm_with_config(&doc_empty, &cfg);
        assert!(printed_empty.contains("aliases: []"));
        assert!(printed_empty.contains("flags: []"));
    }

    #[test]
    fn test_load_config_from_file_default_config_tm() {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = loop {
            let candidate = dir.join("docs/tests/test.config.tm");
            if candidate.exists() {
                break candidate;
            }
            if !dir.pop() {
                panic!("could not find docs/tests/test.config.tm in parent directories");
            }
        };
        let cfg = load_config_from_file(&path).expect("failed to load docs/tests/test.config.tm");
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
        assert_eq!(cfg.meta_always_newline, true);
        assert_eq!(cfg.meta_fields.get("aliases").unwrap().always_newline, true);
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().offset.as_deref(),
            Some("+09:00")
        );
        assert_eq!(cfg.wikilink_no_space, true);
        assert_eq!(cfg.callout_content_style.as_deref(), Some("block"));
        assert_eq!(cfg.list_multiline_style_content.as_deref(), Some("box"));
        assert_eq!(
            cfg.ignore_files,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );

        let default_config_path = path.parent().unwrap().parent().unwrap().join("default.config.tm");
        let cfg_default = load_config_from_file(&default_config_path).expect("failed to load docs/default.config.tm");
        assert_eq!(cfg_default.callout_content_style.as_deref(), Some("block"));
        assert_eq!(cfg_default.list_multiline_style_content.as_deref(), Some("box"));
    }

    #[test]
    fn test_rfc3339_with_offset_formatting() {
        let md = "---\ncreated: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                offset: Some("+09:00".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44+09:00"));
    }

    #[test]
    fn test_null_value_rendering() {
        let rendered = render_value_inner(&Value::Null);
        assert_eq!(rendered, "");
        let md = "---\ntitle:\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("title:\n"));
        assert!(!printed.contains("title: null"));
        assert!(!printed.contains("title: \"\""));
    }

    #[test]
    fn test_wikilink_no_space_setting() {
        let md = "Check [[target]] and [[target|display]].\n";
        let doc = typedmark_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@(wiki: target)"));
        assert!(printed_default.contains("@[display](wiki: target)"));

        let cfg_no_space = PrinterConfig {
            wikilink_no_space: true,
            ..Default::default()
        };
        let printed_no_space = document_to_tm_with_config(&doc, &cfg_no_space);
        assert!(printed_no_space.contains("@(wiki:target)"));
        assert!(printed_no_space.contains("@[display](wiki:target)"));
    }

    #[test]
    fn test_ignore_files_parsing_and_path_matching() {
        let settings_src = r#"@settings(format:json){
  {
    "ignore": {
      "files": [
        "00-09 System/01 Apps/obsidian"
      ]
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        assert_eq!(
            cfg.ignore_files,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );

        let root = std::path::Path::new("/workspace");
        let ignored_file = root.join("00-09 System/01 Apps/obsidian/note.md");
        let normal_file = root.join("00-09 System/01 Apps/other/note.md");

        assert!(is_path_ignored(&ignored_file, Some(root), &cfg.ignore_files));
        assert!(!is_path_ignored(&normal_file, Some(root), &cfg.ignore_files));
    }

    #[test]
    fn test_nanoid_generation_and_ensure_document_id() {
        let settings_src = r#"@settings(format:json){
  {
    "meta": {
      "id": {
        "type": "nanoid",
        "length": 8,
        "prefix": "doc-"
      }
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").expect("id config present");
        assert_eq!(id_cfg.field_type.as_deref(), Some("nanoid"));
        assert_eq!(id_cfg.length, Some(8));
        assert_eq!(id_cfg.prefix.as_deref(), Some("doc-"));

        let gen_id = generate_id_for_field(id_cfg);
        assert!(gen_id.starts_with("doc-"));
        assert_eq!(gen_id.len(), 4 + 8); // "doc-" + 8 chars

        let mut doc = typedmark_markdown::from_markdown("# Test Note\nHello world\n");
        ensure_document_id_with_config(&mut doc, &cfg);

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("id: doc-"));
    }

    #[test]
    fn test_nanoid_force_and_overwrite_behavior() {
        let settings_src = r#"@settings(format:json){
  {
    "meta": {
      "id": {
        "type": "nanoid",
        "length": 8,
        "prefix": "doc-",
        "force": true,
        "overwrite": true
      }
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").unwrap();
        assert_eq!(id_cfg.force, Some(true));
        assert_eq!(id_cfg.overwrite, Some(true));

        // Case 1: Valid existing ID is kept
        let md1 = "---\nid: doc-12345678\n---\n# Note 1\n";
        let mut doc1 = typedmark_markdown::from_markdown(md1);
        ensure_document_id_with_config(&mut doc1, &cfg);
        let printed1 = document_to_tm_with_config(&doc1, &cfg);
        assert!(printed1.contains("id: doc-12345678"));

        // Case 2: Invalid format existing ID is overwritten when overwrite: true
        let md2 = "---\nid: invalid-slug\n---\n# Note 2\n";
        let mut doc2 = typedmark_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc2, &cfg);
        let printed2 = document_to_tm_with_config(&doc2, &cfg);
        assert!(!printed2.contains("invalid-slug"));
        assert!(printed2.contains("id: doc-"));

        // Case 3: When force: true and overwrite: false, invalid existing ID is preserved
        let cfg_no_overwrite = PrinterConfig {
            meta_fields: std::collections::BTreeMap::from([(
                "id".to_string(),
                FieldConfig {
                    field_type: Some("nanoid".to_string()),
                    length: Some(8),
                    prefix: Some("doc-".to_string()),
                    force: Some(true),
                    overwrite: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut doc3 = typedmark_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc3, &cfg_no_overwrite);
        let printed3 = document_to_tm_with_config(&doc3, &cfg_no_overwrite);
        assert!(printed3.contains("id: invalid-slug"));
    }

    #[test]
    fn test_obsidian_callout_blockquote_printing() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n";
        let doc = typedmark_markdown::from_markdown(md);
        let printed = document_to_tm_with_config(&doc, &PrinterConfig::default());
        assert!(printed.contains("<callout>(info, title: \"2025/04/29 11:09\")["));
        assert!(printed.contains("コレさすがに草www"));

        let md_plain = "> Plain quote text\n";
        let doc_plain = typedmark_markdown::from_markdown(md_plain);
        let printed_plain = document_to_tm_with_config(&doc_plain, &PrinterConfig::default());
        assert!(printed_plain.contains("<blockquote>["));
        assert!(printed_plain.contains("Plain quote text"));
        assert!(!printed_plain.contains("<blockquote>("));
    }

    #[test]
    fn test_callout_content_style_formatting() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n> お前なら[[2025-04-26|どうするんだ]]？\n";
        let doc = typedmark_markdown::from_markdown(md);

        // Test "block" style
        let cfg_block = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("<callout>(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@[どうするんだ](wiki: 2025-04-26)？\n]"));

        // Test "box" style
        let cfg_box = PrinterConfig {
            callout_content_style: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("<callout>(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@[どうするんだ](wiki: 2025-04-26)？ ]"));

        // Test "expanded" style
        let cfg_expanded = PrinterConfig {
            callout_content_style: Some("expanded".to_string()),
            ..Default::default()
        };
        let printed_expanded = document_to_tm_with_config(&doc, &cfg_expanded);
        assert!(printed_expanded.contains("<callout>(info, title: \"2025/04/29 11:09\")[\n  コレさすがに草www\n  お前なら@[どうするんだ](wiki: 2025-04-26)？\n]"));
    }

    #[test]
    fn test_list_multiline_style_formatting() {
        let md = "1. いや、まずこういう話をするときの前提として、\n   Vtuberでくくってるやつがまず、ゴミだ。\n   確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n";
        let doc = typedmark_markdown::from_markdown(md);

        let cfg_box = PrinterConfig {
            list_multiline_style_content: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。 ]"));

        let cfg_block = PrinterConfig {
            list_multiline_style_content: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n     ]"));
    }

    #[test]
    fn test_single_line_block_style_formatting() {
        let md_callout = "> [!info] Single Line\n> 一行テキスト\n";
        let doc_callout = typedmark_markdown::from_markdown(md_callout);
        let cfg = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc_callout, &cfg);
        assert!(printed.contains("<callout>(info, title: \"Single Line\")\n[ 一行テキスト ]"));
    }

    #[test]
    fn test_link_no_space_setting() {
        let md = "[Google](https://google.com)\n";
        let doc = typedmark_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@[Google](url: \"https://google.com\")"));

        let cfg_nospace = PrinterConfig {
            link_no_space: true,
            ..Default::default()
        };
        let printed_nospace = document_to_tm_with_config(&doc, &cfg_nospace);
        assert!(printed_nospace.contains("@[Google](url:\"https://google.com\")"));
    }

    #[test]
    fn test_codeblock_formatting() {
        let md = "```shell\nirm \"https://christitus.com/win\" | iex\n```\n";
        let doc = typedmark_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert_eq!(
            printed.trim(),
            "<codeblock>(shell)[\n  irm \"https://christitus.com/win\" | iex\n]"
        );
    }
}
