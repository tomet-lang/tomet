<script lang="ts">
  import type { Block, TometSvelteComponents } from './types';
  import InlineRenderer from './InlineRenderer.svelte';
  import ElementRenderer from './ElementRenderer.svelte';
  import { extractTextFromInlines } from './utils';

  interface Props {
    block: Block;
    components?: TometSvelteComponents;
    macros?: Record<string, string>;
  }

  let { block, components, macros }: Props = $props();

  let paragraphInlines = $derived(
    'Paragraph' in block
      ? block.Paragraph.content
      : 'type' in block && block.type === 'Paragraph'
      ? block.content || (block as any).inlines || []
      : null
  );

  let isEmptyParagraph = $derived(
    paragraphInlines ? !extractTextFromInlines(paragraphInlines).trim() && paragraphInlines.length <= 1 : false
  );

  let headingLevel = $derived(
    'type' in block && block.type === 'Heading' ? Math.min(Math.max(block.level || 1, 1), 6) : null
  );

  let thematicBreakTitle = $derived(
    'type' in block && block.type === 'ThematicBreak' ? block.title : null
  );
</script>

{#if paragraphInlines !== null}
  {#if !isEmptyParagraph}
    <p class="tomet-p">
      <InlineRenderer inlines={paragraphInlines} {components} {macros} />
    </p>
  {/if}
{:else if headingLevel !== null && 'type' in block && block.type === 'Heading'}
  <svelte:element this={`h${headingLevel}`} class={`tomet-heading tomet-h${headingLevel}`}>
    <InlineRenderer inlines={block.content} {components} {macros} />
  </svelte:element>
{:else if 'type' in block && block.type === 'ThematicBreak'}
  {#if thematicBreakTitle && thematicBreakTitle.length > 0}
    <div class="tomet-hr-titled">
      <hr />
      <span class="tomet-hr-title">
        <InlineRenderer inlines={thematicBreakTitle} {components} {macros} />
      </span>
      <hr />
    </div>
  {:else}
    <hr class="tomet-hr" />
  {/if}
{:else if 'Element' in block}
  <ElementRenderer element={block.Element} {components} {macros} isInline={false} />
{:else if 'type' in block && block.type === 'Element' && (block as any).data}
  <ElementRenderer element={(block as any).data} {components} {macros} isInline={false} />
{/if}
