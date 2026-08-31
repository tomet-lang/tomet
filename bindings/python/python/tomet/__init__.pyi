from typing import Any, Optional, Union

def loads(source: str) -> Any:
    """Parse a data-only .tmt document into native Python objects (dict, list, str, int, float, bool, None)."""
    ...

def parse_document(source: str) -> dict[str, Any]:
    """Parse full .tmt markup source text into a Python dict representing the Document AST."""
    ...

def to_html(source_or_doc: Union[str, dict[str, Any]]) -> str:
    """Convert .tmt source text or a Document AST dict into an HTML body string."""
    ...

def to_markdown(source_or_doc: Union[str, dict[str, Any]]) -> str:
    """Convert .tmt source text or a Document AST dict into a CommonMark Markdown string."""
    ...

def to_typst(source_or_doc: Union[str, dict[str, Any]]) -> str:
    """Convert .tmt source text or a Document AST dict into a Typst markup string."""
    ...

def from_markdown(markdown: str) -> dict[str, Any]:
    """Parse CommonMark Markdown text into a Document AST dict."""
    ...

def print_document(doc: dict[str, Any]) -> str:
    """Serialize a Document AST dict back into formatted .tmt source code."""
    ...

def format(source: str) -> str:
    """Format .tmt source text with lossless whitespace hygiene and span preservation."""
    ...

def validate(source: str) -> list[dict[str, Any]]:
    """Validate .tmt source text and return a list of validation errors."""
    ...
