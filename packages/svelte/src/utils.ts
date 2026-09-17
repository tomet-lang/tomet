import type { Element, ElementValue, Inline, Value } from './types.js';

/**
 * Extracts string name and kind information from an Element sigil.
 */
export function getElementKind(el: Element): { name: string; isAt: boolean; isType: boolean; isBare: boolean; isDollar: boolean } {
  const sigil = el.sigil;
  if (!sigil) {
    return { name: el.name || '', isAt: false, isType: false, isBare: true, isDollar: false };
  }

  if (typeof sigil === 'string') {
    if (sigil === 'Bare') return { name: el.name || '', isAt: false, isType: false, isBare: true, isDollar: false };
    if (sigil === 'Dollar') return { name: '$', isAt: false, isType: false, isBare: false, isDollar: true };
    return { name: sigil, isAt: false, isType: true, isBare: false, isDollar: false };
  }

  if ('Type' in sigil) {
    return { name: sigil.Type, isAt: false, isType: true, isBare: false, isDollar: false };
  }

  if ('At' in sigil) {
    return { name: sigil.At || 'at', isAt: true, isType: false, isBare: false, isDollar: false };
  }

  return { name: el.name || '', isAt: false, isType: false, isBare: true, isDollar: false };
}

/**
 * Checks whether an element represents an invisible document directive.
 */
export function isDirective(name: string): boolean {
  return ['version', 'kind', 'meta', 'config'].includes(name.toLowerCase());
}

/**
 * Checks whether an element represents a heading and determines its level (1-6).
 */
export function getHeadingLevel(el: Element, name: string): number | null {
  const lower = name.toLowerCase();
  if (lower === 'heading') {
    if (typeof el.args === 'number') return Math.min(Math.max(el.args, 1), 6);
    if (el.args && typeof el.args === 'object' && !Array.isArray(el.args)) {
      const lvl = (el.args as Record<string, Value>).level;
      if (typeof lvl === 'number') return Math.min(Math.max(lvl, 1), 6);
    }
    return 1;
  }
  const match = /^h([1-6])$/i.exec(lower);
  if (match) {
    return parseInt(match[1], 10);
  }
  return null;
}

/**
 * Extracts target URL/href from element arguments for explicit links (@link, <link>).
 */
export function extractLinkHref(args: Value | null | undefined): string {
  if (args == null) return '';
  if (typeof args === 'string') return args;
  if (typeof args === 'object' && !Array.isArray(args)) {
    const map = args as Record<string, Value>;
    if (typeof map.url === 'string') return map.url;
    if (typeof map.target === 'string') return map.target;
    if (typeof map.href === 'string') return map.href;
    if (typeof map.file === 'string') return map.file;
    if (typeof map.tm === 'string') return map.tm;
    if (typeof map.id === 'string') return `#link-${map.id}`;
    if (typeof map.ref === 'string') return `#ref-${map.ref}`;
  }
  return '';
}

/**
 * Extracts plain text from an array of Inline nodes.
 */
export function extractTextFromInlines(inlines: Inline[] | null | undefined): string {
  if (!inlines || !Array.isArray(inlines)) return '';
  let out = '';
  for (const inline of inlines) {
    if ('Text' in inline) {
      out += inline.Text.value;
    } else if ('type' in inline && inline.type === 'Text') {
      out += inline.value;
    } else if ('Element' in inline) {
      out += extractTextFromInlines(inline.Element.content);
    } else if ('type' in inline && inline.type === 'Element' && inline.data) {
      out += extractTextFromInlines(inline.data.content);
    }
  }
  return out;
}

/**
 * Helper to safely extract child Elements from ElementValue.
 */
export function getElementChildren(value: ElementValue | null | undefined): Element[] | null {
  if (!value) return null;
  if ('Children' in value) return value.Children;
  if ('type' in value && value.type === 'Children') return value.data;
  return null;
}

/**
 * Recursively or shallowly converts JS Map, array of pairs, or plain Object into a Record<string, any>.
 */
export function unwrapMap(val: any): Record<string, any> {
  const result: Record<string, any> = {};
  if (!val) return result;

  if (val instanceof Map) {
    for (const [k, v] of val.entries()) {
      result[String(k)] = v;
    }
  } else if (Array.isArray(val)) {
    for (const item of val) {
      if (Array.isArray(item) && item.length >= 2) {
        result[String(item[0])] = item[1];
      }
    }
  } else if (typeof val === 'object') {
    for (const [k, v] of Object.entries(val)) {
      result[k] = v;
    }
  }

  return result;
}

/**
 * Extracts macro templates declared in `@config` blocks in the document.
 */
