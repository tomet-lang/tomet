/* tslint:disable */
/* eslint-disable */

/**
 * Format `.tmt` source text with lossless whitespace hygiene and span preservation.
 */
export function formatSource(source: string): string;

/**
 * Parse CommonMark Markdown text into a `Document` AST object.
 */
export function fromMarkdown(markdown: string): any;

/**
 * Compute syntax highlight token spans for CodeMirror and other editors.
 */
export function highlightSpans(source: string): any;

/**
 * Parse `.tmt` markup source text into a JavaScript `Document` AST object.
 */
export function parseDocument(source: string): any;

/**
 * Parse a data-only `.tmt` document into a native JavaScript object/primitive.
 */
export function parseValue(source: string): any;

/**
 * Serialize a `Document` AST object back into formatted `.tmt` source code.
 */
export function printDocument(doc_val: any): string;

/**
 * Process `.tmt` markup source text or a `Document` AST object into HTML, metadata, title, and TOC.
 */
export function processDocument(source_or_doc: any, options?: any | null): any;

/**
 * Convert `.tmt` source text or a `Document` AST object into an HTML body string.
 */
export function toHtml(source_or_doc: any, options?: any | null): string;

/**
 * Convert `.tmt` source text or a `Document` AST object into a CommonMark Markdown string.
 */
export function toMarkdown(source_or_doc: any): string;

/**
 * Convert `.tmt` source text or a `Document` AST object into a Typst markup string.
 */
export function toTypst(source_or_doc: any): string;

/**
 * Validate `.tmt` source text and return an array of validation diagnostics.
 */
export function validate(source: string): any;

/**
 * Validate `.tmt` source text against `std` plus the vocabularies given
 * as source text.
 *
 * A host that has the vault's `@vocabulary(...)` files can pass their
 * contents here. Without them, `validate` knows only `std`, so every
 * element a vocabulary declares comes back as unknown -- correct, since
 * nothing said it existed, but not useful in an editor that could have
 * said so.
 *
 * A source that does not parse, or that carries no `@vocabulary(ns)`
 * header, is skipped: it declares no namespace, so there is nothing to
 * bind. Check vocabularies themselves with `tomet check`.
 */
export function validateWith(source: string, vocabularies: string[]): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly formatSource: (a: number, b: number) => [number, number];
    readonly fromMarkdown: (a: number, b: number) => [number, number, number];
    readonly highlightSpans: (a: number, b: number) => [number, number, number];
    readonly parseDocument: (a: number, b: number) => [number, number, number];
    readonly parseValue: (a: number, b: number) => [number, number, number];
    readonly printDocument: (a: any) => [number, number, number, number];
    readonly processDocument: (a: any, b: number) => [number, number, number];
    readonly toHtml: (a: any, b: number) => [number, number, number, number];
    readonly toMarkdown: (a: any) => [number, number, number, number];
    readonly toTypst: (a: any) => [number, number, number, number];
    readonly validate: (a: number, b: number) => [number, number, number];
    readonly validateWith: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
