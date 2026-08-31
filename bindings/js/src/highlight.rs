use serde::{Deserialize, Serialize};
use tomet_lexer::{tokenize, SyntaxKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HighlightSpan {
    pub from: usize,
    pub to: usize,
    pub tag: String,
}

pub fn compute_highlight_spans(src: &str) -> Vec<HighlightSpan> {
    let mut spans = Vec::new();
    let tokens = tokenize(src);
    let mut offset = 0;
    
    // We compute byte offsets for every token
    let token_offsets: Vec<(SyntaxKind, &str, usize, usize)> = tokens
        .into_iter()
        .map(|(k, text)| {
            let start = offset;
            offset += text.len();
            (k, text, start, offset)
        })
        .collect();

    let mut i = 0;
    let mut in_paren_depth = 0;
    let mut in_brace_depth = 0;
    let mut is_line_start = true;

    while i < token_offsets.len() {
        let (kind, text, start, end) = token_offsets[i];

        match kind {
            SyntaxKind::NEWLINE => {
                is_line_start = true;
                i += 1;
                continue;
            }
            SyntaxKind::WHITESPACE => {
                i += 1;
                continue;
            }
            SyntaxKind::COMMENT => {
                let tag = if text.starts_with("/*") {
                    "blockComment"
                } else {
                    "lineComment"
                };
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: tag.to_string(),
                });
                i += 1;
                continue;
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "string".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER => {
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "number".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW | SyntaxKind::NULL_KW => {
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "atom".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::HASH if is_line_start => {
                // Heading line start `#[` or `###[`
                let heading_start = start;
                let mut heading_end = end;
                while i + 1 < token_offsets.len() && token_offsets[i + 1].0 == SyntaxKind::HASH {
                    i += 1;
                    heading_end = token_offsets[i].3;
                }
                spans.push(HighlightSpan {
                    from: heading_start,
                    to: heading_end,
                    tag: "heading".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::MINUS if is_line_start => {
                // Check for list marker or thematic break
                let mut j = i + 1;
                let mut minus_count = 1;
                while j < token_offsets.len() && token_offsets[j].0 == SyntaxKind::MINUS {
                    minus_count += 1;
                    j += 1;
                }
                if minus_count >= 3 {
                    // Thematic break
                    let break_end = token_offsets[j - 1].3;
                    spans.push(HighlightSpan {
                        from: start,
                        to: break_end,
                        tag: "contentSeparator".to_string(),
                    });
                    i = j;
                    is_line_start = false;
                    continue;
                }

                // Check list marker with possible task checkbox `( )`, `(x)`, `(T)`, etc.
                let mut list_end = end;
                if i + 1 < token_offsets.len() && token_offsets[i + 1].0 == SyntaxKind::DOT {
                    i += 1;
                    list_end = token_offsets[i].3;
                }
                
                // Peek ahead for task checkbox: whitespace then `(` then status then `)`
                let mut check_idx = i + 1;
                if check_idx < token_offsets.len() && token_offsets[check_idx].0 == SyntaxKind::WHITESPACE {
                    check_idx += 1;
                }
                if check_idx < token_offsets.len() && token_offsets[check_idx].0 == SyntaxKind::L_PAREN {
                    let mut close_idx = check_idx + 1;
                    while close_idx < token_offsets.len()
                        && token_offsets[close_idx].0 != SyntaxKind::R_PAREN
                        && token_offsets[close_idx].0 != SyntaxKind::NEWLINE
                    {
                        close_idx += 1;
                    }
                    if close_idx < token_offsets.len() && token_offsets[close_idx].0 == SyntaxKind::R_PAREN {
                        list_end = token_offsets[close_idx].3;
                        i = close_idx;
                    }
                }

                spans.push(HighlightSpan {
                    from: start,
                    to: list_end,
                    tag: "list".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::AT => {
                // `@name` element or `@`
                let mut tag_end = end;
                if i + 1 < token_offsets.len() && token_offsets[i + 1].0 == SyntaxKind::IDENT {
                    i += 1;
                    tag_end = token_offsets[i].3;
                }
                spans.push(HighlightSpan {
                    from: start,
                    to: tag_end,
                    tag: "tagName".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::LT => {
                // `<Type>` element
                let mut gt_idx = i + 1;
                while gt_idx < token_offsets.len()
                    && token_offsets[gt_idx].0 != SyntaxKind::GT
                    && token_offsets[gt_idx].0 != SyntaxKind::NEWLINE
                {
                    gt_idx += 1;
                }
                if gt_idx < token_offsets.len() && token_offsets[gt_idx].0 == SyntaxKind::GT {
                    let tag_end = token_offsets[gt_idx].3;
                    spans.push(HighlightSpan {
                        from: start,
                        to: tag_end,
                        tag: "tagName".to_string(),
                    });
                    i = gt_idx + 1;
                    is_line_start = false;
                    continue;
                }
            }
            SyntaxKind::DOLLAR => {
                // `$name` or `${...}` macro/interpolation
                if i + 1 < token_offsets.len() && token_offsets[i + 1].0 == SyntaxKind::L_BRACE {
                    spans.push(HighlightSpan {
                        from: start,
                        to: token_offsets[i + 1].3,
                        tag: "brace.special".to_string(),
                    });
                    i += 2;
                    in_brace_depth += 1;
                    is_line_start = false;
                    continue;
                } else if i + 1 < token_offsets.len() && token_offsets[i + 1].0 == SyntaxKind::IDENT {
                    i += 1;
                    spans.push(HighlightSpan {
                        from: start,
                        to: token_offsets[i].3,
                        tag: "variableName.function".to_string(),
                    });
                    i += 1;
                    is_line_start = false;
                    continue;
                }
            }
            SyntaxKind::BACKTICK => {
                // Inline code or code fence ```
                let mut count = 1;
                let mut j = i + 1;
                while j < token_offsets.len() && token_offsets[j].0 == SyntaxKind::BACKTICK {
                    count += 1;
                    j += 1;
                }
                if count >= 3 {
                    // Code fence line
                    let mut fence_end = token_offsets[j - 1].3;
                    while j < token_offsets.len() && token_offsets[j].0 != SyntaxKind::NEWLINE {
                        fence_end = token_offsets[j].3;
                        j += 1;
                    }
                    spans.push(HighlightSpan {
                        from: start,
                        to: fence_end,
                        tag: "processingInstruction".to_string(),
                    });
                    i = j;
                    is_line_start = false;
                    continue;
                } else {
                    // Inline code span `...`
                    let mut close_idx = j;
                    while close_idx < token_offsets.len()
                        && token_offsets[close_idx].0 != SyntaxKind::BACKTICK
                        && token_offsets[close_idx].0 != SyntaxKind::NEWLINE
                    {
                        close_idx += 1;
                    }
                    if close_idx < token_offsets.len() && token_offsets[close_idx].0 == SyntaxKind::BACKTICK {
                        let span_end = token_offsets[close_idx].3;
                        spans.push(HighlightSpan {
                            from: start,
                            to: span_end,
                            tag: "monospace".to_string(),
                        });
                        i = close_idx + 1;
                        is_line_start = false;
                        continue;
                    }
                }
            }
            SyntaxKind::IDENT => {
                // Check if this IDENT is a property key (followed by COLON inside value group)
                if in_paren_depth > 0 || in_brace_depth > 0 {
                    let mut next_idx = i + 1;
                    while next_idx < token_offsets.len() && token_offsets[next_idx].0 == SyntaxKind::WHITESPACE {
                        next_idx += 1;
                    }
                    if next_idx < token_offsets.len() && token_offsets[next_idx].0 == SyntaxKind::COLON {
                        spans.push(HighlightSpan {
                            from: start,
                            to: end,
                            tag: "propertyName".to_string(),
                        });
                        i += 1;
                        is_line_start = false;
                        continue;
                    }
                }
            }
            SyntaxKind::L_PAREN => {
                in_paren_depth += 1;
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "punctuation".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::R_PAREN => {
                if in_paren_depth > 0 {
                    in_paren_depth -= 1;
                }
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "punctuation".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::L_BRACE => {
                in_brace_depth += 1;
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "punctuation".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::R_BRACE => {
                if in_brace_depth > 0 {
                    in_brace_depth -= 1;
                }
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "punctuation".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            SyntaxKind::L_BRACKET | SyntaxKind::R_BRACKET | SyntaxKind::COLON | SyntaxKind::COMMA => {
                spans.push(HighlightSpan {
                    from: start,
                    to: end,
                    tag: "punctuation".to_string(),
                });
                i += 1;
                is_line_start = false;
                continue;
            }
            _ => {}
        }

        i += 1;
        is_line_start = false;
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_highlight_spans() {
        let src = "#[ Heading ]\n- Official: @link(id: \"tasks\")[Tasks]\n";
        let spans = compute_highlight_spans(src);
        
        let tags: Vec<(&str, &str)> = spans
            .iter()
            .map(|s| (&src[s.from..s.to], s.tag.as_str()))
            .collect();

        assert!(tags.iter().any(|(t, tag)| *t == "#" && *tag == "heading"));
        assert!(tags.iter().any(|(t, tag)| *t == "-" && *tag == "list"));
        assert!(tags.iter().any(|(t, tag)| *t == "@link" && *tag == "tagName"));
        assert!(tags.iter().any(|(t, tag)| *t == "id" && *tag == "propertyName"));
        assert!(tags.iter().any(|(t, tag)| *t == "\"tasks\"" && *tag == "string"));
    }
}
