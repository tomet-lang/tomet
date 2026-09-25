//! Attribute, value and escaping helpers shared by the renderers.

use tomet_ast::{Element, Inline, Value};
use tomet_semantics::{EXACT_DATA_KEY, flatten_data};

/// Flattens inline content to plain text -- used for the `alt` attribute,
/// which can't itself carry markup.
pub(crate) fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for (idx, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Raw(t) => s.push_str(&t.value),
            Inline::SoftBreak(_) => {
                let before = s.chars().last();
                let after = inlines.get(idx + 1).and_then(Inline::first_char);
                s.push_str(tomet_ast::softbreak_join(before, after));
            }
            Inline::LineBreak(_) => s.push(' '),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&inlines_to_plain(content));
                }
            }
        }
    }
    s
}

pub(crate) fn split_attrs(
    attrs: Option<&Value>,
) -> (Option<String>, Option<String>, Vec<(String, String)>) {
    let mut id = None;
    let mut class = None;
    let mut data = Vec::new();
    if let Some(map) = attrs.and_then(as_map) {
        for (k, v) in map {
            match k.as_str() {
                "id" => id = Some(value_to_plain(v)),
                "cssclass" => class = Some(value_to_plain(v)),
                _ => data.push((k.clone(), value_to_plain(v))),
            }
        }
    }
    (id, class, data)
}

pub(crate) fn push_named_attrs(
    out: &mut String,
    id: &Option<String>,
    class: &Option<String>,
    data: &[(String, String)],
) {
    if let Some(id) = id {
        out.push_str(&format!(" id=\"{}\"", escape_attr(id)));
    }
    if let Some(class) = class {
        out.push_str(&format!(" class=\"{}\"", escape_attr(class)));
    }
    for (k, v) in data {
        out.push_str(&format!(" data-{}=\"{}\"", escape_attr(k), escape_attr(v)));
    }
}

/// Writes an element's arguments as `data-*` attributes.
///
/// Follows the shared projection rule (`tomet_semantics::flatten`): a
/// scalar or a sequence of scalars gets its own readable attribute, and
/// when that projection would lose something -- a nested map -- the whole
/// group is added as JSON under `data-tomet-data`.
///
/// Before that, a nested map rendered as `data-m=""`: the key survived
/// and its contents did not.
/// A `{...}` group's pairs, as `data-*` beside the ones `(args)` gives.
///
/// `{}` has been "always data" since the uniform-group change, and this
/// is where data goes in HTML -- the same shelf `(args)` already uses,
/// invisible to the reader.
///
/// They used to be dropped in silence. `render_element_value` asked
/// `value_to_plain` for a string to show in a `tm-value` span, and
/// `as_data` returns a `Value::Map` for every group while `value_to_plain`
/// answers `""` for every map, so the span's guard was never true. It
/// appears nowhere in `tests/ref/`.
///
/// A key `(args)` already spent is skipped rather than written twice,
/// which would be invalid HTML. `(args)` wins because it is the group the
/// reader wrote closer to the name.
pub(crate) fn push_value_data_attrs(out: &mut String, el: &Element) {
    let Some(data) = el.value.as_ref().and_then(|v| v.as_data()) else {
        return;
    };
    let taken: Vec<String> = el
        .args
        .as_ref()
        .map(|a| {
            flatten_data(Some(a), None)
                .pairs
                .into_iter()
                .map(|(k, _)| k)
                .collect()
        })
        .unwrap_or_default();
    let skip: Vec<&str> = taken.iter().map(String::as_str).collect();
    push_data_attrs(out, Some(&data), &skip);
}

pub(crate) fn push_data_attrs(out: &mut String, args: Option<&Value>, skip: &[&str]) {
    let Some(args) = args else {
        return;
    };
    let data = flatten_data(Some(args), None);

    match args {
        Value::Map(_) => {
            for (k, v) in &data.pairs {
                if skip.contains(&k.as_str()) {
                    continue;
                }
                out.push_str(&format!(" data-{}=\"{}\"", escape_attr(k), escape_attr(v)));
            }
        }
        // A positional group (`@x(1)`) has no key to project onto, so
        // the shared rule hands it back separately.
        _ => {
            if let Some(text) = data.positional.as_ref().filter(|t| !t.is_empty()) {
                out.push_str(&format!(" data-value=\"{}\"", escape_attr(text)));
            }
        }
    }

    if let Some(exact) = data.exact {
        out.push_str(&format!(
            " data-{}=\"{}\"",
            EXACT_DATA_KEY,
            escape_attr(&exact)
        ));
    }
}

pub(crate) fn value_to_plain(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Seq(items) => items
            .iter()
            .map(value_to_plain)
            .collect::<Vec<_>>()
            .join(", "),
        // A map has no single scalar representation; callers that need
        // per-key access use `as_map`/`map_get` instead.
        Value::Map(_) => String::new(),
        // A call is validation-only (e.g. `:rule`'s `allow:list(...)`)
        // and has no rendered form.
        Value::Call(..) => String::new(),
        // An embedded element has no scalar form either -- same as `Map`.
        Value::Element(_) => String::new(),
    }
}

pub(crate) fn as_map(v: &Value) -> Option<&Vec<(String, Value)>> {
    match v {
        Value::Map(m) => Some(m),
        _ => None,
    }
}

pub(crate) fn map_get<'a>(map: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn escape_attr(s: &str) -> String {
    escape_html(s).replace('"', "&quot;")
}
