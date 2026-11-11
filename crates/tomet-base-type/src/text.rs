//! The text form of a type: the one reader of it, and how a string is written
//! in it.
//!
//! ```text
//! type    = name [ "(" [ argument { "," argument } ] ")" ]
//! argument = [ name ":" ] value
//! value   = type | "auto" | "null" | "true" | "false" | number | string | list
//! list    = "(" [ value { "," value } ] ")"
//! ```
//!
//! A name is a run of characters that are not white space and not one of
//! `( ) , : "`. Arguments written by position come first. A string is written
//! in `"..."` with the escapes `\"`, `\\`, `\n`, `\r` and `\t`. What the
//! arguments mean is not decided here: they are handed to
//! [`Type::build`], which holds them to the rules.

use crate::{Arg, Datum, Type, TypeError};

/// Reads a type from its text form.
pub(crate) fn parse(text: &str) -> Result<Type, TypeError> {
    let mut reader = Reader { text, at: 0 };
    reader.skip_space();
    let name = reader.word("a type name")?;
    let ty = reader.type_named(name)?;
    reader.skip_space();
    if reader.at < text.len() {
        return Err(reader.malformed("the end of the type"));
    }
    Ok(ty)
}

/// `text` in `"..."`, with the escapes the reader takes back.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::new();
    write_quoted(text, &mut out);
    out
}

pub(crate) fn write_quoted(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    text: &'a str,
    /// A byte offset into `text`, always on a character boundary.
    at: usize,
}

impl<'a> Reader<'a> {
    fn malformed(&self, expected: &'static str) -> TypeError {
        TypeError::Malformed {
            text: self.text.to_string(),
            at: self.at,
            expected,
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start().len();
    }

    /// A name or a number or a keyword: the run up to white space or a
    /// delimiter. `expected` is what to say when there is none.
    fn word(&mut self, expected: &'static str) -> Result<&'a str, TypeError> {
        let rest = &self.text[self.at..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ',' | ':' | '"'))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(self.malformed(expected));
        }
        self.at += end;
        Ok(&rest[..end])
    }

    /// The type that `name` begins, with the arguments that follow it in
    /// parentheses, if any.
    fn type_named(&mut self, name: &str) -> Result<Type, TypeError> {
        self.skip_space();
        if !self.eat('(') {
            return Type::build(name, &[], &[]);
        }
        let mut positional = Vec::new();
        let mut named: Vec<(String, Arg)> = Vec::new();
        self.skip_space();
        if !self.eat(')') {
            loop {
                self.skip_space();
                let start = self.at;
                match self.key()? {
                    Some(key) => named.push((key.to_string(), self.value()?)),
                    None if named.is_empty() => positional.push(self.value()?),
                    None => {
                        self.at = start;
                        return Err(self.malformed(
                            "a named argument, `name: value`: one written by position cannot follow a named one",
                        ));
                    }
                }
                self.skip_space();
                if self.eat(',') {
                    continue;
                }
                if self.eat(')') {
                    break;
                }
                return Err(self.malformed("`,` or `)`"));
            }
        }
        let named: Vec<(&str, Arg)> = named.iter().map(|(k, a)| (k.as_str(), a.clone())).collect();
        Type::build(name, &positional, &named)
    }

    /// `name:` at the cursor, and past it; or nothing read when what is there
    /// is not followed by a `:`.
    fn key(&mut self) -> Result<Option<&'a str>, TypeError> {
        let start = self.at;
        if matches!(self.peek(), None | Some('"' | '(' | ')' | ',' | ':')) {
            return Ok(None);
        }
        let key = self.word("an argument")?;
        self.skip_space();
        if self.eat(':') {
            return Ok(Some(key));
        }
        self.at = start;
        Ok(None)
    }

    /// An argument's value: a type, `auto`, or a plain value.
    fn value(&mut self) -> Result<Arg, TypeError> {
        self.skip_space();
        if matches!(self.peek(), Some('"' | '(')) {
            return Ok(Arg::Datum(self.datum()?));
        }
        let start = self.at;
        let word = self.word("a value")?;
        if word == "auto" {
            return Ok(Arg::Auto);
        }
        if let Some(datum) = self.literal(word, start)? {
            return Ok(Arg::Datum(datum));
        }
        Ok(Arg::Type(self.type_named(word)?))
    }

    /// A plain value: text, a number, `true`, `false`, `null` or a list of them.
    fn datum(&mut self) -> Result<Datum, TypeError> {
        self.skip_space();
        match self.peek() {
            Some('"') => Ok(Datum::Text(self.string()?)),
            Some('(') => {
                self.eat('(');
                let mut items = Vec::new();
                self.skip_space();
                if self.eat(')') {
                    return Ok(Datum::Seq(items));
                }
                loop {
                    items.push(self.datum()?);
                    self.skip_space();
                    if self.eat(',') {
                        self.skip_space();
                        if self.eat(')') {
                            return Ok(Datum::Seq(items));
                        }
                        continue;
                    }
                    if self.eat(')') {
                        return Ok(Datum::Seq(items));
                    }
                    return Err(self.malformed("`,` or `)`"));
                }
            }
            _ => {
                let start = self.at;
                let word = self.word("a value")?;
                match self.literal(word, start)? {
                    Some(datum) => Ok(datum),
                    None => {
                        self.at = start;
                        Err(self.malformed(
                            "text in quotes, a number, `true`, `false`, `null` or a list",
                        ))
                    }
                }
            }
        }
    }

    /// The plain value `word` is -- `null`, `true`, `false` or a number -- or
    /// `None` when it is none of them (so a name). `start` is where it began,
    /// for the error of a number that is not one.
    fn literal(&mut self, word: &str, start: usize) -> Result<Option<Datum>, TypeError> {
        match word {
            "null" => return Ok(Some(Datum::Null)),
            "true" => return Ok(Some(Datum::Bool(true))),
            "false" => return Ok(Some(Datum::Bool(false))),
            _ => {}
        }
        let first = word.chars().next();
        let numeric = first.is_some_and(|c| c.is_ascii_digit())
            || (word.len() > 1 && matches!(first, Some('-' | '+')));
        if !numeric {
            return Ok(None);
        }
        if let Ok(n) = word.parse::<i64>() {
            return Ok(Some(Datum::Int(n)));
        }
        match word.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Some(Datum::Float(n))),
            _ => {
                self.at = start;
                Err(self.malformed("a number"))
            }
        }
    }

    /// A string in `"..."`.
    fn string(&mut self) -> Result<String, TypeError> {
        self.eat('"');
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.malformed("a closing `\"`"));
            };
            self.at += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(escaped) = self.peek() else {
                        return Err(
                            self.malformed("an escape: `\\\"`, `\\\\`, `\\n`, `\\r` or `\\t`")
                        );
                    };
                    let unescaped = match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(
                                self.malformed("an escape: `\\\"`, `\\\\`, `\\n`, `\\r` or `\\t`")
                            );
                        }
                    };
                    self.at += escaped.len_utf8();
                    out.push(unescaped);
                }
                _ => out.push(c),
            }
        }
    }
}
