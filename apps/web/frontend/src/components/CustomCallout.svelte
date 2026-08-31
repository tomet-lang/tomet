<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    args?: any;
    className?: string;
    children?: Snippet;
  }

  let { args, className, children }: Props = $props();

  let type = $derived(typeof args?.type === 'string' ? args.type : 'note');

  const icons: Record<string, string> = {
    info: 'ℹ️',
    tip: '💡',
    warning: '⚠️',
    caution: '🚨',
    note: '📝',
  };
</script>

<aside class={`custom-callout callout-${type} ${className || ''}`}>
  <div class="callout-icon">{icons[type] || '📌'}</div>
  <div class="callout-content">
    {@render children?.()}
  </div>
</aside>

<style>
  .custom-callout {
    display: flex;
    gap: 12px;
    padding: 14px 18px;
    margin: 16px 0;
    border-radius: 8px;
    background: rgba(30, 41, 59, 0.5);
    border-left: 4px solid #3b82f6;
  }
  .callout-info { border-left-color: #3b82f6; background: rgba(59, 130, 246, 0.1); }
  .callout-tip { border-left-color: #10b981; background: rgba(16, 185, 129, 0.1); }
  .callout-warning { border-left-color: #f59e0b; background: rgba(245, 158, 11, 0.1); }
  .callout-caution { border-left-color: #ef4444; background: rgba(239, 68, 68, 0.1); }
  .callout-icon { font-size: 1.2rem; flex-shrink: 0; line-height: 1.4; }
  .callout-content { flex: 1; min-width: 0; }
</style>
