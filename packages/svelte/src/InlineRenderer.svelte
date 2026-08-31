<script lang="ts">
  import type { Inline, TometSvelteComponents } from './types';
  import ElementRenderer from './ElementRenderer.svelte';

  interface Props {
    inlines?: Inline[] | null;
    components?: TometSvelteComponents;
    macros?: Record<string, string>;
  }

  let { inlines, components, macros }: Props = $props();
</script>

{#if inlines && Array.isArray(inlines)}
  {#each inlines as inline}
    {#if 'Text' in inline}
      {inline.Text.value}
    {:else if 'type' in inline && inline.type === 'Text'}
      {inline.value}
    {:else if 'type' in inline && inline.type === 'Code'}
      <code class="tomet-inline-code">{inline.content}</code>
    {:else if 'Element' in inline}
      <ElementRenderer element={inline.Element} {components} {macros} isInline={true} />
    {:else if 'type' in inline && inline.type === 'Element' && inline.data}
      <ElementRenderer element={inline.data} {components} {macros} isInline={true} />
    {/if}
  {/each}
{/if}
