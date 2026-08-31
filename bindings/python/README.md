# tomet (Python Bindings)

High-performance Python bindings for Tomet markup and AST, powered by PyO3.

## Installation

```bash
pip install tomet
```

## Quick Start

```python
import tomet

# 1. Parse data documents (like json.loads)
data = tomet.loads("name: 'Alice'\ncount: 42")
print(data)  # {'name': 'Alice', 'count': 42}

# 2. Parse markup documents into AST dict
doc = tomet.parse_document("#[ Hello World ]\n\n<task>(done: true)[Buy milk]")
print(doc["blocks"])

# 3. Convert to HTML & Markdown
html = tomet.to_html(doc)
md = tomet.to_markdown(doc)

# 4. Format source code
formatted = tomet.format("#[  Messy  Heading  ]\n")
```
