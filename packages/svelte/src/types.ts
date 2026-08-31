import type { Component, Snippet } from 'svelte';

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

export interface ElementComponentProps {
  name: string;
  sigil: Sigil;
  args?: Value | null;
  content?: Inline[] | null;
  value?: ElementValue | null;
  rawElement: Element;
  className?: string;
}

export interface HeadingProps {
  level: number;
  id?: string;
  className?: string;
}

export interface LinkProps {
  href: string;
  className?: string;
  args?: Value | null;
}

export interface TaskProps {
  done: boolean;
  className?: string;
  args?: Value | null;
}

export interface CalloutProps {
  type?: string;
  className?: string;
  args?: Value | null;
}

export interface CodeBlockProps {
  language?: string;
  className?: string;
  content: string;
}

/**
 * Svelte 5 component / snippet override map for Tomet rendering.
 */
export interface TometSvelteComponents {
  heading?: Component<any> | Snippet<[HeadingProps & { children: Snippet }]>;
  p?: Component<any> | Snippet<[{ className?: string; children: Snippet }]>;
  link?: Component<any> | Snippet<[LinkProps & { children: Snippet }]>;
  task?: Component<any> | Snippet<[TaskProps & { children: Snippet }]>;
  callout?: Component<any> | Snippet<[CalloutProps & { children: Snippet }]>;
  codeblock?: Component<any> | Snippet<[CodeBlockProps]>;
  blockquote?: Component<any> | Snippet<[{ className?: string; children: Snippet }]>;
  hr?: Component<any> | Snippet<[{ className?: string; title?: Snippet }]>;
  [tagName: string]: Component<any> | Snippet<[any]> | undefined;
}

export interface TometProps {
  content?: string;
  ast?: Document | null;
  components?: TometSvelteComponents;
  class?: string;
  parse?: (source: string) => Document;
}
