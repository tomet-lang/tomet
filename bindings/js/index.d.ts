// The functions of `@tomet/wasm` and what they take besides a document. The shape of a document
// (`File`, `Value`, `PlainValue`, ...) is the wire format of the AST, declared in `ast.d.ts`, which
// is generated from the Rust types: read their doc comments there, never here.
import type { Declaration, File, PlainValue, Span } from "./ast.js";

export * from "./ast.js";

export interface ValidationError {
  message: string;
  span: Span;
}

/**
 * Parse `.tmt` markup source text into a JavaScript `File` AST object.
 */
export function parseFile(source: string): File;

/**
 * Parse a data-only `.tmt` document into plain data (`PlainValue`).
 *
 * Throws, naming it, if the document holds anything that is not data: a call,
 * an interpolation, an element, `auto`, ... Read it with `parseFile` to see the AST.
 */
export function parseValue(source: string): PlainValue;

export interface ProcessOptions {
  advanced?: boolean;
  numberHeadings?: boolean;
  autoSlugHeadings?: boolean;
  lang?: string;
  currentPath?: string;
  urlPrefix?: string;
  assetPrefix?: string;
  vaultFiles?: string[];
  config?: string;
}

export interface TocItem {
  id: string;
  level: number;
  text: string;
}

/**
 * What `processFile` returns. The document's `::meta` is seen through two windows:
 *
 * - `meta` is the data of it, as an object of plain data. An entry that is not
 *   data (an `@element` such as `icon: @doc.icon("star")`, a call, `auto`, ...)
 *   is not in it: nothing stands in its place, and it is not an error. `meta` is
 *   `null` if the document writes no `::meta`, and `{}` if it writes one with no
 *   entry that is data.
 * - `header` is everything the document declares, `::meta` included, as the AST
 *   (`Declaration`): where an application finds what `meta` leaves out, and
 *   interprets it. Tomet does not resolve an icon; the application does.
 *
 * Both are read after the expressions of the document are resolved.
 */
export interface ProcessedFile {
  title: string | null;
  html: string;
  meta: { [key: string]: PlainValue } | null;
  header: Declaration[];
  toc: TocItem[];
  is_data_only: boolean;
}

/**
 * Convert `.tmt` source text or a `File` AST object into an HTML string.
 */
export function toHtml(
  sourceOrFile: string | File,
  options?: ProcessOptions,
): string;

/**
 * Process `.tmt` markup source text or a `File` AST object into HTML, metadata, title, and TOC.
 */
export function processFile(
  sourceOrFile: string | File,
  options?: ProcessOptions,
): ProcessedFile;

/**
 * Convert `.tmt` source text or a `File` AST object into a CommonMark Markdown string.
 */
export function toMarkdown(sourceOrFile: string | File): string;

/**
 * Convert `.tmt` source text or a `File` AST object into a Typst markup string.
 */
export function toTypst(sourceOrFile: string | File): string;

/**
 * Parse CommonMark Markdown text into a `File` AST object.
 */
export function fromMarkdown(markdown: string): File;

/**
 * Serialize a `File` AST object back into formatted `.tmt` source code.
 */
export function printFile(file: File): string;

/**
 * Format `.tmt` source text with lossless whitespace hygiene and span preservation.
 */
export function formatSource(source: string): string;

/**
 * Validate `.tmt` source text and return an array of validation diagnostics.
 */
export function validate(source: string): ValidationError[];

/** What `validateWith` is given besides vocabularies; each part is optional. */
export interface ValidateOptions {
  /** The settings, as a config document (what `default.config.tmt` holds): they declare the types of `@meta` fields. */
  config?: string;
  /** Blueprints as source text; the one whose `@blueprint(kind)` matches the document's `@kind` is checked against. */
  blueprints?: string[];
}

/**
 * Validate `.tmt` source text the way `tomet check` does, given the pieces of
 * the vault as source text: vocabularies, settings and blueprints.
 *
 * Settings that do not parse throw; a vocabulary or blueprint that does not is
 * skipped. An expression that reaches another document is not reported.
 */
export function validateWith(
  source: string,
  vocabularies: string[],
  options?: ValidateOptions | null,
): ValidationError[];

/**
 * Initialize the WebAssembly module.
 */
export default function init(module_or_path?: any): Promise<any>;

/**
 * Initialize the WebAssembly module synchronously.
 */
export function initSync(module?: any): any;

/**
 * Pre-builds and caches the link resolution index in WebAssembly memory once for the entire vault.
 */
export function setVaultFiles(files: string[]): void;

/**
 * Clears the cached link resolution index.
 */
export function clearVaultFiles(): void;

/** A range of the source to color. Offsets are bytes of the source. */
export interface HighlightSpan {
  from: number;
  to: number;
  tag: string;
  depth: number;
}

/**
 * Compute syntax highlight token spans for CodeMirror and other editors: the ranges of the source
 * that have a tag, in order and without overlap.
 */
export function highlightSpans(source: string): HighlightSpan[];