export function extractDocumentMacros(doc: any): Record<string, string> {
  const macros: Record<string, string> = {};
  if (!doc || !doc.blocks || !Array.isArray(doc.blocks)) return macros;

  for (const block of doc.blocks) {
    const el = 'Element' in block ? block.Element : 'type' in block && block.type === 'Element' ? block.data : null;
    if (!el) continue;

    const kindInfo = getElementKind(el);
    if (kindInfo.name.toLowerCase() === 'config') {
      let dataMap: any = null;
      if (el.value) {
        if ('Data' in el.value && el.value.Data) {
          dataMap = el.value.Data;
        } else if ('type' in el.value && el.value.type === 'Data') {
          dataMap = el.value.data;
        }
      }
      if (!dataMap && el.args) {
        dataMap = el.args;
      }

      if (dataMap) {
        const top = unwrapMap(dataMap);
        const macroEntries = top.macros || top.macro;
        if (macroEntries) {
          const macroMap = unwrapMap(macroEntries);
          for (const [k, v] of Object.entries(macroMap)) {
            if (typeof v === 'string') {
              macros[k] = v;
            } else if (v && typeof v === 'object') {
              if ('String' in v) {
                macros[k] = (v as any).String;
              } else {
                macros[k] = String(v);
              }
            } else if (v != null) {
              macros[k] = String(v);
            }
          }
        }
      }
    }
  }

  return macros;
}

/**
 * Evaluates an InterpExpr AST node into a string using the provided macros map.
 */
export function evaluateInterpExpr(expr: any, macros?: Record<string, string>): string {
  if (!expr) return '';
  const kind = expr.kind || expr;

  if (typeof kind === 'object') {
    if ('Literal' in kind) {
      const lit = kind.Literal;
      if (lit == null) return '';
      if (typeof lit === 'object') {
        if ('String' in lit) return lit.String;
        if ('Int' in lit) return String(lit.Int);
        if ('Float' in lit) return String(lit.Float);
      }
      return String(lit);
    }

    if ('Identifier' in kind) {
      const id = kind.Identifier;
      if (macros && id in macros) {
        return macros[id];
      }
      return id;
    }

    if ('Member' in kind) {
      const obj = evaluateInterpExpr(kind.Member.object, macros);
      return `${obj}.${kind.Member.member}`;
    }

    if ('NamedArg' in kind) {
      return evaluateInterpExpr(kind.NamedArg.value, macros);
    }

    if ('Call' in kind) {
      const callee = kind.Call.callee;
      const args = kind.Call.args || [];
      let calleeName = '';
      if (typeof callee === 'string') {
        calleeName = callee;
      } else if (callee && typeof callee === 'object') {
        const cKind = callee.kind || callee;
        if (typeof cKind === 'string') {
          calleeName = cKind;
        } else if (cKind && typeof cKind === 'object' && 'Identifier' in cKind) {
          calleeName = cKind.Identifier;
        }
      }

      const evaluatedArgs = args.map((a: any) => evaluateInterpExpr(a, macros));

      if (calleeName === 'add') {
        const nums: number[] = evaluatedArgs.map(Number);
        return String(nums.reduce((a: number, b: number) => a + b, 0));
      }
      if (calleeName === 'sub') {
        const [a, b] = evaluatedArgs.map(Number);
        return String(a - b);
      }
      if (calleeName === 'mul') {
        const [a, b] = evaluatedArgs.map(Number);
        return String(a * b);
      }
      if (calleeName === 'div') {
        const [a, b] = evaluatedArgs.map(Number);
        return b !== 0 ? String(a / b) : '0';
      }
      if (calleeName === 'unicode') {
        try {
          const code = parseInt(evaluatedArgs[0], 16);
          return String.fromCodePoint(code);
        } catch {
          return evaluatedArgs[0] || '';
        }
      }
      if (calleeName === 'emoji') {
        const emojis: Record<string, string> = {
          sparkles: '✨',
          tada: '🎉',
          rocket: '🚀',
          fire: '🔥',
          heart: '❤️',
          check: '✅',
        };
        return emojis[evaluatedArgs[0]] || evaluatedArgs[0] || '';
      }

      if (macros && calleeName in macros) {
        let tmpl = macros[calleeName];
        // Positional placeholders: ${1}, ${2}, etc.
        for (let i = 0; i < evaluatedArgs.length; i++) {
          tmpl = tmpl.replaceAll(`\${${i + 1}}`, evaluatedArgs[i]);
        }
        // Named placeholders
        for (const arg of args) {
          const aKind = arg?.kind || arg;
          if (aKind && typeof aKind === 'object' && 'NamedArg' in aKind) {
            const { name, value } = aKind.NamedArg;
            const valStr = evaluateInterpExpr(value, macros);
            tmpl = tmpl.replaceAll(`\${${name}}`, valStr);
          }
        }
        return tmpl;
      }

      return `${calleeName}(${evaluatedArgs.join(', ')})`;
    }
  }

  return '';
}
