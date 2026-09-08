/**
 * Span information representing exact source location.
 */
export interface Span {
  start: number;
  end: number;
  line: number;
  column: number;
}

/**
 * Pure data values supported in Tomet.
 */
export type Value =
  | null
  | boolean
  | number
  | string
  | Value[]
  | { [key: string]: Value };

export type Sigil =
  | 'Bare'
  | 'Angle'
  | 'At'
  | 'Dollar'
  | 'Ampersand'
  | 'Colon'
  | 'Pipe'
  | 'Tilde'
  | 'Hash';

export interface InlineText {
  content: string;
  span: Span;
}

export type Inline =
  | { type: 'Text'; data: InlineText }
  | { type: 'Element'; data: Element }
  | { type: 'Code'; content: string; span: Span };

export type ElementValue =
  | { type: 'Data'; data: Value }
  | { type: 'Children'; data: Element[] };

export interface Element {
  sigil: Sigil;
  name: string | null;
  args: Value | null;
  content: Inline[] | string | null;
  value: ElementValue | null;
  span: Span;
}

export type Block =
  | { type: 'Heading'; level: number; content: Inline[]; span: Span }
  | { type: 'Paragraph'; inlines: Inline[]; span: Span }
  | { type: 'ThematicBreak'; title: Inline[] | null; span: Span }
  | { type: 'Element'; data: Element }
  | { type: 'LineComment'; content: string; span: Span }
  | { type: 'BlockComment'; content: string; span: Span };

export interface Document {
  blocks: Block[];
  span: Span;
}

export interface ValidationError {
  message: string;
  span: Span;
}

/**
 * Parse `.tmt` markup source text into a JavaScript `Document` AST object.
 */
export function parseDocument(source: string): Document;

/**
 * Parse a data-only `.tmt` document into a native JavaScript object/primitive.
 */
export function parseValue(source: string): any;

export interface ProcessOptions {
  advanced?: boolean;
  numberHeadings?: boolean;
  autoSlugHeadings?: boolean;
  lang?: string;
  currentPath?: string;
  urlPrefix?: string;
  assetPrefix?: string;
  vaultFiles?: string[];
}

export interface TocItem {
  id: string;
  level: number;
  text: string;
}

export interface ProcessedDoc {
  title: string | null;
  html: string;
  meta: Value | null;
  toc: TocItem[];
  is_data_only: boolean;
}

/**
 * Convert `.tmt` source text or a `Document` AST object into an HTML string.
 */
export function toHtml(sourceOrDoc: string | Document, options?: ProcessOptions): string;

/**
 * Process `.tmt` markup source text or a `Document` AST object into HTML, metadata, title, and TOC.
 */
export function processDocument(
  sourceOrDoc: string | Document,
  options?: ProcessOptions,
): ProcessedDoc;

/**
 * Convert `.tmt` source text or a `Document` AST object into a CommonMark Markdown string.
 */
export function toMarkdown(sourceOrDoc: string | Document): string;

/**
 * Convert `.tmt` source text or a `Document` AST object into a Typst markup string.
 */
export function toTypst(sourceOrDoc: string | Document): string;

/**
 * Parse CommonMark Markdown text into a `Document` AST object.
 */
export function fromMarkdown(markdown: string): Document;

/**
 * Serialize a `Document` AST object back into formatted `.tmt` source code.
 */
export function printDocument(doc: Document): string;

/**
 * Format `.tmt` source text with lossless whitespace hygiene and span preservation.
 */
export function formatSource(source: string): string;

/**
 * Validate `.tmt` source text and return an array of validation diagnostics.
 */
export function validate(source: string): ValidationError[];

/**
 * Initialize the WebAssembly module.
 */
export default function init(module_or_path?: any): Promise<any>;

/**
 * Initialize the WebAssembly module synchronously.
 */
export function initSync(module?: any): any;
