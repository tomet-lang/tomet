# @tomet/react

High-performance React components for Tomet markup and AST rendering with custom element overrides.

## Installation

```bash
npm install @tomet/react
# or
pnpm add @tomet/react
```

## Quick Start

### 1. Render from Pre-parsed AST (Recommended for SSR / Next.js)

```tsx
import { Tomet } from '@tomet/react';

export default function Page({ documentAst }) {
  return (
    <Tomet
      ast={documentAst}
      components={{
        // Override explicit links
        link: ({ href, children }) => (
          <a href={href} className="text-blue-500 underline">{children}</a>
        ),
        // Custom Tomet element components (<task>, <callout>, <badge>, etc.)
        task: ({ done, children }) => (
          <div className="flex items-center gap-2">
            <input type="checkbox" checked={done} readOnly />
            <span>{children}</span>
          </div>
        ),
        callout: ({ type, children }) => (
          <aside className={`p-4 rounded-lg bg-gray-100 border-l-4 border-blue-500`}>
            {children}
          </aside>
        ),
      }}
    />
  );
}
```

### 2. Client-side Live Parsing with `@tomet/tomet-wasm`

```tsx
import { Tomet } from '@tomet/react';
import { parseDocument } from '@tomet/tomet-wasm';

export function LivePreview({ sourceText }: { sourceText: string }) {
  return (
    <Tomet
      content={sourceText}
      parse={parseDocument}
      className="prose prose-slate max-w-none"
    />
  );
}
```

## License

MIT OR Apache-2.0
