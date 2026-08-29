import { EditorState, type Extension } from "@codemirror/state";
import {
	EditorView,
	keymap,
	lineNumbers,
	highlightActiveLine,
	highlightActiveLineGutter,
	drawSelection,
} from "@codemirror/view";
import {
	syntaxHighlighting,
	HighlightStyle,
	bracketMatching,
	indentOnInput,
} from "@codemirror/language";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { linter, lintGutter, setDiagnostics, type Diagnostic } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";
import { tometLanguage } from "./tomet-mode";
import { presets } from "./presets";

// -- Theme: reuses the exact CSS custom properties already in
// style.css, so the editor never drifts out of sync with the rest of
// the page's dark theme.
const editorTheme = EditorView.theme(
	{
		"&": {
			color: "var(--text-main)",
			backgroundColor: "var(--bg-input)",
			height: "100%",
			fontSize: "14.5px",
		},
		".cm-content": {
			fontFamily: "'Fira Code', monospace",
			padding: "20px 24px",
			caretColor: "var(--text-main)",
		},
		".cm-gutters": {
			backgroundColor: "var(--bg-input)",
			color: "var(--text-muted)",
			border: "none",
		},
		".cm-activeLine": { backgroundColor: "rgba(255, 255, 255, 0.04)" },
		".cm-activeLineGutter": { backgroundColor: "rgba(255, 255, 255, 0.06)" },
		".cm-selectionBackground": { backgroundColor: "rgba(59, 130, 246, 0.35) !important" },
		"&.cm-focused .cm-selectionBackground": { backgroundColor: "rgba(59, 130, 246, 0.35) !important" },
		".cm-cursor": { borderLeftColor: "var(--text-main)" },
		".cm-matchingBracket": { backgroundColor: "rgba(59, 130, 246, 0.25)", outline: "1px solid var(--accent)" },
		".cm-lintRange-error": { textDecoration: "underline wavy var(--error)" },
		".cm-diagnostic-error": {
			borderLeftColor: "var(--error)",
			backgroundColor: "var(--bg-panel)",
			color: "var(--text-main)",
		},
		".cm-tooltip": {
			backgroundColor: "var(--bg-panel)",
			color: "var(--text-main)",
			border: "1px solid var(--border)",
		},
	},
	{ dark: true },
);

// Deliberately restrained: four accent hues total (violet for headings
// only, sky blue for structural/element tokens, emerald for strings and
// code, amber for numbers/operators/list markers), everything else
// falls back to the theme's own muted gray. The previous version gave
// almost every token its own distinct saturated hue (six at once,
// visible together on any real document) -- individually reasonable
// colors, but competing for attention rather than establishing a clear
// hierarchy.
const SKY = "#7dd3fc";
const VIOLET = "#c4b5fd";
const EMERALD = "#6ee7b7";
const AMBER = "#fbbf24";
const MUTED = "var(--text-muted)";

const highlightStyle = HighlightStyle.define([
	{ tag: t.heading, color: VIOLET, fontWeight: "700" },
	{ tag: t.contentSeparator, color: MUTED },
	{ tag: t.list, color: AMBER, fontWeight: "600" },
	{ tag: t.lineComment, color: MUTED, fontStyle: "italic" },
	{ tag: t.blockComment, color: MUTED, fontStyle: "italic" },
	{ tag: t.monospace, color: EMERALD },
	{ tag: t.tagName, color: SKY, fontWeight: "600" },
	{ tag: t.propertyName, color: SKY },
	{ tag: t.string, color: EMERALD },
	{ tag: t.number, color: AMBER },
	{ tag: t.operator, color: AMBER },
	{ tag: t.punctuation, color: MUTED },
	{ tag: t.special(t.brace), color: SKY },
	{ tag: t.function(t.variableName), color: SKY },
	{ tag: t.strong, fontWeight: "700" },
	{ tag: t.emphasis, fontStyle: "italic" },
	{ tag: t.strikethrough, textDecoration: "line-through" },
	{ tag: t.processingInstruction, color: MUTED },
]);

interface ParseErrorInfo {
	message: string;
	line: number;
	column: number;
	offset: number;
	formatted: string;
}

interface ParseResult {
	ok: boolean;
	html: string;
	ast: string;
	markdown: string;
	error: ParseErrorInfo | null;
}

/**
 * Converts a UTF-8 byte offset (as reported by the Rust parser, which
 * indexes source bytes) to a UTF-16 code-unit offset (as CodeMirror's
 * `Diagnostic.from`/`.to` and `EditorState`'s doc positions expect).
 * Needed because Tomet source routinely contains non-ASCII text
 * (this project's own docs are largely Japanese) where the two indices
 * diverge.
 */
function byteOffsetToUtf16(source: string, byteOffset: number): number {
	const bytes = new TextEncoder().encode(source);
	const clamped = Math.max(0, Math.min(byteOffset, bytes.length));
	return new TextDecoder().decode(bytes.slice(0, clamped)).length;
}

class App {
	private view!: EditorView;
	private readonly editorHost: HTMLElement;
	private readonly presetSelect: HTMLSelectElement;
	private readonly advancedCheck: HTMLInputElement;
	private readonly formatBtn: HTMLButtonElement;
	private readonly statusText: HTMLElement;
	private readonly statusDot: HTMLElement;
	private readonly problemsPanel: HTMLElement;
	private readonly previewFrame: HTMLIFrameElement;
	private readonly tabButtons: NodeListOf<HTMLButtonElement>;
	private readonly tabViews: Record<string, HTMLElement>;
	private activeTab = "preview";
	private debounceTimer: ReturnType<typeof setTimeout> | undefined;

