<script lang="ts">
  import type { Element, TometSvelteComponents } from './types';
  import {
    getElementKind,
    isDirective,
    getHeadingLevel,
    extractLinkHref,
    extractTextFromInlines,
    getElementChildren,
    evaluateInterpExpr,
  } from './utils';
  import InlineRenderer from './InlineRenderer.svelte';
  import BlockRenderer from './BlockRenderer.svelte';

  interface Props {
    element: Element;
    components?: TometSvelteComponents;
    macros?: Record<string, string>;
    isInline?: boolean;
  }

  let { element, components, macros, isInline = false }: Props = $props();

  let kindInfo = $derived(getElementKind(element));
  let name = $derived(kindInfo.name);
  let normalizedName = $derived(name.toLowerCase());
  let directive = $derived(isDirective(normalizedName));
  let headingLevel = $derived(getHeadingLevel(element, name));
  let customComponent = $derived(components?.[name] || components?.[normalizedName]);

  let isDollar = $derived(
    kindInfo.isDollar ||
    normalizedName === '$' ||
    Boolean(element.value && typeof element.value === 'object' && ('Interp' in element.value || ('type' in element.value && (element.value as any).type === 'Interp')))
  );

  let interpExpr = $derived(
    element.value && typeof element.value === 'object'
      ? 'Interp' in element.value
        ? element.value.Interp
        : 'type' in element.value && (element.value as any).type === 'Interp'
        ? (element.value as any).data
        : null
      : null
  );

  let evaluatedInterpValue = $derived(
    interpExpr ? evaluateInterpExpr(interpExpr, macros) : ''
  );

  let isTask = $derived(
    normalizedName === 'task' ||
    Boolean(element.args && typeof element.args === 'object' && !Array.isArray(element.args) && 'done' in (element.args as any))
  );
  let isDone = $derived(
    Boolean(element.args && typeof element.args === 'object' && !Array.isArray(element.args) ? (element.args as any).done : false)
  );

  let isCallout = $derived(
    normalizedName === 'callout' ||
    ['note', 'tip', 'warning', 'caution', 'info'].includes(normalizedName)
  );
  let calloutType = $derived(
    normalizedName === 'callout'
      ? (element.args && typeof element.args === 'object' && !Array.isArray(element.args) && typeof (element.args as any).type === 'string'
          ? (element.args as any).type
          : 'note')
      : normalizedName
  );

  let linkHref = $derived(normalizedName === 'link' ? extractLinkHref(element.args) : '');
  let codeContent = $derived(normalizedName === 'codeblock' ? extractTextFromInlines(element.content) : '');
  let codeLang = $derived(
    element.args && typeof element.args === 'object' && !Array.isArray(element.args) && typeof (element.args as any).lang === 'string'
      ? (element.args as any).lang
      : typeof element.args === 'string'
      ? element.args
      : ''
  );

  let listChildren = $derived(
    normalizedName === 'ul' || normalizedName === 'ol' ? getElementChildren(element.value) : null
  );
  let CustomComp = $derived(customComponent);
</script>

{#snippet innerContent()}
  <InlineRenderer inlines={element.content} {components} {macros} />
{/snippet}

{#if !directive}
  {#if CustomComp}
    <CustomComp
      {name}
      sigil={element.sigil}
      args={element.args}
      content={element.content}
      value={element.value}
      rawElement={element}
      className={`tomet-element tomet-${normalizedName}`}
    >
      {@render innerContent()}
    </CustomComp>
  {:else if headingLevel !== null}
    <svelte:element
      this={`h${headingLevel}`}
      class={`tomet-heading tomet-h${headingLevel}`}
    >
      {@render innerContent()}
    </svelte:element>
  {:else if isDollar}
    <span class="tomet-interp">{evaluatedInterpValue}</span>
  {:else if normalizedName === 'link'}
    <a href={linkHref} class="tomet-link">
      {@render innerContent()}
    </a>
  {:else if isTask}
    <div class={`tomet-task ${isDone ? 'tomet-task-done' : 'tomet-task-todo'}`}>
      <input type="checkbox" checked={isDone} disabled class="tomet-task-checkbox" />
      <span class="tomet-task-label">{@render innerContent()}</span>
    </div>
  {:else if isCallout}
    <aside class={`tomet-callout tomet-callout-${calloutType}`}>
      {@render innerContent()}
    </aside>
  {:else if normalizedName === 'codeblock'}
    <pre class={`tomet-codeblock ${codeLang ? `language-${codeLang}` : ''}`}><code>{codeContent}</code></pre>
  {:else if normalizedName === 'em'}
    <em class="tomet-em">{@render innerContent()}</em>
  {:else if normalizedName === 'strong'}
    <strong class="tomet-strong">{@render innerContent()}</strong>
  {:else if normalizedName === 'mark'}
    <mark class="tomet-mark">{@render innerContent()}</mark>
  {:else if normalizedName === 'hr'}
    <hr class="tomet-hr" />
  {:else if normalizedName === 'blockquote'}
    <blockquote class="tomet-blockquote">{@render innerContent()}</blockquote>
  {:else if normalizedName === 'ul' || normalizedName === 'ol'}
    <svelte:element this={normalizedName} class={`tomet-list tomet-${normalizedName}`}>
      {#if listChildren}
        {#each listChildren as itemEl}
          <li class="tomet-list-item">
            <InlineRenderer inlines={itemEl.content} {components} {macros} />
          </li>
        {/each}
      {/if}
    </svelte:element>
  {:else if isInline}
    <span class={`tomet-element tomet-${normalizedName}`} data-tomet-tag={normalizedName}>
      {@render innerContent()}
    </span>
  {:else}
    <div class={`tomet-element tomet-${normalizedName}`} data-tomet-tag={normalizedName}>
      {@render innerContent()}
      {#if element.children}
        <div class="tomet-element-children">
          {#each element.children as childBlock}
            <BlockRenderer block={childBlock} {components} {macros} />
          {/each}
        </div>
      {/if}
    </div>
  {/if}
{/if}
