//! Table layout formatting according to `table.adjust_width`, `table.max_col_width`, and `table.align`.

use tomet_config::PrinterConfig;

/// Formats tables in `src` according to `table.adjust_width`, `table.max_col_width`, and `table.align`.
pub fn format_tables_with_config(src: &str, config: &PrinterConfig) -> String {
    let mode = config
        .format
        .table_adjust_width
        .as_deref()
        .unwrap_or("auto");
    if mode == "false" || mode == "off" {
        return src.to_string();
    }
    let max_col_width_limit = config.format.table_max_col_width.unwrap_or(20);

    let lines: Vec<&str> = src.lines().collect();
    let mut out_lines = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        let is_table_lead = trimmed.starts_with("@table");
        let mut is_bracket_table = false;
        let mut is_pipe_table = false;

        if is_table_lead {
            if trimmed.contains('[') {
                is_bracket_table = true;
            } else if trimmed.ends_with('|') {
                is_pipe_table = true;
            } else {
                let mut next_idx = i + 1;
                while next_idx < lines.len() && lines[next_idx].trim().is_empty() {
                    next_idx += 1;
                }
                if next_idx < lines.len() {
                    let next_trimmed = lines[next_idx].trim();
                    if next_trimmed.starts_with('|') && extract_row_cells(lines[next_idx]).is_some()
                    {
                        is_pipe_table = true;
                    } else if next_trimmed.starts_with('[') {
                        is_bracket_table = true;
                    }
                }
            }
        }

        if is_bracket_table || is_pipe_table {
            out_lines.push(line.to_string());
            i += 1;

            let mut table_rows: Vec<(String, Vec<String>)> = Vec::new();
            let mut raw_table_lines = Vec::new();

            while i < lines.len() {
                let t_line = lines[i];
                let t_trimmed = t_line.trim();

                if is_bracket_table {
                    if t_trimmed == "]"
                        || t_trimmed.starts_with("]{")
                        || t_trimmed.starts_with("] ")
                    {
                        break;
                    }
                } else if is_pipe_table && (t_trimmed.is_empty() || !t_trimmed.starts_with('|')) {
                    break;
                }

                if let Some((prefix, cells)) = extract_row_cells(t_line) {
                    table_rows.push((prefix, cells));
                    raw_table_lines.push(None);
                } else {
                    raw_table_lines.push(Some(t_line.to_string()));
                }
                i += 1;
            }

            let aligns = extract_table_alignments(line, config.format.table_align.as_deref());

            if !table_rows.is_empty() {
                let mut col_count = 0;
                for (_, cells) in &table_rows {
                    col_count = col_count.max(cells.len());
                }

                let mut max_lens = vec![0usize; col_count];
                for (_, cells) in &table_rows {
                    for (c_idx, cell) in cells.iter().enumerate() {
                        let len = text_display_width(cell.trim());
                        max_lens[c_idx] = max_lens[c_idx].max(len);
                    }
                }

                let mut target_widths: Vec<Option<usize>> = vec![None; col_count];
                let mut auto_align_active = true;

                for (c_idx, &max_len) in max_lens.iter().enumerate() {
                    match mode {
                        "true" | "all" => {
                            target_widths[c_idx] = Some(max_len.max(1));
                        }
                        "auto" => {
                            if auto_align_active && max_len <= max_col_width_limit {
                                target_widths[c_idx] = Some(max_len.max(1));
                            } else {
                                auto_align_active = false;
                                target_widths[c_idx] = None;
                            }
                        }
                        _ => {
                            target_widths[c_idx] = None;
                        }
                    }
                }

                let mut row_idx = 0;
                for raw in raw_table_lines {
                    if let Some(other_line) = raw {
                        out_lines.push(other_line);
                    } else {
                        let (prefix, cells) = &table_rows[row_idx];
                        row_idx += 1;
                        let mut formatted_cells = Vec::new();
                        for (c_idx, &target) in target_widths.iter().enumerate() {
                            let cell_text = cells.get(c_idx).map(|s| s.trim()).unwrap_or("");
                            let cell_len = text_display_width(cell_text);
                            let col_align = aligns.get(c_idx).map(|s| s.as_str()).unwrap_or("left");

                            let (left_spaces, right_spaces) = if let Some(target_w) = target {
                                let target_width = target_w + 2;
                                let extra = if target_width > cell_len {
                                    target_width - cell_len
                                } else {
                                    2
                                };
                                match col_align {
                                    "right" => {
                                        let left = extra.saturating_sub(1);
                                        let right = 1;
                                        (left, right)
                                    }
                                    "center" => {
                                        let left = extra / 2;
                                        let right = extra - left;
                                        (left, right)
                                    }
                                    _ => {
                                        let left = 1;
                                        let right = extra.saturating_sub(1);
                                        (left, right)
                                    }
                                }
                            } else {
                                (1, 1)
                            };

                            let left_str = " ".repeat(left_spaces);
                            let right_str = " ".repeat(right_spaces);
                            formatted_cells.push(format!("[{left_str}{cell_text}{right_str}]"));
                        }
                        out_lines.push(format!("{prefix}{}", formatted_cells.join("")));
                    }
                }
            }

            if is_bracket_table && i < lines.len() {
                out_lines.push(lines[i].to_string());
                i += 1;
            }
        } else {
            out_lines.push(line.to_string());
            i += 1;
        }
    }

    let mut res = out_lines.join("\n");
    if src.ends_with('\n') && !res.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn extract_table_alignments(header_line: &str, default_align: Option<&str>) -> Vec<String> {
    let def = default_align.unwrap_or("left");
    if let Some(args_start) = header_line.find('(')
        && let Some(args_end) = header_line[args_start..].find(')')
    {
        let args = &header_line[args_start + 1..args_start + args_end];
        if let Some(pos) = args.find("align:") {
            let val = args[pos + 6..].trim();
            if val.starts_with('[') {
                if let Some(end_bracket) = val.find(']') {
                    let inner = &val[1..end_bracket];
                    let mut aligns = Vec::new();
                    for item in inner.split(',') {
                        let a = item.trim().trim_matches('"').trim_matches('\'');
                        if !a.is_empty() {
                            aligns.push(a.to_string());
                        }
                    }
                    if !aligns.is_empty() {
                        return aligns;
                    }
                }
            } else {
                let a = val
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !a.is_empty() {
                    return vec![a.to_string(); 50];
                }
            }
        }
    }
    vec![def.to_string(); 50]
}

fn text_display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn extract_row_cells(line: &str) -> Option<(String, Vec<String>)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let rest = &line[indent_len..];

    let (prefix, cells_part) = if let Some(after_pipe) = rest.strip_prefix('|') {
        let spaces_len = after_pipe.len() - after_pipe.trim_start().len();
        let space_str = if spaces_len > 0 {
            &after_pipe[..spaces_len]
        } else {
            ""
        };
        (format!("{indent}|{space_str}"), after_pipe.trim_start())
    } else {
        (indent.to_string(), rest)
    };

    let trimmed = cells_part.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    let mut cells = Vec::new();
    let mut depth = 0;
    let mut cell_start = 0;
    let chars: Vec<(usize, char)> = cells_part.char_indices().collect();

    for &(byte_idx, c) in &chars {
        if c == '[' {
            if depth == 0 {
                cell_start = byte_idx + c.len_utf8();
            }
            depth += 1;
        } else if c == ']' {
            depth -= 1;
            if depth == 0 {
                let cell_content = &cells_part[cell_start..byte_idx];
                cells.push(cell_content.to_string());
            }
        }
    }

    if depth == 0 && !cells.is_empty() {
        Some((prefix, cells))
    } else {
        None
    }
}
