# @tomet/svelte

High-performance Svelte 5 components (using Runes `$props()` & Snippets) for Tomet markup and AST rendering with custom element overrides.

## Installation

```bash
npm install @tomet/svelte
# or
pnpm add @tomet/svelte
```

## Quick Start

### 1. Render from Pre-parsed AST (SvelteKit / SSR)

```svelte
<script lang="ts">
  import { Tomet } from '@tomet/svelte';
  import CustomTask from './CustomTask.svelte';

  let { documentAst } = $props();

  const components = {
    // Custom Tomet element component mapping
    task: CustomTask,
  };
</script>

<Tomet ast={documentAst} {components} />
```

### 2. Live Client-side Parsing with `@tomet/tomet-wasm`

```svelte
<script lang="ts">
  import { Tomet } from '@tomet/svelte';
  import { parseDocument } from '@tomet/tomet-wasm';

  let sourceText = $state('#[ Hello Svelte 5 ]\n\n<task>(done: true)[Explore Tomet]');
</script>

<Tomet content={sourceText} parse={parseDocument} class="prose max-w-none" />
```

## License

MIT OR Apache-2.0
