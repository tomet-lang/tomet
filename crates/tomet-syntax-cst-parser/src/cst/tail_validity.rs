//! Whether a chain of groups read at the end of a line is valid, as the AST would read it.

use tomet_cst::SyntaxKind as K;
use tomet_cst::SyntaxNode;

fn validate_args_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN)
        || node.text().to_string().trim_end().ends_with(')');
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    if entry_children.is_empty() {
        return closed;
    }
    if !closed {
        return false;
    }

    let mut keys = Vec::new();
    for (i, child) in entry_children.iter().enumerate() {
        if child.kind() == K::ERROR {
            return false;
        }
        if child.kind() == K::MAP_ENTRY {
            let has_colon = child.children_with_tokens().any(|e| e.kind() == K::COLON);
            if has_colon {
                let key_str: String = child
                    .children_with_tokens()
                    .take_while(|e| e.kind() != K::COLON)
                    .filter(|e| !e.kind().is_trivia())
                    .map(|e| e.to_string())
                    .collect();
                if keys.contains(&key_str) {
                    return false;
                }
                keys.push(key_str);
            }
        }
        if i + 1 < entry_children.len() {
            let next_child = &entry_children[i + 1];
            let mut has_sep = false;
            let mut cur = child.last_token().and_then(|t| t.next_token());
            let next_start = next_child
                .first_token()
                .map(|t| usize::from(t.text_range().start()))
                .unwrap_or(0);
            while let Some(t) = cur {
                if usize::from(t.text_range().start()) >= next_start {
                    break;
                }
                if t.kind() == K::COMMA || t.kind() == K::NEWLINE {
                    has_sep = true;
                    break;
                }
                cur = t.next_token();
            }
            if !has_sep {
                return false;
            }
        }
    }
    true
}

fn validate_value_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_BRACE);
    if !closed {
        return false;
    }
    let mut keys = Vec::new();
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    for (i, child) in entry_children.iter().enumerate() {
        match child.kind() {
            K::ERROR => return false,
            K::MAP_ENTRY => {
                let has_colon = child.children_with_tokens().any(|e| e.kind() == K::COLON);
                if !has_colon {
                    return false;
                }
                let key_str: String = child
                    .children_with_tokens()
                    .take_while(|e| e.kind() != K::COLON)
                    .filter(|e| !e.kind().is_trivia())
                    .map(|e| e.to_string())
                    .collect();
                if keys.contains(&key_str) {
                    return false;
                }
                keys.push(key_str);
            }
            K::SEQ_ITEM => {
                let first_non_trivia = child.children_with_tokens().find(|e| !e.kind().is_trivia());
                match first_non_trivia {
                    Some(tomet_cst::SyntaxElement::Node(n))
                        if matches!(n.kind(), K::ARGS | K::INLINE_FORM | K::BLOCK_FORM) => {}
                    _ => return false,
                }
            }
            _ => return false,
        }
        if i + 1 < entry_children.len() {
            let next_child = &entry_children[i + 1];
            let mut has_sep = false;
            let mut cur = child.last_token().and_then(|t| t.next_token());
            let next_start = next_child
                .first_token()
                .map(|t| usize::from(t.text_range().start()))
                .unwrap_or(0);
            while let Some(t) = cur {
                if usize::from(t.text_range().start()) >= next_start {
                    break;
                }
                if t.kind() == K::COMMA || t.kind() == K::NEWLINE {
                    has_sep = true;
                    break;
                }
                cur = t.next_token();
            }
            if !has_sep {
                return false;
            }
        }
    }
    true
}

fn validate_id_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN);
    if !closed {
        return false;
    }
    let mut tokens_inside = Vec::new();
    let mut seen_lparen = false;
    for elem in node.children_with_tokens() {
        if elem.kind() == K::L_PAREN {
            seen_lparen = true;
            continue;
        }
        if elem.kind() == K::R_PAREN {
            break;
        }
        if seen_lparen {
            match elem {
                tomet_cst::SyntaxElement::Token(t) => {
                    if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                        tokens_inside.push(t);
                    }
                }
                tomet_cst::SyntaxElement::Node(n) => {
                    if n.kind() == K::CONTENT {
                        return false;
                    }
                    for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
                        if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                            tokens_inside.push(t);
                        }
                    }
                }
            }
        }
    }
    if tokens_inside.len() != 1 {
        return false;
    }
    let tok = &tokens_inside[0];
    tok.kind() == K::NAME
        || tok.kind() == K::IDENT
        || tok
            .text()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn validate_connect(node: &SyntaxNode) -> bool {
    if node
        .children()
        .any(|n| n.kind() == K::ID_GROUP || n.kind() == K::ERROR)
    {
        return false;
    }
    let has_name = node.children().any(|n| n.kind() == K::NAME)
        || node.children().any(|n| n.kind() == K::SIGIL);
    if !has_name {
        let mut group_count = 0;
        for child in node.children() {
            match child.kind() {
                K::ARGS => {
                    if !validate_args_group(&child) {
                        return false;
                    }
                    group_count += 1;
                }
                K::VALUE_DATA | K::MAP_LITERAL => {
                    if !validate_value_group(&child) {
                        return false;
                    }
                    group_count += 1;
                }
                _ => return false,
            }
        }
        group_count > 0
    } else {
        // Only what is structurally a connect: whether it stands on a primary group is
        // lowering's to refuse, so that a bracket-less body and a full-form head agree.
        for child in node.children() {
            match child.kind() {
                K::NAME | K::SIGIL => {}
                K::ARGS => {
                    if !validate_args_group(&child) {
                        return false;
                    }
                }
                K::VALUE_DATA | K::MAP_LITERAL => {
                    if !validate_value_group(&child) {
                        return false;
                    }
                }
                K::CONTENT => {}
                K::CONNECT => {
                    if !validate_connect(&child) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

pub(super) fn is_valid_tail_group_list(groups: &[SyntaxNode]) -> bool {
    if groups.is_empty() {
        return false;
    }
    let mut seen_args = false;
    let mut seen_value = false;
    let mut seen_id = false;

    for child in groups {
        match child.kind() {
            K::SIGIL | K::SECTION_HEADING | K::NAME | K::BODY => {}
            K::LIST
            | K::INLINE_FORM
            | K::SLOT
            | K::ESCAPE
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::CODE_SPAN
            | K::AUTOLINK => {}
            K::CONTENT => return false,
            K::ERROR => return false,
            K::ARGS => {
                if seen_args || !validate_args_group(child) {
                    return false;
                }
                seen_args = true;
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                if seen_value || !validate_value_group(child) {
                    return false;
                }
                seen_value = true;
            }
            K::ID_GROUP => {
                if seen_id || !validate_id_group(child) {
                    return false;
                }
                seen_id = true;
            }
            K::CONNECT => {
                if !validate_connect(child) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn tail_claims_something(groups: &[SyntaxNode]) -> bool {
    groups.iter().any(|g| {
        matches!(
            g.kind(),
            K::ARGS | K::VALUE_DATA | K::MAP_LITERAL | K::ID_GROUP | K::CONNECT
        )
    })
}
