# tomet-lexer

A minimal byte-position cursor `Cursor<'a>` over `&str` source text for Tomet.

## Overview & Design

- **Context-Sensitive Scanning**:
  Tomet's grammar is context-sensitive (`<`, `@`, `(`, `[`, `{` are structural only after specific triggers). Scanning directly with a lightweight cursor is more efficient and flexible than pre-tokenizing into a context-free token stream.
- **Zero-Copy & Copyable**:
  `Cursor<'a>` holds only `&'a str` and byte offset `pos: usize`, making it `Copy` and zero-copy.

## Features

- **Scanning & Consumption**:
  `bump()`, `eat_char(c)`, `eat_str(s)`, `eat_if(pred)`, `eat_while(pred)`, `eat_whitespace()`, `eat_until_char(c)`
- **Lookahead & Peeking**:
  `peek()`, `peek_at(n)`, `starts_with(s)`
- **Position & Span Mapping**:
  `line_col(pos)`, `position_at(pos)`, `current_position()`, `span_from(start_pos)`
