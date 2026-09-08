// Syntax highlighting for Tomet (`.tmt`) source, as a CodeMirror 6
// `StreamLanguage`. Deliberately a flat, single-pass tokenizer, not a
// port of the real grammar's recursive structure: it colors token
// *shapes* wherever they appear (e.g. `identifier:` always reads as a
// map-entry key) rather than only where the real grammar would permit
// them. That's a fine trade-off for a live-preview editor -- the same
// kind of approximation `crates/tree-sitter-tomet/grammar.js`
// already makes for the same reason (see that crate's own module doc).
// Ported from `editors/vscode/syntaxes/tomet.tmLanguage.json`,
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

interface TometState {
	inBlockComment: boolean;
	inFencedCode: boolean;
	/** Innermost-last stack of currently-open emphasis/strong/mark delimiters. */
	emphasisStack: EmphasisDelim[];
	/** Depth inside `(...)` or `{...}` where property keys (`key:`) are active. */
	inValueGroup: number;
}

function startState(): TometState {
	return { inBlockComment: false, inFencedCode: false, emphasisStack: [], inValueGroup: 0 };
}

function emphasisTag(delim: EmphasisDelim): string {
	return delim === "==" ? "strikethrough" : delim.length === 2 ? "strong" : "emphasis";
}

const CONNECT_RE = /^(<->|->|<-|==>|--)/;
const HEADING_RE = /^\s*#+(?=\[)/;
const THEMATIC_BREAK_RE = /^\s*-{3,}\s*$/;
const TITLED_BREAK_OPEN_RE = /^\s*-{3,}(?=\[)/;
const FENCE_RE = /^\s*```([a-zA-Z0-9_+-]*)\s*$/;
const LIST_MARKER_RE = /^\s*(-\.|-)(?=[ \t])(\s*\([xX ?T! -]\))?/;
const TYPE_ELEMENT_RE = /^<[A-Za-z_][A-Za-z0-9_.-]*>/;
const AT_ELEMENT_RE = /^@[A-Za-z_][A-Za-z0-9_.-]*|^@/;
const MAP_KEY_RE = /^[A-Za-z_][A-Za-z0-9_.-]*(?=\s*:)/;
const INTERPOLATION_CALL_RE = /^\$([A-Za-z_][A-Za-z0-9_.-]*)(?=\s*\()/;
const INTERPOLATION_VAR_RE = /^\$([A-Za-z_][A-Za-z0-9_.-]*)/;
const EMPHASIS_DELIMS = ["**", "__", "==", "*", "_"] as const;

function tokenBase(stream: StringStream, state: TometState): string | null {
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
	if (stream.match(INTERPOLATION_CALL_RE) || stream.match(INTERPOLATION_VAR_RE)) {
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

	// Inside value groups `(...)` or `{...}`: recognize property keys `key:`
	if (state.inValueGroup > 0) {
		if (stream.match(MAP_KEY_RE)) {
			return "propertyName";
		}
		if (stream.match(/^\b(true|false|null)\b/)) {
			return "atom";
		}
	}

	if (stream.match(/^\b[0-9]+(\.[0-9]+)?\b/)) {
		return "number";
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

	// Bracket tracking for group context
	if (stream.match(/^[\({]/)) {
		state.inValueGroup++;
		return "punctuation";
	}
	if (stream.match(/^[\)}]/)) {
		if (state.inValueGroup > 0) state.inValueGroup--;
		return "punctuation";
	}
	if (stream.match(/^[[\]]/)) {
		return "punctuation";
	}
	if (stream.match(/^[,:]/)) {
		return "punctuation";
	}
	if (stream.match("|")) {
		return "controlKeyword";
	}

	if (state.emphasisStack.length > 0) {
		if (stream.match(/^[^*_=`$@<\n\s(){}[\]:,|]+/)) {
			return emphasisTag(state.emphasisStack[state.emphasisStack.length - 1]);
		}
	}

	// Consume a single plain word up to the next sigil or delimiter
	if (stream.match(/^[^\s,()[\]{}:"`@<$=*_\\|]+/)) {
		return null;
	}

	stream.next();
	return null;
}

const tometParser: StreamParser<TometState> = {
	name: "tomet",
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
		state.emphasisStack.length = 0;
		state.inValueGroup = 0;
	},
};

export const tometLanguage = StreamLanguage.define(tometParser);

