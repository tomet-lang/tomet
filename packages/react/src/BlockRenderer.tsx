import React, { FC, createElement } from 'react';
import type { Block, TometComponents } from './types';
import { InlineRenderer } from './InlineRenderer';
import { ElementRenderer } from './ElementRenderer';
import { extractTextFromInlines } from './utils';

interface BlockRendererProps {
  block: Block;
  components?: TometComponents;
  macros?: Record<string, string>;
}

export const BlockRenderer: FC<BlockRendererProps> = ({ block, components, macros }) => {
  // 1. Paragraph variant
  if ('Paragraph' in block || ('type' in block && block.type === 'Paragraph')) {
    const inlines = 'Paragraph' in block ? block.Paragraph.content : (block.content || (block as any).inlines || []);
    
    // Check if the paragraph is completely empty or whitespace
    const textContent = extractTextFromInlines(inlines);
    if (!textContent.trim() && (!inlines || inlines.length <= 1)) {
      // Check if it's only an invisible directive
      return null;
    }

    const PComp = components?.p;
    if (PComp) {
      return (
        <PComp className="tomet-p">
          <InlineRenderer inlines={inlines} components={components} macros={macros} />
        </PComp>
      );
    }
    return (
      <p className="tomet-p">
        <InlineRenderer inlines={inlines} components={components} macros={macros} />
      </p>
    );
  }

  // 2. Heading block variant
  if ('type' in block && block.type === 'Heading') {
    const level = Math.min(Math.max(block.level || 1, 1), 6);
    const HeadingComp = components?.heading;
    if (HeadingComp) {
      return (
        <HeadingComp level={level} className={`tomet-heading tomet-h${level}`}>
          <InlineRenderer inlines={block.content} components={components} macros={macros} />
        </HeadingComp>
      );
    }
    return createElement(
      `h${level}`,
      { className: `tomet-heading tomet-h${level}` },
      <InlineRenderer inlines={block.content} components={components} macros={macros} />
    );
  }

  // 3. ThematicBreak block variant
  if ('type' in block && block.type === 'ThematicBreak') {
    const HrComp = components?.hr;
    const titleInlines = block.title;
    if (titleInlines && titleInlines.length > 0) {
      const titleContent = <InlineRenderer inlines={titleInlines} components={components} macros={macros} />;
      if (HrComp) {
        return <HrComp title={titleContent} className="tomet-hr tomet-hr-titled" />;
      }
      return (
        <div className="tomet-hr-titled">
          <hr />
          <span className="tomet-hr-title">{titleContent}</span>
          <hr />
        </div>
      );
    }
    return HrComp ? <HrComp className="tomet-hr" /> : <hr className="tomet-hr" />;
  }

  // 4. Element block variant
  if ('Element' in block) {
    return <ElementRenderer element={block.Element} components={components} macros={macros} isInline={false} />;
  }
  if ('type' in block && block.type === 'Element' && (block as any).data) {
    return <ElementRenderer element={(block as any).data} components={components} macros={macros} isInline={false} />;
  }

  return null;
};
