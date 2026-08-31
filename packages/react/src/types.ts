import type { ReactNode, ComponentType } from 'react';

/**
 * Span information representing exact source location.
 */
export interface Span {
  start: number;
  end: number;
  line?: number;
  column?: number;
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
  | { Type: string }
  | { At: string | null }
  | 'Bare'
  | 'Dollar'
  | string;

export interface TextNode {
  value: string;
  span?: Span;
}

export type Inline =
  | { Text: TextNode }
  | { Element: Element }
  | { type: 'Text'; value: string; span?: Span }
  | { type: 'Element'; data: Element }
  | { type: 'Code'; content: string; span?: Span };

export type ElementValue =
  | { Data: Value }
  | { Children: Element[] }
  | { Interp: unknown }
  | { type: 'Data'; data: Value }
  | { type: 'Children'; data: Element[] };

export interface Element {
  sigil: Sigil;
  name?: string | null;
  args?: Value | null;
  content?: Inline[] | null;
  children?: Block[] | null;
  value?: ElementValue | null;
  span?: Span;
}

export interface Paragraph {
  content: Inline[];
  span?: Span;
}

export type Block =
  | { Paragraph: Paragraph }
  | { Element: Element }
  | { type: 'Paragraph'; content: Inline[]; inlines?: Inline[]; span?: Span }
  | { type: 'Element'; data: Element }
  | { type: 'Heading'; level: number; content: Inline[]; span?: Span }
  | { type: 'ThematicBreak'; title?: Inline[] | null; span?: Span };

export interface Document {
  blocks: Block[];
  span?: Span;
}

/**
 * Props passed to custom element component overrides.
 */
export interface ElementComponentProps {
  name: string;
  sigil: Sigil;
  args?: Value | null;
  content?: Inline[] | null;
  children?: ReactNode;
  value?: ElementValue | null;
  rawElement: Element;
  className?: string;
}

export interface HeadingProps {
  level: number;
  id?: string;
  className?: string;
  children: ReactNode;
}

export interface ParagraphProps {
  className?: string;
  children: ReactNode;
}

export interface LinkProps {
  href: string;
  className?: string;
  args?: Value | null;
  children: ReactNode;
}

export interface ListProps {
  ordered: boolean;
  className?: string;
  children: ReactNode;
}

export interface ListItemProps {
  args?: Value | null;
  className?: string;
  children: ReactNode;
}

export interface TaskProps {
  done: boolean;
  className?: string;
  args?: Value | null;
  children: ReactNode;
}

export interface CalloutProps {
  type?: string;
  className?: string;
  args?: Value | null;
  children: ReactNode;
}

export interface CodeBlockProps {
  language?: string;
  className?: string;
  content: string;
  children?: ReactNode;
}

export interface TableProps {
  className?: string;
  children: ReactNode;
}

export interface HrProps {
  title?: ReactNode;
  className?: string;
}

export interface InlineStyleProps {
  className?: string;
  children: ReactNode;
}

/**
 * Component override map for Tomet rendering.
 */
export interface TometComponents {
  // Built-in structural overrides
  heading?: ComponentType<HeadingProps>;
  p?: ComponentType<ParagraphProps>;
  link?: ComponentType<LinkProps>;
  list?: ComponentType<ListProps>;
  listItem?: ComponentType<ListItemProps>;
  task?: ComponentType<TaskProps>;
  callout?: ComponentType<CalloutProps>;
  codeblock?: ComponentType<CodeBlockProps>;
  blockquote?: ComponentType<ParagraphProps>;
  table?: ComponentType<TableProps>;
  hr?: ComponentType<HrProps>;
  em?: ComponentType<InlineStyleProps>;
  strong?: ComponentType<InlineStyleProps>;
  mark?: ComponentType<InlineStyleProps>;

  // Custom element tag overrides (e.g. <badge>, <embed>, <author>, etc.)
  [tagName: string]: ComponentType<any> | undefined;
}

export interface TometProps {
  /**
   * Tomet source markup string (will be parsed if provided).
   */
  content?: string;

  /**
   * Pre-parsed Tomet Document AST object.
   */
  ast?: Document | null;

  /**
   * Optional custom components map.
   */
  components?: TometComponents;

  /**
   * Optional root container CSS class.
   */
  className?: string;

  /**
   * Optional parser function (e.g. from @tomet/tomet-wasm parseDocument).
   */
  parse?: (source: string) => Document;

  /**
   * Optional fallback rendered while parsing or on empty doc.
   */
  fallback?: ReactNode;
}