	constructor() {
		this.editorHost = document.getElementById("editor-host")!;
		this.presetSelect = document.getElementById("preset-select") as HTMLSelectElement;
		this.advancedCheck = document.getElementById("advanced-check") as HTMLInputElement;
		this.formatBtn = document.getElementById("format-btn") as HTMLButtonElement;
		this.statusText = document.getElementById("status-text")!;
		this.statusDot = document.querySelector(".status-dot")!;
		this.problemsPanel = document.getElementById("problems-panel")!;
		this.previewFrame = document.getElementById("preview-frame") as HTMLIFrameElement;
		this.tabButtons = document.querySelectorAll(".tab");
		this.tabViews = {
			preview: document.getElementById("tab-preview")!,
			ast: document.getElementById("tab-ast")!,
			markdown: document.getElementById("tab-markdown")!,
			source: document.getElementById("tab-source")!,
		};

		this.setUpEditor(presets.cheatsheet);
		this.setUpControls();
		void this.triggerParse();
	}

	private setUpEditor(doc: string): void {
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
			editorTheme,
			linter(null),
			lintGutter(),
			EditorView.lineWrapping,
			EditorView.updateListener.of((update) => {
				if (update.docChanged) {
					this.scheduleParse();
				}
			}),
		];

		this.view = new EditorView({
			state: EditorState.create({ doc, extensions }),
			parent: this.editorHost,
		});
	}

	private setUpControls(): void {
		this.tabButtons.forEach((btn) => {
			btn.addEventListener("click", () => {
				this.tabButtons.forEach((b) => b.classList.remove("active"));
				btn.classList.add("active");
				this.activeTab = btn.dataset.tab!;
				Object.keys(this.tabViews).forEach((k) => {
					this.tabViews[k].style.display = k === this.activeTab ? "block" : "none";
				});
			});
		});

		this.presetSelect.addEventListener("change", (e) => {
			const key = (e.target as HTMLSelectElement).value as keyof typeof presets;
			if (presets[key]) {
				this.setDocument(presets[key]);
				void this.triggerParse();
			}
		});

		this.advancedCheck.addEventListener("change", () => void this.triggerParse());

		this.formatBtn.addEventListener("click", () => void this.applyFormat());
	}

	private setDocument(doc: string): void {
		this.view.dispatch({
			changes: { from: 0, to: this.view.state.doc.length, insert: doc },
		});
	}

	private scheduleParse(): void {
		clearTimeout(this.debounceTimer);
		this.debounceTimer = setTimeout(() => void this.triggerParse(), 150);
	}

	private async applyFormat(): Promise<void> {
		const source = this.view.state.doc.toString();
		const cursorLine = this.view.state.doc.lineAt(this.view.state.selection.main.head);
		const cursorCol = this.view.state.selection.main.head - cursorLine.from;

		try {
			const res = await fetch("/api/format", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ source }),
			});
			const data: { formatted: string } = await res.json();
			if (typeof data.formatted !== "string") {
				return;
			}
			this.setDocument(data.formatted);

			// Best-effort cursor restoration: formatting only trims/collapses
			// whitespace, so keeping the same line number and clamping the
			// column to that line's new length lands close to where the user
			// was, instead of always snapping back to the document start.
			const newDoc = this.view.state.doc;
			const targetLineNum = Math.min(cursorLine.number, newDoc.lines);
			const targetLine = newDoc.line(targetLineNum);
			const targetPos = targetLine.from + Math.min(cursorCol, targetLine.length);
			this.view.dispatch({ selection: { anchor: targetPos } });
			this.view.focus();

			void this.triggerParse();
		} catch {
			this.setStatus(false, "Server connection error");
		}
	}

	private setStatus(ok: boolean, text: string): void {
		this.statusDot.className = `status-dot ${ok ? "ok" : "err"}`;
		this.statusText.textContent = text;
	}

	private async triggerParse(): Promise<void> {
		const source = this.view.state.doc.toString();
		const advanced = this.advancedCheck.checked;

		let data: ParseResult;
		try {
			const res = await fetch("/api/parse", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ source, advanced }),
			});
			data = await res.json();
		} catch {
			this.setStatus(false, "Server connection error");
			return;
		}

		if (data.ok) {
			this.setStatus(true, "Valid Tomet Document");
			// `data.html` is a *complete* standalone HTML document (own
			// <head>/<style>), not a fragment -- `srcdoc` on an isolated
			// iframe is the only way to show it without its own styles
			// leaking into this page (an earlier `innerHTML` assignment did
			// exactly that: its `body { max-width: 48rem; margin: 2rem
			// auto; }` was silently overriding *this* page's real body).
			this.previewFrame.srcdoc = data.html;
			this.tabViews.ast.textContent = data.ast;
			this.tabViews.markdown.textContent = data.markdown;
			this.tabViews.source.textContent = data.html;
			this.showProblems(null);
			this.view.dispatch(setDiagnostics(this.view.state, []));
			return;
		}

		const err = data.error!;
		this.setStatus(false, `Parse Error line ${err.line}:${err.column} - ${err.message}`);
		this.showProblems(err.formatted);

		const from = byteOffsetToUtf16(source, err.offset);
		const to = Math.min(from + 1, this.view.state.doc.length);
		const diagnostic: Diagnostic = {
			from,
			to: to > from ? to : from,
			severity: "error",
			message: err.message,
		};
		this.view.dispatch(setDiagnostics(this.view.state, [diagnostic]));
	}

	private showProblems(formatted: string | null): void {
		if (formatted === null) {
			this.problemsPanel.style.display = "none";
			this.problemsPanel.textContent = "";
			return;
		}
		this.problemsPanel.style.display = "block";
		this.problemsPanel.textContent = formatted;
	}
}

document.addEventListener("DOMContentLoaded", () => {
	new App();
});
