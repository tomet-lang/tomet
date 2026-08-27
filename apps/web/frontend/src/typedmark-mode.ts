// Syntax highlighting for TypedMark (`.tm`) source, as a CodeMirror 6
// `StreamLanguage`. Deliberately a flat, single-pass tokenizer, not a
// port of the real grammar's recursive structure: it colors token
// *shapes* wherever they appear (e.g. `identifier:` always reads as a
// map-entry key) rather than only where the real grammar would permit
// them. That's a fine trade-off for a live-preview editor -- the same
// kind of approximation `crates/tree-sitter-typedmark/grammar.js`
// already makes for the same reason (see that crate's own module doc).
// Ported from `editors/vscode/syntaxes/typedmark.tmLanguage.json`,
// which is the authoritative list of what token shapes exist.
//
// `token()` below returns plain strings, not `@lezer/highlight` tag
// objects: `@codemirror/language`'s `StreamLanguage` resolves a
// returned string like `"heading"` or `"variableName.function"`
// (base-tag-then-modifier, dot-joined) by looking each dot-separated
// part up in `@lezer/highlight`'s exported `tags` table itself (see
// `createTokenType` in that package's source) -- there's no need to
// import `tags` here at all, the string *is* the lookup key.

import { StreamLanguage } from "@codemirror/language";
import type { StreamParser, StringStream } from "@codemirror/language";

type EmphasisDelim = "**" | "__" | "*" | "_" | "==";

interface TypedMarkState {
	inBlockComment: boolean;
	inFencedCode: boolean;
	/** Innermost-last stack of currently-open emphasis/strong/mark delimiters. */
	emphasisStack: EmphasisDelim[];
}

function startState(): TypedMarkState {
	return { inBlockComment: false, inFencedCode: false, emphasisStack: [] };
}

function emphasisTag(delim: EmphasisDelim): string {
	return delim === "==" ? "strikethrough" : delim.length === 2 ? "strong" : "emphasis";
}

const CONNECT_RE = /^(<->|->|<-|==>|--)/;
const HEADING_RE = /^\s*#+(?=\[)/;
const THEMATIC_BREAK_RE = /^\s*-{3,}\s*$/;
const TITLED_BREAK_OPEN_RE = /^\s*-{3,}(?=\[)/;
const FENCE_RE = /^\s*```([a-zA-Z0-9_+-]*)\s*$/;
const LIST_MARKER_RE = /^\s*(-\.|-)(?=[ \t])/;
const TYPE_ELEMENT_RE = /^<[A-Za-z_][A-Za-z0-9_.-]*>/;
const AT_ELEMENT_RE = /^@[A-Za-z_][A-Za-z0-9_.-]*|^@/;
const MAP_KEY_RE = /^[A-Za-z_][A-Za-z0-9_.-]*(?=:)/;
const UNQUOTED_VALUE_RE = /^[^,()[\]{}\n"\s][^,()[\]{}\n"]*/;
const EMPHASIS_DELIMS = ["**", "__", "==", "*", "_"] as const;

function tokenBase(stream: StringStream, state: TypedMarkState): string | null {
	if (stream.sol()) {
		if (stream.match(FENCE_RE)) {
			state.inFencedCode = !state.inFencedCode;
			return "processingInstruction";
		}
		if (!state.inFencedCode) {
			if (stream.match(TITLED_BREAK_OPEN_RE) || stream.match(HEADING_RE)) {
				return "heading";
			}
			if (stream.match(THEMATIC_BREAK_RE)) {
				return "contentSeparator";
			}
			if (stream.match(LIST_MARKER_RE)) {
				return "list";
			}
		}
	}

	if (state.inFencedCode) {
		stream.skipToEnd();
		return "monospace";
	}

	if (stream.match("//")) {
		stream.skipToEnd();
		return "lineComment";
	}
	if (stream.match("/*")) {
		state.inBlockComment = true;
		return "blockComment";
	}

	if (stream.match(/^`[^`\n]*`/)) {
		return "monospace";
	}

	for (const delim of EMPHASIS_DELIMS) {
		if (stream.match(delim, false)) {
			const isClose = state.emphasisStack[state.emphasisStack.length - 1] === delim;
			// Only treat as a delimiter next to non-space content, matching
			// `tmLanguage.json`'s `(?=\S)` open guard -- an isolated `*`/`_`
			// (e.g. multiplication, italic underscore in a word) stays plain
			// punctuation instead of toggling emphasis state.
			const after = stream.string.slice(stream.pos + delim.length, stream.pos + delim.length + 1);
			if (isClose || (after && !/\s/.test(after))) {
				stream.match(delim);
				if (isClose) {
					state.emphasisStack.pop();
				} else {
					state.emphasisStack.push(delim);
				}
				return emphasisTag(delim);
			}
		}
	}

	if (stream.match("${")) {
		return "brace.special";
	}
	if (stream.match(/^\b[0-9]+(\.[0-9]+)?\b/)) {
		return "number";
	}
	if (stream.match(/^[A-Za-z_][A-Za-z0-9_.-]*(?=\s*\()/)) {
		return "variableName.function";
	}

	if (stream.match(TYPE_ELEMENT_RE)) {
		return "tagName";
	}
	if (stream.match(AT_ELEMENT_RE)) {
		return "tagName";
	}

	if (stream.match(CONNECT_RE)) {
		return "operator";
	}

	if (stream.match(MAP_KEY_RE)) {
		return "propertyName";
	}

	if (stream.match('"')) {
		while (!stream.eol()) {
			if (stream.match(/^\\./)) {
				continue;
			}
			if (stream.match('"')) {
				break;
			}
			stream.next();
		}
		return "string";
	}

	if (stream.match(/^[()[\]{}]/)) {
		return "punctuation";
	}
	if (stream.match(":")) {
		return "punctuation";
	}

	if (state.emphasisStack.length > 0) {
		// Inside an open emphasis/strong/mark span: consume a run of plain
		// characters up to the next delimiter/special character, so the
		// whole span (not just its edges) picks up the emphasis tag.
		if (stream.match(/^[^*_=`$@<\n]+/)) {
			return emphasisTag(state.emphasisStack[state.emphasisStack.length - 1]);
		}
	}

	if (stream.match(UNQUOTED_VALUE_RE)) {
		return null;
	}

	stream.next();
	return null;
}

const typedMarkParser: StreamParser<TypedMarkState> = {
	name: "typedmark",
	startState,
	token(stream, state) {
		if (state.inBlockComment) {
			if (stream.match(/^.*?\*\//)) {
				state.inBlockComment = false;
			} else {
				stream.skipToEnd();
			}
			return "blockComment";
		}
		return tokenBase(stream, state);
	},
	blankLine(state) {
		// A blank line always ends an in-progress emphasis/strong/mark span
		// in the real grammar (`docs/ja/specifications/syntax.tm`'s inline
		// rules never carry those across a blank line) -- drop the stack so
		// highlighting doesn't bleed into an unrelated later paragraph.
		state.emphasisStack.length = 0;
	},
};

export const typedMarkLanguage = StreamLanguage.define(typedMarkParser);
