/**
 * Astro integration for Tomet.
 *
 * This is a skeleton: the integration registers itself and does nothing
 * else. The content layer it is meant to provide -- document discovery,
 * metadata, slugs, the link graph -- is blocked on the workspace layer
 * being reachable from `@tomet/tomet-wasm`. See the README's
 * "Prerequisites" before adding to this file.
 *
 * Note what is deliberately absent: any declaration of the Tomet AST.
 * Those types come from the binding once it can be depended on. The
 * README explains why a local copy is not an option.
 */
import type { AstroIntegration } from 'astro';

/** Options accepted by the integration. Empty until there is a content layer. */
export interface TometOptions {}

/**
 * Registers the Tomet content layer with Astro.
 *
 * ```js
 * // astro.config.mjs
 * import tomet from '@tomet/astro';
 * export default defineConfig({ integrations: [tomet()] });
 * ```
 */
export default function tomet(_options: TometOptions = {}): AstroIntegration {
  return {
    name: '@tomet/astro',
    hooks: {},
  };
}
