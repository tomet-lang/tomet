# tomet-html

HTML renderer for Tomet AST documents.

## Responsibilities

1. **HTML Rendering**:
   - `render_page` / `render_page_with`: Renders a complete standalone HTML document with DOCTYPE, CSS styling, and head metadata.
   - `render_body` / `render_body_with`: Renders the HTML fragment representing the body content.

2. **Advanced Rendering Options**:
   - Heading numbering (`number_headings: true` -> `1`, `1.1`, `1.2`, etc.).
   - Automatic heading slug anchors (`auto_slug_headings: true`).
   - Configurable document language (`<html lang="...">`).
   - Semantic CSS classes for callouts, elements, and data attributes.
