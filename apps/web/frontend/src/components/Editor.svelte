<script lang="ts">
  import { onMount } from 'svelte';
  import { EditorState, RangeSetBuilder, type Extension } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLine,
    highlightActiveLineGutter,
    drawSelection,
    Decoration,
    type DecorationSet,
    ViewPlugin,
    type ViewUpdate,
  } from '@codemirror/view';
  import {
    syntaxHighlighting,
    HighlightStyle,
    bracketMatching,
    indentOnInput,
  } from '@codemirror/language';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { linter, lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint';
  import { tags as t } from '@lezer/highlight';
  import { tometLanguage } from '../tomet-mode';
  import { highlightSpans } from '@tomet/wasm';

  interface Props {
    value: string;
    diagnostics?: Diagnostic[];
    onchange?: (val: string) => void;
  }

  let { value = $bindable(), diagnostics = [], onchange }: Props = $props();

  let hostEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined = $state();

  const tagToClass: Record<string, string> = {
    heading: "cm-tomet-heading",
    contentSeparator: "cm-tomet-separator",
    list: "cm-tomet-list",
    lineComment: "cm-tomet-comment",
    blockComment: "cm-tomet-comment",
    monospace: "cm-tomet-mono",
    tagName: "cm-tomet-tag",
    propertyName: "cm-tomet-prop",
    string: "cm-tomet-string",
    number: "cm-tomet-number",
    atom: "cm-tomet-atom",
    operator: "cm-tomet-operator",
    punctuation: "cm-tomet-punct",
    "brace.special": "cm-tomet-brace-special",
    "variableName.function": "cm-tomet-fn",
    processingInstruction: "cm-tomet-meta",
    strong: "cm-tomet-strong",
    emphasis: "cm-tomet-emphasis",
    strikethrough: "cm-tomet-strike",
  };

  function buildWasmDecorations(v: EditorView): DecorationSet {
    const builder = new RangeSetBuilder<Decoration>();
    const docText = v.state.doc.toString();
    try {
      const rawSpans = highlightSpans(docText) || [];
      const spans = [...rawSpans].sort((a, b) => a.from - b.from || a.to - b.to);
      for (const span of spans) {
        const cls = tagToClass[span.tag];
        if (cls && span.from < span.to && span.to <= docText.length) {
          builder.add(
            span.from,
            span.to,
            Decoration.mark({ class: cls })
          );
        }
      }
    } catch {
      // Fallback silently if wasm is not ready yet
    }
    return builder.finish();
  }

  const tometWasmHighlightPlugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildWasmDecorations(view);
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = buildWasmDecorations(update.view);
        }
      }
    },
    {
      decorations: (v) => v.decorations,
    }
  );

  const editorTheme = EditorView.theme(
    {
      "&": {
        color: "var(--text-main, #f8fafc)",
        backgroundColor: "var(--bg-input, #090d16)",
        height: "100%",
        fontSize: "14.5px",
      },
      ".cm-content": {
        fontFamily: "'Fira Code', monospace",
        padding: "20px 24px",
        caretColor: "var(--text-main, #f8fafc)",
      },
      ".cm-gutters": {
        backgroundColor: "var(--bg-input, #090d16)",
        color: "var(--text-muted, #94a3b8)",
        border: "none",
      },
      ".cm-activeLine": { backgroundColor: "rgba(255, 255, 255, 0.04)" },
      ".cm-activeLineGutter": { backgroundColor: "rgba(255, 255, 255, 0.06)" },
      ".cm-selectionBackground": { backgroundColor: "rgba(59, 130, 246, 0.35) !important" },
      "&.cm-focused .cm-selectionBackground": { backgroundColor: "rgba(59, 130, 246, 0.35) !important" },
      ".cm-cursor": { borderLeftColor: "var(--text-main, #f8fafc)" },
      ".cm-matchingBracket": { backgroundColor: "rgba(59, 130, 246, 0.25)", outline: "1px solid var(--accent, #3b82f6)" },
      ".cm-lintRange-error": { textDecoration: "underline wavy var(--error, #ef4444)" },
      ".cm-lintRange-warning": { textDecoration: "underline wavy #f59e0b" },
      ".cm-diagnostic-error": {
        borderLeftColor: "var(--error, #ef4444)",
        backgroundColor: "var(--bg-panel, #1e293b)",
        color: "var(--text-main, #f8fafc)",
      },
      ".cm-diagnostic-warning": {
        borderLeftColor: "#f59e0b",
        backgroundColor: "var(--bg-panel, #1e293b)",
        color: "var(--text-main, #f8fafc)",
      },
      ".cm-tooltip": {
        backgroundColor: "var(--bg-panel, #1e293b)",
        color: "var(--text-main, #f8fafc)",
        border: "1px solid var(--border, #334155)",
      },
      // WASM CST Highlighting styles
      ".cm-tomet-heading": { color: "#c4b5fd", fontWeight: "700" },
      ".cm-tomet-separator": { color: "#94a3b8" },
      ".cm-tomet-list": { color: "#fbbf24", fontWeight: "600" },
      ".cm-tomet-comment": { color: "#94a3b8", fontStyle: "italic" },
      ".cm-tomet-mono": { color: "#6ee7b7", fontFamily: "'Fira Code', monospace" },
      ".cm-tomet-tag": { color: "#7dd3fc", fontWeight: "600" },
      ".cm-tomet-prop": { color: "#7dd3fc" },
      ".cm-tomet-string": { color: "#6ee7b7" },
      ".cm-tomet-number": { color: "#fbbf24" },
      ".cm-tomet-atom": { color: "#f472b6" },
      ".cm-tomet-operator": { color: "#fbbf24" },
      ".cm-tomet-punct": { color: "#94a3b8" },
      ".cm-tomet-brace-special": { color: "#7dd3fc", fontWeight: "600" },
      ".cm-tomet-fn": { color: "#7dd3fc", fontWeight: "600" },
      ".cm-tomet-meta": { color: "#94a3b8" },
      ".cm-tomet-strong": { fontWeight: "700" },
      ".cm-tomet-emphasis": { fontStyle: "italic" },
      ".cm-tomet-strike": { textDecoration: "line-through" },
    },
    { dark: true },
  );

  const highlightStyle = HighlightStyle.define([
    { tag: t.heading, color: "#c4b5fd", fontWeight: "700" },
    { tag: t.contentSeparator, color: "#94a3b8" },
    { tag: t.list, color: "#fbbf24", fontWeight: "600" },
    { tag: t.lineComment, color: "#94a3b8", fontStyle: "italic" },
    { tag: t.blockComment, color: "#94a3b8", fontStyle: "italic" },
    { tag: t.monospace, color: "#6ee7b7" },
    { tag: t.tagName, color: "#7dd3fc", fontWeight: "600" },
    { tag: t.propertyName, color: "#7dd3fc" },
    { tag: t.string, color: "#6ee7b7" },
    { tag: t.number, color: "#fbbf24" },
    { tag: t.atom, color: "#f472b6" },
    { tag: t.operator, color: "#fbbf24" },
    { tag: t.punctuation, color: "#94a3b8" },
    { tag: t.special(t.brace), color: "#7dd3fc" },
    { tag: t.function(t.variableName), color: "#7dd3fc" },
    { tag: t.strong, fontWeight: "700" },
    { tag: t.emphasis, fontStyle: "italic" },
    { tag: t.strikethrough, textDecoration: "line-through" },
    { tag: t.processingInstruction, color: "#94a3b8" },
  ]);

  onMount(() => {
    if (!hostEl) return;

    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      drawSelection(),
      bracketMatching(),
      indentOnInput(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      tometLanguage,
      syntaxHighlighting(highlightStyle),
      tometWasmHighlightPlugin,
      editorTheme,
      linter(null),
      lintGutter(),
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          const docStr = update.state.doc.toString();
          value = docStr;
          onchange?.(docStr);
        }
      }),
    ];

    view = new EditorView({
      state: EditorState.create({ doc: value, extensions }),
      parent: hostEl,
    });

    return () => {
      view?.destroy();
    };
  });

  $effect(() => {
    if (view && diagnostics) {
      view.dispatch(setDiagnostics(view.state, diagnostics));
    }
  });

  export function setContent(newContent: string) {
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== newContent) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: newContent },
      });
    }
  }

  export function focus() {
    view?.focus();
  }
</script>

<div bind:this={hostEl} class="cm-editor-wrapper"></div>

<style>
  .cm-editor-wrapper {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  :global(.cm-editor-wrapper .cm-editor) {
    height: 100%;
  }
  :global(.cm-editor-wrapper .cm-scroller) {
    overflow: auto;
    font-family: 'Fira Code', monospace;
    line-height: 1.6;
  }
</style>
