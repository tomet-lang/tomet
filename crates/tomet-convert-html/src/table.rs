//! `@table`.

use crate::RenderCtx;
use crate::block::render_inlines;
use crate::util::{as_map, map_get, push_named_attrs, split_attrs};
use tomet_ast::{Element, Value};
use tomet_semantics::normalized_element_args;

pub(crate) fn render_table_element(cx: &RenderCtx, el: &Element, out: &mut String) {
    let inlines = match &el.content {
        Some(content) => content,
        None => {
            out.push_str("<table class=\"tm-element tm-table\"></table>\n");
            return;
        }
    };
    let rows = tomet_semantics::parse_table_rows(inlines);
    if rows.is_empty() {
        out.push_str("<table class=\"tm-element tm-table\"></table>\n");
        return;
    }

    let args = normalized_element_args(el);
    let has_header = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "header"))
        .map(|v| match v {
            Value::Bool(b) => *b,
            _ => true,
        })
        .unwrap_or(true);

    let attrs = match &el.value {
        Some(v) => v.as_data(),
        _ => None,
    };
    let (id, class, data) = split_attrs(attrs.as_ref());
    let class = class.or_else(|| Some("tm-element tm-table".to_string()));

    out.push_str("<table");
    push_named_attrs(out, &id, &class, &data);
    out.push_str(">\n");

    let (header_row, body_rows) = if has_header && !rows.is_empty() {
        (Some(&rows[0]), &rows[1..])
    } else {
        (None, &rows[..])
    };

    if let Some(hrow) = header_row {
        out.push_str("<thead>\n<tr>\n");
        for cell in &hrow.cells {
            out.push_str("<th>");
            render_inlines(cx, &cell.content, out);
            out.push_str("</th>\n");
        }
        out.push_str("</tr>\n</thead>\n");
    }

    if !body_rows.is_empty() {
        out.push_str("<tbody>\n");
        for row in body_rows {
            out.push_str("<tr>\n");
            for cell in &row.cells {
                out.push_str("<td>");
                render_inlines(cx, &cell.content, out);
                out.push_str("</td>\n");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</tbody>\n");
    }

    out.push_str("</table>\n");
}
