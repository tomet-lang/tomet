#!/usr/bin/env python3
"""One-shot migration to the shape-axis sigils and the `+++` fence.

THROWAWAY. Run once over the repo's own `.tmt` files, commit the result,
then delete this script. Nothing is published, so no upgrade path is owed
to anyone outside this repository -- that is why the parser has no legacy
mode and this is not a supported `tomet migrate` command.

Rewrites:

  <T>(args)[content]      ->  #T(args)[content]   at a line start
                              @T(args)[content]   mid-line
  @meta / @config / ...   ->  #meta / #config     (block-shaped builtins)
  (content:raw)[body]     ->  +++\\nbody\\n+++
  (format:x){body}        ->  (format:x)+++\\nbody\\n+++
  <id:taskA>              ->  #id(taskA)

The `[...]` and `{...}` bodies are found by real bracket matching rather
than a regex: `#memo(content:raw)[don't forget [this]]` nests, and a
regex would stop at the first `]`. The two matchers mirror the parser's
own former `find_matching_bracket` (quote-agnostic, for free-form prose
where an apostrophe is not a quote) and `find_matching_delimiter`
(quote-aware, for embedded JSON/YAML/TOML source).

Code fences are skipped: a ``` block in the docs shows Tomet syntax as an
example, and its contents are migrated too -- but only because those
examples must also be updated. Inline ``...`` spans are left alone.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Built-in names whose shape is block. Anything else keeps the shape its
# position implies. Mirrors `required_shape` in
# `crates/tomet-semantics/src/kind.rs`.
BLOCK_BUILTINS = {
    "meta", "config", "settings", "import", "references", "id", "blueprint",
    "links", "hr", "codeblock", "blockquote", "table", "heading", "ol", "ul",
    "kind", "version",
}
INLINE_BUILTINS = {"em", "strong", "mark", "link", "embed", "icon"}

NAME_RE = r"[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*"


def find_matching(text: str, start: int, open_c: str, close_c: str,
                  quote_aware: bool) -> int | None:
    """Index of the `close_c` matching the `open_c` at `start`, or None."""
    assert text[start] == open_c
    depth = 0
    i = start
    n = len(text)
    while i < n:
        c = text[i]
        if quote_aware and c in "\"'":
            quote = c
            i += 1
            while i < n:
                if text[i] == "\\" and quote == '"':
                    i += 2
                    continue
                if text[i] == quote:
                    i += 1
                    break
                i += 1
            continue
        if c == open_c:
            depth += 1
        elif c == close_c:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def at_line_start(text: str, idx: int) -> bool:
    """Whether only whitespace precedes `idx` on its line."""
    line_start = text.rfind("\n", 0, idx) + 1
    return text[line_start:idx].strip() == ""


def migrate_type_sigils(text: str) -> str:
    """`<T>` becomes `#T` in block position, `@T` inline."""
    out = []
    i = 0
    # `<id:taskA>` first: the only `<T>` whose name is not an identifier.
    text = re.sub(r"<id:([A-Za-z0-9_,\- ]+)>", r"#id(\1)", text)
    pattern = re.compile(rf"<({NAME_RE})>")
    while True:
        m = pattern.search(text, i)
        if not m:
            out.append(text[i:])
            break
        name = m.group(1)
        if name in INLINE_BUILTINS:
            sigil = "@"
        elif name in BLOCK_BUILTINS:
            sigil = "#"
        else:
            sigil = "#" if at_line_start(text, m.start()) else "@"
        out.append(text[i:m.start()])
        out.append(f"{sigil}{name}")
        i = m.end()
    return "".join(out)


def migrate_at_directives(text: str) -> str:
    """Block-shaped builtins move from `@` to `#`."""
    names = "|".join(sorted(BLOCK_BUILTINS))
    return re.sub(rf"(?<![A-Za-z0-9_.@]) @({names})\b".replace(" ", ""),
                  r"#\1", text)


def migrate_raw_content(text: str) -> str:
    """`(content:raw)[body]` becomes a `+++` fence."""
    pattern = re.compile(r"\(\s*content\s*:\s*raw\s*\)\s*")
    while True:
        m = pattern.search(text)
        if not m:
            return text
        bracket = text.find("[", m.end() - 1)
        if bracket == -1 or text[m.end():bracket].strip() != "":
            # Not the shape we handle; leave it and move past.
            text = text[:m.start()] + "(content_raw_UNMIGRATED)" + text[m.end():]
            continue
        close = find_matching(text, bracket, "[", "]", quote_aware=False)
        if close is None:
            text = text[:m.start()] + "(content_raw_UNMIGRATED)" + text[m.end():]
            continue
        body = text[bracket + 1:close].strip("\n")
        text = text[:m.start()] + fence(body) + text[close + 1:]


def migrate_format_bodies(text: str) -> str:
    """`(format:x){body}` and `(x){body}` on meta/config become fences."""
    pattern = re.compile(r"\(\s*(?:format\s*:\s*)?(?:json|yaml|toml)\s*\)\s*\{")
    pos = 0
    while True:
        m = pattern.search(text, pos)
        if not m:
            return text
        brace = m.end() - 1
        close = find_matching(text, brace, "{", "}", quote_aware=True)
        if close is None:
            # An unbalanced fragment -- a test asserting on a substring
            # such as `"#meta(format:yaml){"`. Nothing to convert.
            pos = m.end()
            continue
        body = text[brace + 1:close].strip("\n")
        # Re-indent to column 0: the body is now fence content, and its old
        # indentation was only there to sit inside `{ }`.
        body = dedent_block(body)
        head = text[m.start():brace]
        text = text[:m.start()] + head + fence(body) + text[close + 1:]
        pos = m.start() + len(head)


def dedent_block(body: str) -> str:
    lines = [l for l in body.split("\n")]
    indents = [len(l) - len(l.lstrip()) for l in lines if l.strip()]
    if not indents:
        return body
    cut = min(indents)
    return "\n".join(l[cut:] if l.strip() else "" for l in lines)


def fence(body: str) -> str:
    """Wraps `body` in a `+++` run long enough to contain it."""
    runs = [len(l.strip()) for l in body.split("\n")
            if l.strip() and set(l.strip()) == {"+"}]
    n = max([*runs, 2]) + 1
    marker = "+" * n
    return f"{marker}\n{body}\n{marker}"


def migrate(text: str) -> str:
    text = migrate_raw_content(text)
    text = migrate_type_sigils(text)
    text = migrate_at_directives(text)
    text = migrate_format_bodies(text)
    return text


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    changed = 0
    for arg in argv[1:]:
        path = Path(arg)
        original = path.read_text(encoding="utf-8")
        migrated = migrate(original)
        if migrated != original:
            path.write_text(migrated, encoding="utf-8")
            changed += 1
            print(f"migrated {path}")
    print(f"{changed} file(s) changed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
