// The functions of `@tomet/wasm`. What they take and return is declared by the generated files next to
// this one, from the Rust types: `ast.d.ts` is the wire format of a document (`File`, `Value`,
// `PlainValue`, ...) and `binding.d.ts` is everything else (`VaultSources`, `ProcessedFile`,
// `Diagnostic`, ...). Read their doc comments there, never here.
import type { File, PlainValue } from "./ast.js";
import type {
  Diagnostic,
  HighlightSpan,
  ProcessOptions,
  ProcessedFile,
  Registry,
  VaultSources,
} from "./binding.js";

export * from "./ast.js";
export * from "./binding.js";

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

/**
 * A vault to read documents in: its settings, vocabularies and dictionaries, and its files. It is made
 * once from its sources and owns all of its state, so two vaults never see each other's files.
 */
export class Vault {
  /** Throws, naming it, if a part of `sources` is not usable. */
  constructor(sources?: VaultSources);

  /**
   * Read `.tmt` source text or a `File` AST object for a page: HTML, metadata, title, table of contents,
   * and the diagnostics about the document. Throws if the document does not parse; the message starts
   * with `line:column:`.
   */
  processFile(sourceOrFile: string | File, options?: ProcessOptions): ProcessedFile;

  /**
   * Validate `.tmt` source text or a `File` AST object the way `tomet check` does, in this vault.
   */
  validate(sourceOrFile: string | File): Diagnostic[];

  /** Release the memory of the vault. */
  free(): void;
}

/**
 * The files `config` (the text of a `vault.config.tmt`) registers, as paths from its directory:
 * what to read to make the `vocabularies`, `dictionaries` and `blueprints` of `VaultSources`.
 */
export function vaultRegistry(config: string): Registry;

/**
 * Convert `.tmt` source text or a `File` AST object into an HTML string, in a vault of nothing.
 */
export function toHtml(
  sourceOrFile: string | File,
  options?: ProcessOptions,
): string;

/**
 * Process `.tmt` source text or a `File` AST object into HTML, metadata, title, table of contents and
 * diagnostics, in a vault of nothing. For a document that links to others, make a `Vault`.
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
 * Validate `.tmt` source text or a `File` AST object the way `tomet check` does, in a vault of
 * nothing: `std` alone, so every element a vocabulary declares comes back as unknown. For the vault
 * of a project, make a `Vault`.
 */
export function validate(sourceOrFile: string | File): Diagnostic[];

/**
 * Initialize the WebAssembly module.
 */
export default function init(module_or_path?: any): Promise<any>;

/**
 * Initialize the WebAssembly module synchronously.
 */
export function initSync(module?: any): any;

/**
 * Compute syntax highlight token spans for CodeMirror and other editors: the ranges of the source
 * that have a tag, in order and without overlap.
 */
export function highlightSpans(source: string): HighlightSpan[];
