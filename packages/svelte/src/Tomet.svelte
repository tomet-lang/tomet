<script lang="ts">
  import type { Document, TometProps } from './types';
  import { extractDocumentMacros } from './utils';
  import BlockRenderer from './BlockRenderer.svelte';

  let {
    content,
    ast,
    components,
    class: className = 'tomet-document',
    parse,
  }: TometProps = $props();

  let documentAst: Document | null = $derived.by(() => {
    if (ast) return ast;
    if (content && typeof parse === 'function') {
      try {
        return parse(content);
      } catch (err) {
        console.error('Failed to parse Tomet document:', err);
        return null;
      }
    }
    return null;
  });

  let macros = $derived(extractDocumentMacros(documentAst));
</script>

{#if documentAst && documentAst.blocks && documentAst.blocks.length > 0}
  <article class={className}>
    {#each documentAst.blocks as block}
      <BlockRenderer {block} {components} {macros} />
    {/each}
  </article>
{/if}
