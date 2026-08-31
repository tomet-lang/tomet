import React, { FC, useMemo } from 'react';
import type { Document, TometProps } from './types';
import { extractDocumentMacros } from './utils';
import { BlockRenderer } from './BlockRenderer';

export const Tomet: FC<TometProps> = ({
  content,
  ast,
  components,
  className = 'tomet-document',
  parse,
  fallback = null,
}) => {
  const documentAst: Document | null = useMemo(() => {
    if (ast) return ast;
    if (content && typeof parse === 'function') {
      try {
        return parse(content);
      } catch (err) {
        console.error('Failed to parse Tomet document:', err);
        return null;
      }
    }
    return null;
  }, [ast, content, parse]);

  const macros = useMemo(() => extractDocumentMacros(documentAst), [documentAst]);

  if (!documentAst || !documentAst.blocks || documentAst.blocks.length === 0) {
    return fallback ? <>{fallback}</> : null;
  }

  return (
    <article className={className}>
      {documentAst.blocks.map((block, index) => (
        <BlockRenderer key={index} block={block} components={components} macros={macros} />
      ))}
    </article>
  );
};
