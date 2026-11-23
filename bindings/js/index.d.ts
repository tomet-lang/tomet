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
 * The scalar a document holds: `none`, `auto`, a boolean, an integer, a float, a
 * number kept as the text it was written as (`1.10`, `1e3`, an integer past
 * `i64`), or a text.
 */
export type Scalar =
  | "None"
  | "Auto"
  | { Bool: boolean }
  | { Int: number }
  | { Float: number }
  | { Number: string }
  | { Text: string };

/**
 * A tuple: entries in order, each with a key or without one. It is the one
 * container, for `(args)` and for a value alike, so there is no list and no
 * map: a list is a tuple whose entries have no key, a map one whose entries all
 * do, and a tuple may mix both. `(x)` is a tuple of one entry, never the bare `x`.
 *
 * An entry with no key is keyed by its position among the unnamed entries of
 * its tuple (`"0"`, `"1"`), so `(a, b)` and `("0": a, "1": b)` are one tuple.
 */
export interface Tuple {
  entries: TupleEntry[];
  /**
   * Where the tuple is written (for a call's arguments, where the whole call
   * is), or `null` if it is not written anywhere, as a tuple a pass built.
   */
  span: Span | null;
}

/** One entry of a `Tuple`: a value, with the key it was written with or `null`. */
export interface TupleEntry {
  key: EntryKey | null;
  value: Value;
}

/** The key of a tuple entry: a plain or quoted name, or a `<$type>` seat. */
export type EntryKey = { Name: string } | { TypeSeat: TypeSeat };

/**
 * What stands where a value goes: a scalar, a tuple, or a node the parser keeps
 * for a later layer. A value reads back as exactly the variant it was written
 * as: nothing is told apart by guessing at its shape.
 *
 * - `Call`: `name(a, b, ...)`, a call the parser did not resolve (`enum(1, 2)`);
 *   its arguments are a `Tuple`;
 * - `Interp`: `{$ ... }` / `$name(...)`, evaluated later;
 * - `Hole`: a `<?matcher(...)>` hole;
 * - `TypeSeat`: a `<$type>` seat;
 * - `Element`: an `@element` sitting where a value goes, as in
 *   `::meta(icon: @doc.icon("star"))`;
 * - `Blocks`: `[ ... ]` block content.
 *
 * For `null`, booleans, numbers, strings, arrays and objects, see `PlainValue`.
 */
export type Value =
  | { Scalar: Scalar }
  | { Tuple: Tuple }
  | { Call: [string, Tuple] }
  | { Interp: Interpolation }
  | { Hole: Hole }
  | { TypeSeat: TypeSeat }
  | { Element: Element }
  | { Blocks: Block[] };

/**
 * Plain data: what `parseValue` and `ProcessedFile.meta` hand out. It is a
 * `Value` seen as data and nothing else, and it is lossy on purpose:
 *
 * - `none` is `null`; a float, and a number kept as written, is a number
 *   (`1.10` is `1.1`);
 * - a tuple with entries and no key on any of them is an array; any other tuple
 *   is an object in written order, an unnamed entry under its position among the
 *   unnamed ones (`(a, k: b)` is `{ "0": a, "k": b }`); an empty tuple is `{}`.
 *   A JavaScript object lists integer-like keys before every other, so a
 *   mixed tuple keeps its entries but not how they interleave;
 * - what is not data (a call, an interpolation, an element, `auto`, ...) has no
 *   plain form. `parseValue` throws, naming it; `ProcessedFile.meta` leaves the
 *   entry out.
 *
 * It does not read back into a `Value`: pass the AST (`parseFile`) to go back.
 */
export type PlainValue =
  | null
  | boolean
  | number
  | string
  | PlainValue[]
  | { [key: string]: PlainValue };

/** The two groups every node that takes them holds: `(args)` and `{value}`. */
export interface Data {
  args: Tuple | null;
  value: Group | null;
}

/** The entries of a `{...}` group, in source order. */
export type Group = Entry[];

export type Entry =
  | { Pair: [string, Value] }
  | { Element: Element }
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

/** A `::name(args)[content]{value}` stacked after an owner. */
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

/** A `<?matcher(...)>` hole. */
export interface Hole {
  matcher: Name | null;
  placement: Placement;
  data: Data;
  content: Block[] | null;
  span: Span;
}

/**
 * A `<$type(args)>` seat: the type of a value that is not there yet, as the
 * expression it is written (`uuid` is a name, `list(string, length: 6)` a call),
 * left to the evaluator. Which names are types is not decided here.
 */
export interface TypeSeat {
  expr: InterpExpr;
  span: Span;
}

/** The expression of an interpolation (`{$ ... }`, `$(...)`, `$name(...)`), left to the evaluator. */
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
  | { TypeSeat: TypeSeat }
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
  | { TypeSeat: TypeSeat }
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
