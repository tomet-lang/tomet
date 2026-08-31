import React, { FC, ReactNode, createElement } from 'react';
import type { Element, TometComponents } from './types';
import {
  getElementKind,
  isDirective,
  getHeadingLevel,
  extractLinkHref,
  extractTextFromInlines,
  getElementChildren,
  evaluateInterpExpr,
} from './utils';
import { InlineRenderer } from './InlineRenderer';
import { BlockRenderer } from './BlockRenderer';

interface ElementRendererProps {
  element: Element;
  components?: TometComponents;
  macros?: Record<string, string>;
  isInline?: boolean;
}

export const ElementRenderer: FC<ElementRendererProps> = ({
  element,
  components,
  macros,
  isInline = false,
}) => {
  const kindInfo = getElementKind(element);
  const name = kindInfo.name;
  const normalizedName = name.toLowerCase();

  // 1. Skip invisible directives (@meta, @config, @kind, @version)
  if (isDirective(normalizedName)) {
    return null;
  }

  // 2. Interpolation / macro expressions ($name(...) or ${...})
  const isDollar =
    kindInfo.isDollar ||
    normalizedName === '$' ||
    Boolean(element.value && typeof element.value === 'object' && ('Interp' in element.value || ('type' in element.value && (element.value as any).type === 'Interp')));

  if (isDollar) {
    const interpExpr =
      element.value && typeof element.value === 'object'
        ? 'Interp' in element.value
          ? element.value.Interp
          : 'type' in element.value && (element.value as any).type === 'Interp'
          ? (element.value as any).data
          : null
        : null;

    const evaluatedText = interpExpr ? evaluateInterpExpr(interpExpr, macros) : '';
    return <span className="tomet-interp">{evaluatedText}</span>;
  }

  // 3. Render children nodes if present
  const innerChildren: ReactNode = (
    <InlineRenderer inlines={element.content} components={components} macros={macros} />
  );

  // 3. User-defined custom component override for this tag name
  const CustomComponent = components?.[name] || components?.[normalizedName];
  if (CustomComponent) {
    return (
      <CustomComponent
        name={name}
        sigil={element.sigil}
        args={element.args}
        content={element.content}
        value={element.value}
        rawElement={element}
        className={`tomet-element tomet-${normalizedName}`}
      >
        {innerChildren}
      </CustomComponent>
    );
  }

  // 4. Heading elements (<heading>, @heading, <h1>..<h6>)
  const headingLevel = getHeadingLevel(element, name);
  if (headingLevel !== null) {
    const HeadingComp = components?.heading;
    const headingId =
      element.args && typeof element.args === 'object' && !Array.isArray(element.args) && typeof (element.args as any).id === 'string'
        ? (element.args as any).id
        : undefined;

    if (HeadingComp) {
      return (
        <HeadingComp level={headingLevel} id={headingId} className={`tomet-h${headingLevel}`}>
          {innerChildren}
        </HeadingComp>
      );
    }
    return createElement(
      `h${headingLevel}`,
      { id: headingId, className: `tomet-heading tomet-h${headingLevel}` },
      innerChildren
    );
  }

  // 5. Explicit Link elements (@link(...)[text] or <link>(...)[text])
  if (normalizedName === 'link') {
    const href = extractLinkHref(element.args);
    const LinkComp = components?.link;
    if (LinkComp) {
      return (
        <LinkComp href={href} args={element.args} className="tomet-link">
          {innerChildren}
        </LinkComp>
      );
    }
    return (
      <a href={href} className="tomet-link">
        {innerChildren}
      </a>
    );
  }

  // 6. Task elements (<task>(done: true)[title] or args.done)
  const isTask =
    normalizedName === 'task' ||
    (element.args && typeof element.args === 'object' && !Array.isArray(element.args) && 'done' in (element.args as any));

  if (isTask) {
    const isDone = Boolean(
      element.args && typeof element.args === 'object' && !Array.isArray(element.args)
        ? (element.args as any).done
        : false
    );
    const TaskComp = components?.task;
    if (TaskComp) {
      return (
        <TaskComp done={isDone} args={element.args} className="tomet-task">
          {innerChildren}
        </TaskComp>
      );
    }
    return (
      <div className={`tomet-task ${isDone ? 'tomet-task-done' : 'tomet-task-todo'}`}>
        <input type="checkbox" checked={isDone} readOnly className="tomet-task-checkbox" />
        <span className="tomet-task-label">{innerChildren}</span>
      </div>
    );
  }

  // 7. Callout elements (<callout>, <note>, <tip>, <warning>, <caution>, <info>)
  if (
    normalizedName === 'callout' ||
    ['note', 'tip', 'warning', 'caution', 'info'].includes(normalizedName)
  ) {
    const calloutType =
      normalizedName === 'callout'
        ? (element.args && typeof element.args === 'object' && !Array.isArray(element.args) && typeof (element.args as any).type === 'string'
            ? (element.args as any).type
            : 'note')
        : normalizedName;

    const CalloutComp = components?.callout;
    if (CalloutComp) {
      return (
        <CalloutComp type={calloutType} args={element.args} className={`tomet-callout tomet-callout-${calloutType}`}>
          {innerChildren}
        </CalloutComp>
      );
    }
    return (
      <aside className={`tomet-callout tomet-callout-${calloutType}`}>
        {innerChildren}
      </aside>
    );
  }

  // 8. CodeBlock elements (<codeblock>)
  if (normalizedName === 'codeblock') {
    const lang =
      element.args && typeof element.args === 'object' && !Array.isArray(element.args) && typeof (element.args as any).lang === 'string'
        ? (element.args as any).lang
        : typeof element.args === 'string'
        ? element.args
        : undefined;

    const rawCode = extractTextFromInlines(element.content);
    const CodeBlockComp = components?.codeblock;
    if (CodeBlockComp) {
      return (
        <CodeBlockComp language={lang} content={rawCode} className="tomet-codeblock">
          {innerChildren}
        </CodeBlockComp>
      );
    }
    return (
      <pre className={`tomet-codeblock ${lang ? `language-${lang}` : ''}`}>
        <code>{rawCode}</code>
      </pre>
    );
  }

  // 9. Inline formatting (em, strong, mark)
  if (normalizedName === 'em') {
    const EmComp = components?.em;
    return EmComp ? <EmComp className="tomet-em">{innerChildren}</EmComp> : <em className="tomet-em">{innerChildren}</em>;
  }
  if (normalizedName === 'strong') {
    const StrongComp = components?.strong;
    return StrongComp ? (
      <StrongComp className="tomet-strong">{innerChildren}</StrongComp>
    ) : (
      <strong className="tomet-strong">{innerChildren}</strong>
    );
  }
  if (normalizedName === 'mark') {
    const MarkComp = components?.mark;
    return MarkComp ? <MarkComp className="tomet-mark">{innerChildren}</MarkComp> : <mark className="tomet-mark">{innerChildren}</mark>;
  }

  // 10. Horizontal break (<hr>)
  if (normalizedName === 'hr') {
    const HrComp = components?.hr;
    return HrComp ? <HrComp title={innerChildren} className="tomet-hr" /> : <hr className="tomet-hr" />;
  }

  // 11. Blockquote (<blockquote>)
  if (normalizedName === 'blockquote') {
    const BqComp = components?.blockquote;
    return BqComp ? (
      <BqComp className="tomet-blockquote">{innerChildren}</BqComp>
    ) : (
      <blockquote className="tomet-blockquote">{innerChildren}</blockquote>
    );
  }

  // 12. Lists (ul, ol)
  if (normalizedName === 'ul' || normalizedName === 'ol') {
    const isOrdered = normalizedName === 'ol';
    const childrenElements = getElementChildren(element.value);
    const ListComp = components?.list;
    const ListItemComp = components?.listItem;

    const listItemsContent = childrenElements?.map((itemEl, idx) => (
      <li key={idx} className="tomet-list-item">
        {ListItemComp ? (
          <ListItemComp args={itemEl.args} className="tomet-list-item-inner">
            <InlineRenderer inlines={itemEl.content} components={components} macros={macros} />
          </ListItemComp>
        ) : (
          <InlineRenderer inlines={itemEl.content} components={components} macros={macros} />
        )}
      </li>
    ));

    if (ListComp) {
      return (
        <ListComp ordered={isOrdered} className={`tomet-list tomet-${normalizedName}`}>
          {listItemsContent}
        </ListComp>
      );
    }
    return isOrdered ? (
      <ol className="tomet-list tomet-ol">{listItemsContent}</ol>
    ) : (
      <ul className="tomet-list tomet-ul">{listItemsContent}</ul>
    );
  }

  // 13. Nested child blocks if element has children
  const nestedBlocks = element.children ? (
    <div className="tomet-element-children">
      {element.children.map((childBlock, idx) => (
        <BlockRenderer key={idx} block={childBlock} components={components} macros={macros} />
      ))}
    </div>
  ) : null;

  // 14. Generic Fallback
  if (isInline) {
    return (
      <span className={`tomet-element tomet-${normalizedName}`} data-tomet-tag={normalizedName}>
        {innerChildren}
      </span>
    );
  }

  return (
    <div className={`tomet-element tomet-${normalizedName}`} data-tomet-tag={normalizedName}>
      {innerChildren}
      {nestedBlocks}
    </div>
  );
};
