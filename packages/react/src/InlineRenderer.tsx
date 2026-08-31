import React, { FC, Fragment } from 'react';
import type { Inline, TometComponents } from './types';
import { ElementRenderer } from './ElementRenderer';

interface InlineRendererProps {
  inlines?: Inline[] | null;
  components?: TometComponents;
  macros?: Record<string, string>;
}

export const InlineRenderer: FC<InlineRendererProps> = ({ inlines, components, macros }) => {
  if (!inlines || !Array.isArray(inlines)) return null;

  return (
    <Fragment>
      {inlines.map((inline, index) => {
        if ('Text' in inline) {
          return <Fragment key={index}>{inline.Text.value}</Fragment>;
        }
        if ('type' in inline && inline.type === 'Text') {
          return <Fragment key={index}>{inline.value}</Fragment>;
        }
        if ('type' in inline && inline.type === 'Code') {
          return <code key={index} className="tomet-inline-code">{inline.content}</code>;
        }
        if ('Element' in inline) {
          return (
            <ElementRenderer
              key={index}
              element={inline.Element}
              components={components}
              macros={macros}
              isInline={true}
            />
          );
        }
        if ('type' in inline && inline.type === 'Element' && inline.data) {
          return (
            <ElementRenderer
              key={index}
              element={inline.data}
              components={components}
              macros={macros}
              isInline={true}
            />
          );
        }
        return null;
      })}
    </Fragment>
  );
};
