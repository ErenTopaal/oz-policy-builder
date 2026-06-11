// lazy import wrapper for the `monaco-editor` package. The whole point of
// this file is to keep Monaco — a ~1MB editor bundle — OUT of the landing
// page chunk. Vite's code-splitter only emits a separate chunk when the
// import is dynamic (i.e. `import('...')` at runtime), so we centralise the
// dynamic import here and hand callers a memoised promise.
//
// `@monaco-editor/react`'s `<Editor>` component already lazy-loads Monaco
// from the CDN by default, but that path makes a network request to JSDelivr
// at runtime, which is the wrong story for our deploy. Instead, we feed the
// react wrapper a `loader` configured to use this bundled instance, so the
// Monaco chunk ships in our own dist and works offline.
//
// callers: SourceTab.tsx (mounted on demand when the user clicks "Source").

import type * as monacoNs from "monaco-editor";

export type MonacoModule = typeof monacoNs;

let cached: Promise<MonacoModule> | null = null;

/**
 * Lazy-load the bundled `monaco-editor` package. The returned promise is
 * memoised — subsequent calls share the same chunk fetch.
 *
 * Vite turns the `import('monaco-editor')` below into a separate async
 * chunk (look for `monaco-*.js` in `dist/assets/`). Verifying that chunk
 * exists is part of the build-time check in our CI script.
 */
export function loadMonaco(): Promise<MonacoModule> {
  if (cached === null) {
    cached = import("monaco-editor");
  }
  return cached;
}

/**
 * Configure `@monaco-editor/react` to use the bundled Monaco instance
 * rather than its default CDN loader. Idempotent — safe to call on every
 * SourceTab mount. Returns the same promise as `loadMonaco`.
 */
export async function ensureMonacoReactBundled(): Promise<MonacoModule> {
  const monaco = await loadMonaco();
  const { loader } = await import("@monaco-editor/react");
  loader.config({ monaco });
  return monaco;
}
