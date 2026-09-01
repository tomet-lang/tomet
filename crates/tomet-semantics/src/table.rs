use tomet_ast::{Inline, Text};

/// One cell in a `@table` row, holding trimmed inline nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub content: Vec<Inline>,
}

/// One row in a `@table`, holding cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

/// Parses table rows and cells from an inline sequence (the `[content]` of a `@table` element).
pub fn parse_table_rows(inlines: &[Inline]) -> Vec<TableRow> {
    let mut rows: Vec<TableRow> = Vec::new();
    let mut current_row: Vec<TableCell> = Vec::new();
    let mut current_cell: Vec<Inline> = Vec::new();
    let mut current_text_buf = String::new();
    let mut in_cell = false;
    let mut bracket_depth = 0usize;

    let flush_text = |cell: &mut Vec<Inline>, buf: &mut String| {
        if !buf.is_empty() {
            cell.push(Inline::Text(Text::from(std::mem::take(buf))));
        }
    };

    let finish_cell = |cell: &mut Vec<Inline>, buf: &mut String, row: &mut Vec<TableCell>| {
        flush_text(cell, buf);
        let mut trimmed_cell = std::mem::take(cell);
        trim_inlines(&mut trimmed_cell);
        row.push(TableCell {
            content: trimmed_cell,
        });
    };

    let finish_row = |row: &mut Vec<TableCell>, rows: &mut Vec<TableRow>| {
        if !row.is_empty() {
            rows.push(TableRow {
                cells: std::mem::take(row),
            });
        }
    };

    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                for c in t.value.chars() {
                    if in_cell {
                        if c == '[' {
                            bracket_depth += 1;
                            current_text_buf.push(c);
                        } else if c == ']' {
                            if bracket_depth > 1 {
                                bracket_depth -= 1;
                                current_text_buf.push(c);
                            } else {
                                finish_cell(
                                    &mut current_cell,
                                    &mut current_text_buf,
                                    &mut current_row,
                                );
                                in_cell = false;
                                bracket_depth = 0;
                            }
                        } else {
                            current_text_buf.push(c);
                        }
                    } else if c == '[' {
                        in_cell = true;
                        bracket_depth = 1;
                        current_cell.clear();
                        current_text_buf.clear();
                    } else if c.is_whitespace() {
                        finish_row(&mut current_row, &mut rows);
                    }
                }
            }
            Inline::Element(el) => {
                if in_cell {
                    flush_text(&mut current_cell, &mut current_text_buf);
                    current_cell.push(Inline::Element(el.clone()));
                }
            }
        }
    }

    if in_cell {
        finish_cell(&mut current_cell, &mut current_text_buf, &mut current_row);
    }
    finish_row(&mut current_row, &mut rows);

    rows
}

fn trim_inlines(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(t)) = inlines.first_mut() {
        t.value = t.value.trim_start().to_string();
    }
    if let Some(Inline::Text(t)) = inlines.first() {
        if t.value.is_empty() {
            inlines.remove(0);
        }
    }
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        t.value = t.value.trim_end().to_string();
    }
    if let Some(Inline::Text(t)) = inlines.last() {
        if t.value.is_empty() {
            inlines.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Sigil};

    #[test]
    fn parses_plain_table_cells() {
        let inlines = vec![Inline::Text(Text::from(
            "\n[ title ][  sdfasdf   ]\n[ r2c1 ][ r2c2 ]\n",
        ))];
        let rows = parse_table_rows(&inlines);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(
            rows[0].cells[0].content,
            vec![Inline::Text(Text::from("title"))]
        );
        assert_eq!(
            rows[0].cells[1].content,
            vec![Inline::Text(Text::from("sdfasdf"))]
        );
        assert_eq!(
            rows[1].cells[0].content,
            vec![Inline::Text(Text::from("r2c1"))]
        );
        assert_eq!(
            rows[1].cells[1].content,
            vec![Inline::Text(Text::from("r2c2"))]
        );
    }

    #[test]
    fn parses_table_cells_with_inline_elements() {
        let el = tomet_tree::element_new(Sigil::Type("em".to_string()));
        let inlines = vec![
            Inline::Text(Text::from("[ ")),
            Inline::Element(el.clone()),
            Inline::Text(Text::from(" ][ plain ]")),
        ];
        let rows = parse_table_rows(&inlines);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(rows[0].cells[0].content, vec![Inline::Element(el)]);
        assert_eq!(
            rows[0].cells[1].content,
            vec![Inline::Text(Text::from("plain"))]
        );
    }
}
