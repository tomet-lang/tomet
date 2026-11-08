/**
 * A range of byte offsets into the source text a node was parsed from.
 * Line and column are not part of a span: derive them from the source.
 */
export interface Span {
  start: number;
  end: number;
}

/**
 * A name as written after `@` or `::`: `rule`, or `deck.card` with the
 * namespace split off.
 */
export interface Name {
  namespace: string | null;
  name: string;
}

/** The `#(id)` an element, section, list or item carries. */
export type Id = string;

/**
 * A value of `(args)` or a `{...}` pair.
 *
 * A map is an object; the key of a positional entry is `""`. Beyond plain data a
 * value may be what the source wrote there: a call (`{ call, args }`), a
 * `${...}` (`Interpolation`), a `?` hole, an `@element`, or `[content]`
 * (`Block[]`). A reader tells them apart by their own keys.
 *
 * Only plain data and `Block[]` are read back when a `File` is passed in.
 */
export type Value =
  | null
  | boolean
  | number
  | string
  | Value[]
  | { [key: string]: Value };

/** The two groups every node that takes them holds: `(args)` and `{value}`. */
export interface Data {
  args: Value | null;
  value: Group | null;
}

/** The entries of a `{...}` group, in source order. */
export type Group = Entry[];

export type Entry =
  | { Pair: [string, Value] }
  | { Element: Element }
  /** A nameless `(marker)[content]` entry, as in `@links(){ (1)[...] }`. */
  | { Bare: Bare }
  | { Hole: Hole };

/** Whether an element stands on its own or inside running text. */
export type Placement = "Block" | "Inline";

/** An `@name(args)[content]{value}#(id)` element and its `:connect`s. */
export interface Element {
  name: Name;
  placement: Placement;
  data: Data;
  content: Block[] | null;
  id: Id | null;
  connects: Connect[];
  span: Span;
}

/** A `:name(args)[content]{value}` stacked after an owner. */
export interface Connect {
  name: Name;
  data: Data;
  content: Block[] | null;
  span: Span;
}

/** A `::name(args)[content]{value}#(id)` declaration about the document itself. */
export interface Declaration {
  name: Name;
  data: Data;
  content: Block[] | null;
  id: Id | null;
  span: Span;
}

export interface Bare {
  data: Data;
  content: Block[] | null;
  id: Id | null;
  span: Span;
}

/** A `<?matcher(...)>` hole. */
export interface Hole {
  matcher: Name | null;
  placement: Placement;
  data: Data;
  content: Block[] | null;
  span: Span;
}

/** An expression of `${...}`, left to the evaluator. */
export interface InterpExpr {
  kind: unknown;
  span: Span;
}

export interface Interpolation {
  expr: InterpExpr;
  span: Span;
}

export interface Text {
  value: string;
  span: Span;
}

export type Inline =
  | { Text: Text }
  | { Element: Element }
  | { Interp: Interpolation }
  | { Hole: Hole }
  | { SoftBreak: { span: Span } }
  | { LineBreak: { span: Span } }
  /** Verbatim text, which may hold newlines (a code block's body). */
  | { Raw: Text };

export interface Paragraph {
  content: Inline[];
  span: Span;
}

export interface Section {
  level: number;
  title: Inline[];
  data: Data;
  id: Id | null;
  connects: Connect[];
  blocks: Block[];
  span: Span;
}

export interface ListItem {
  /** `null` for a plain `- content`; the element's name for `-@name(...)`. */
  name: Name | null;
  data: Data;
  content: Block[];
  id: Id | null;
  connects: Connect[];
  sublist: List | null;
  span: Span;
}

export interface List {
  ordered: boolean;
  items: ListItem[];
  id: Id | null;
  connects: Connect[];
  span: Span;
}

export type Block =
  | { Paragraph: Paragraph }
  | { Element: Element }
  | { Section: Section }
  | { List: List }
  | { Hole: Hole }
  | { Interp: Interpolation };

export interface File {
  header: Declaration[];
  body: Block[];
  span: Span;
}

export interface ValidationError {
  message: string;
  span: Span;
}

/**
 * Parse `.tmt` markup source text into a JavaScript `File` AST object.
 */
export function parseFile(source: string): File;

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
  config?: string;
}

export interface TocItem {
  id: string;
  level: number;
  text: string;
}

export interface ProcessedFile {
  title: string | null;
  html: string;
  meta: Value | null;
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
