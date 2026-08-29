//! Resilient, lossless Concrete Syntax Tree (CST) parser for Tomet.
//!
//! Preserves 100% of source characters including trivia (whitespace, newlines, comments).

use tomet_cst::{GreenNodeBuilder, SyntaxKind, SyntaxNode};
use tomet_lexar::tokenize;

/// Parse `src` into a lossless, resilient [`SyntaxNode`].
///
/// Guaranteed Invariant: `parse_cst(src).text().to_string() == src`.
pub fn parse_cst(src: &str) -> SyntaxNode {
    let raw_tokens = tokenize(src);
    let mut parser = CstParser::new(&raw_tokens);
    parser.parse_root();
    SyntaxNode::new_root(parser.builder.finish())
}

struct CstParser<'a> {
    tokens: &'a [(SyntaxKind, &'a str)],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
}

impl<'a> CstParser<'a> {
    fn new(tokens: &'a [(SyntaxKind, &'a str)]) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn current(&self) -> Option<(SyntaxKind, &'a str)> {
        self.tokens.get(self.pos).copied()
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        self.current().map(|(k, _)| k)
    }

    fn bump(&mut self) {
        if let Some((kind, text)) = self.current() {
            self.builder.token(kind.into(), text);
            self.pos += 1;
        }
    }

    fn parse_root(&mut self) {
        self.builder.start_node(SyntaxKind::ROOT.into());

        while !self.is_eof() {
            if let Some(kind) = self.current_kind() {
                if kind.is_trivia() {
                    self.bump();
                    continue;
                }
            }
            self.parse_block();
        }

        self.builder.finish_node();
    }

    fn parse_block(&mut self) {
        if let Some((kind, _)) = self.current() {
            match kind {
                SyntaxKind::HASH => {
                    self.parse_heading();
                }
                SyntaxKind::AT | SyntaxKind::LT | SyntaxKind::DOLLAR => {
                    self.parse_element_block();
                }
                SyntaxKind::MINUS if self.is_list_start() => {
                    self.parse_list();
                }
                SyntaxKind::BACKTICK if self.is_code_block_start() => {
                    self.parse_code_block();
                }
                _ => {
                    self.parse_paragraph();
                }
            }
        }
    }

    fn is_list_start(&self) -> bool {
        if let Some(SyntaxKind::MINUS) = self.current_kind() {
            let next = self.tokens.get(self.pos + 1).map(|(k, _)| *k);
            let next2 = self.tokens.get(self.pos + 2).map(|(k, _)| *k);
            if next == Some(SyntaxKind::WHITESPACE) {
                return true;
            }
            if next == Some(SyntaxKind::DOT) && next2 == Some(SyntaxKind::WHITESPACE) {
                return true;
            }
        }
        false
    }

    fn is_code_block_start(&self) -> bool {
        let count = self.tokens[self.pos..]
            .iter()
            .take_while(|(k, _)| *k == SyntaxKind::BACKTICK)
            .count();
        count >= 3
    }

    fn parse_heading(&mut self) {
        self.builder.start_node(SyntaxKind::HEADING.into());

        while let Some(SyntaxKind::HASH) = self.current_kind() {
            self.bump();
        }

        while let Some(kind) = self.current_kind() {
            if kind == SyntaxKind::WHITESPACE {
                self.bump();
            } else {
                break;
            }
        }

        if let Some(SyntaxKind::L_BRACKET) = self.current_kind() {
            self.parse_content_group();
        }

        self.parse_optional_element_groups();

        // Consume until end of line
        while let Some(kind) = self.current_kind() {
            if kind == SyntaxKind::NEWLINE {
                self.bump();
                break;
            }
            self.bump();
        }

        self.builder.finish_node();
    }

    fn parse_element_block(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_ELEMENT.into());

        self.parse_sigil();
        self.parse_optional_element_groups();

        self.builder.finish_node();
    }

    fn parse_sigil(&mut self) {
        self.builder.start_node(SyntaxKind::SIGIL.into());

        if let Some(kind) = self.current_kind() {
            match kind {
                SyntaxKind::AT | SyntaxKind::HASH | SyntaxKind::DOLLAR => {
                    self.bump();
                    if let Some(SyntaxKind::IDENT) = self.current_kind() {
                        self.bump();
                    }
                }
                SyntaxKind::LT => {
                    self.bump();
                    while let Some(k) = self.current_kind() {
                        if k == SyntaxKind::GT || k == SyntaxKind::NEWLINE {
                            break;
                        }
                        self.bump();
                    }
                    if let Some(SyntaxKind::GT) = self.current_kind() {
                        self.bump();
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }

        self.builder.finish_node();
    }

    fn parse_optional_element_groups(&mut self) {
        loop {
            // Skip non-newline whitespace
            while let Some(SyntaxKind::WHITESPACE) = self.current_kind() {
                self.bump();
            }

            match self.current_kind() {
                Some(SyntaxKind::L_PAREN) => {
                    self.parse_args_group();
                }
                Some(SyntaxKind::L_BRACE) => {
                    self.parse_value_group();
                }
                Some(SyntaxKind::L_BRACKET) => {
                    self.parse_content_group();
                }
                _ => break,
            }
        }
    }

    fn parse_args_group(&mut self) {
        self.builder.start_node(SyntaxKind::ARGS.into());
        self.bump(); // '('

        let mut depth = 1;
        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::L_PAREN {
                depth += 1;
            } else if k == SyntaxKind::R_PAREN {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    break;
                }
            }
            self.bump();
        }

        self.builder.finish_node();
    }

    fn parse_value_group(&mut self) {
        self.builder.start_node(SyntaxKind::VALUE_DATA.into());
        self.bump(); // '{'

        let mut depth = 1;
        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::L_BRACE {
                depth += 1;
            } else if k == SyntaxKind::R_BRACE {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    break;
                }
            }
            self.bump();
        }

        self.builder.finish_node();
    }

    fn parse_content_group(&mut self) {
        self.builder.start_node(SyntaxKind::CONTENT.into());
        self.bump(); // '['

        let mut depth = 1;
        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::L_BRACKET {
                depth += 1;
            } else if k == SyntaxKind::R_BRACKET {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    break;
                }
            }
            self.bump();
        }

        self.builder.finish_node();
    }

    fn parse_list(&mut self) {
        self.builder.start_node(SyntaxKind::LIST.into());

        while self.is_list_start() {
            self.parse_list_item();
        }

        self.builder.finish_node();
    }

    fn parse_list_item(&mut self) {
        self.builder.start_node(SyntaxKind::LIST_ITEM.into());

        self.bump(); // '-'
        if let Some(SyntaxKind::DOT) = self.current_kind() {
            self.bump(); // '.'
        }

        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::NEWLINE {
                self.bump();
                break;
            }
            self.bump();
        }

        self.builder.finish_node();
    }

    fn parse_code_block(&mut self) {
        self.builder.start_node(SyntaxKind::CODE_BLOCK.into());

        let mut count = 0;
        while let Some(SyntaxKind::BACKTICK) = self.current_kind() {
            self.bump();
            count += 1;
        }

        // Consume until matching backticks or EOF
        let mut consecutive_backticks = 0;
        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::BACKTICK {
                consecutive_backticks += 1;
                self.bump();
                if consecutive_backticks == count {
                    break;
                }
            } else {
                consecutive_backticks = 0;
                self.bump();
            }
        }

        self.builder.finish_node();
    }

    fn parse_paragraph(&mut self) {
        self.builder.start_node(SyntaxKind::PARAGRAPH.into());

        while let Some(k) = self.current_kind() {
            if k == SyntaxKind::NEWLINE {
                // Check if next token is also NEWLINE (blank line ends paragraph)
                let next = self.tokens.get(self.pos + 1).map(|(kind, _)| *kind);
                if next == Some(SyntaxKind::NEWLINE) {
                    self.bump();
                    break;
                }
                self.bump();
            } else if (k == SyntaxKind::HASH || k == SyntaxKind::AT || k == SyntaxKind::LT)
                && self.pos > 0
                && self.tokens[self.pos - 1].0 == SyntaxKind::NEWLINE
            {
                // New block started on new line
                break;
            } else {
                self.bump();
            }
        }

        self.builder.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cst_lossless_roundtrip_all_syntaxes() {
        let cases = [
            "#[ Heading ]\n\nParagraph text with *em* and <tag>[ content ].\n",
            "@config(format:json){\n  {\n    \"meta\": \"yaml\"\n  }\n}\n",
            "- item 1\n- item 2\n-. ordered 1\n\n```rust\nfn main() {}\n```\n",
            "// Comment line\n<card>(id: 123){ priority: high }[ Note ]\n",
        ];

        for case in cases {
            let root = parse_cst(case);
            let roundtripped = root.text().to_string();
            assert_eq!(roundtripped, case);
        }
    }

    #[test]
    fn test_cst_node_hierarchy() {
        let src = "#[ Title ]\n\nParagraph\n";
        let root = parse_cst(src);
        assert_eq!(root.kind(), SyntaxKind::ROOT);

        let children: Vec<_> = root.children().map(|n| n.kind()).collect();
        assert!(children.contains(&SyntaxKind::HEADING));
        assert!(children.contains(&SyntaxKind::PARAGRAPH));
    }
}
