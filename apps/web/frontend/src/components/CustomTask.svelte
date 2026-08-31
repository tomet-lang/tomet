<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    args?: any;
    className?: string;
    children?: Snippet;
  }

  let isDone = $state(false);

  $effect(() => {
    isDone = Boolean(args?.done);
  });

  function toggle() {
    isDone = !isDone;
  }
</script>

<div
  class={`custom-task ${isDone ? 'is-done' : 'is-todo'} ${className || ''}`}
  onclick={toggle}
  onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && toggle()}
  role="checkbox"
  aria-checked={isDone}
  tabindex="0"
>
  <input type="checkbox" checked={isDone} class="task-checkbox" tabindex="-1" />
  <span class="task-body">
    {@render children?.()}
  </span>
</div>

<style>
  .custom-task {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    margin: 4px 0;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .custom-task:hover {
    background: rgba(59, 130, 246, 0.08);
  }
  .task-checkbox {
    cursor: pointer;
    accent-color: #3b82f6;
  }
  .is-done .task-body {
    text-decoration: line-through;
    color: #94a3b8;
  }
</style>
