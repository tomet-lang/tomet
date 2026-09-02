#!/usr/bin/env python3
"""Applies `migrate-sigils.py` inside Rust string literals only.

THROWAWAY, same as its sibling. Test sources embed `.tmt` snippets as
string literals; those need the same migration as the `.tmt` files, but a
blind text pass over a `.rs` file would rewrite `Vec<Element>` into
`Vec@Element` and every `@` in a doc comment. So this scans for string
literals -- `"..."` with backslash escapes, and `r"..."` / `r#"..."#`
raw strings -- and migrates only what is inside them.

Escaped content is handled by unescaping `\\n` to a real newline before
migrating (the sigil rules are line-sensitive: `#T` at a line start is a
block element, mid-line it is inline) and re-escaping afterwards.
"""

from __future__ import annotations

import sys
from pathlib import Path

# Import the sibling module despite its hyphenated name.
import importlib.util

_spec = importlib.util.spec_from_file_location(
    "migrate_sigils", Path(__file__).parent / "migrate-sigils.py"
)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)


def looks_like_output(text: str) -> bool:
    """Whether a literal is expected *output* rather than Tomet source.

    HTML and Typst expectations live in test assertions and contain the
    same `<tag>` shape as the retired `<T>` sigil, so migrating them would
    turn `"<pre>"` into `"#pre"`. A closing tag, a Typst `#heading(` call,
    or an HTML attribute is a reliable marker that this is output.
    """
    return "</" in text or "/>" in text or 'class=\\"' in text or "class=\"" in text


def migrate_escaped(literal: str) -> str:
    """Migrates the body of a normal `"..."` literal, escapes intact."""
    # Only \n and \" matter for the sigil rules; other escapes are opaque
    # and are left byte-for-byte alone by round-tripping through a
    # sentinel.
    text = literal.replace("\\\\", "\x00").replace('\\"', "\x01").replace("\\n", "\n")
    text = _mod.migrate(text)
    return (
        text.replace("\n", "\\n").replace("\x01", '\\"').replace("\x00", "\\\\")
    )


def scan(src: str) -> str:
    out = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        # Raw string: r"..." or r#"..."#
        if c == "r" and i + 1 < n and src[i + 1] in '#"':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                end = src.find(close, j + 1)
                if end != -1:
                    body = src[j + 1 : end]
                    out.append(src[i : j + 1])
                    out.append(body if looks_like_output(body) else _mod.migrate(body))
                    out.append(close)
                    i = end + len(close)
                    continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            if j < n:
                body = src[i + 1 : j]
                out.append('"')
                out.append(body if looks_like_output(body) else migrate_escaped(body))
                out.append('"')
                i = j + 1
                continue
        # Line comment: skip wholesale so `//` containing a quote does not
        # start a bogus literal.
        if src.startswith("//", i):
            end = src.find("\n", i)
            end = n if end == -1 else end
            out.append(src[i:end])
            i = end
            continue
        out.append(c)
        i += 1
    return "".join(out)


def main(argv: list[str]) -> int:
    # Only `#[cfg(test)]` bodies are migrated. Production strings include
    # HTML and Typst output (`"<div>"`, `"<span>"`), which look exactly
    # like the `<T>` sigil being retired and must not be touched.
    changed = 0
    for arg in argv[1:]:
        path = Path(arg)
        original = path.read_text(encoding="utf-8")
        marker = original.find("#[cfg(test)]")
        if marker == -1:
            continue
        migrated = original[:marker] + scan(original[marker:])
        if migrated != original:
            path.write_text(migrated, encoding="utf-8")
            changed += 1
            print(f"migrated {path}")
    print(f"{changed} file(s) changed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
