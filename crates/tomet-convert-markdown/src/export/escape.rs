//! Text and attribute escaping utilities for CommonMark export.

/// Escapes CommonMark's special characters in a run of plain text.
///
/// A backtick-delimited span is the exception: it is passed through
/// verbatim, backticks included. Tomet does not turn `` `x` `` into an
/// element -- the parser keeps the backticks as literal text and only
/// shields the run from further markup (`inline.rs`'s backtick probe) --
/// so by the time it reaches here it looks like ordinary text. Escaping
/// it would turn the author's inline code into a literal ``\`x\``, which
/// is what every `` `spec/` `` in the docs used to export as.
///
/// The rule matches the parser's: an opening backtick pairs with the next
/// backtick on the same line. An unpaired one is escaped as before.
pub(super) fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            if let Some(span) = take_code_span(&mut chars) {
                out.push('`');
                out.push_str(&span);
                continue;
            }
            out.push('\\');
        } else if matches!(c, '\\' | '*' | '_' | '[' | ']') {
            out.push('\\');
        } else if c == '<' {
            let mut look = chars.clone();
            let mut tag_name = String::new();
            while let Some(&ch) = look.peek() {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    tag_name.push(ch);
                    look.next();
                } else {
                    break;
                }
            }
            if is_common_html_tag(&tag_name) {
                out.push('\\');
            }
        }
        out.push(c);
    }
    out
}

/// Consumes the rest of a backtick span, closing backtick included.
///
/// `chars` must sit just past the opening backtick. Returns `None` (and
/// leaves `chars` untouched) when no closing backtick follows on the same
/// line, which is the parser's condition for the span not being one.
fn take_code_span(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut look = chars.clone();
    let mut span = String::new();
    loop {
        match look.next()? {
            '`' => {
                span.push('`');
                *chars = look;
                return Some(span);
            }
            '\n' | '\r' => return None,
            ch => span.push(ch),
        }
    }
}

fn is_common_html_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "a" | "abbr"
            | "address"
            | "article"
            | "aside"
            | "audio"
            | "b"
            | "base"
            | "bdi"
            | "bdo"
            | "quote"
            | "body"
            | "br"
            | "button"
            | "canvas"
            | "caption"
            | "cite"
            | "code"
            | "col"
            | "colgroup"
            | "data"
            | "datalist"
            | "dd"
            | "del"
            | "details"
            | "dfn"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "em"
            | "embed"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hgroup"
            | "hr"
            | "html"
            | "i"
            | "iframe"
            | "img"
            | "input"
            | "ins"
            | "kbd"
            | "label"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "map"
            | "mark"
            | "menu"
            | "meta"
            | "meter"
            | "nav"
            | "noscript"
            | "object"
            | "ol"
            | "optgroup"
            | "option"
            | "output"
            | "p"
            | "param"
            | "picture"
            | "pre"
            | "progress"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "s"
            | "samp"
            | "script"
            | "section"
            | "select"
            | "small"
            | "source"
            | "span"
            | "strong"
            | "style"
            | "sub"
            | "summary"
            | "sup"
            | "svg"
            | "table"
            | "tbody"
            | "td"
            | "template"
            | "textarea"
            | "tfoot"
            | "th"
            | "thead"
            | "time"
            | "title"
            | "tr"
            | "track"
            | "u"
            | "ul"
            | "var"
            | "video"
            | "wbr"
    )
}

pub(super) fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Fenced code blocks need a fence at least one backtick longer than the
/// longest run of backticks already inside the code, or the fence would
/// terminate early on re-parse.
pub(super) fn fence_for(code: &str) -> String {
    let longest_run = code
        .split(|c: char| c != '`')
        .map(|run| run.len())
        .max()
        .unwrap_or(0);
    "`".repeat((longest_run + 1).max(3))
}
