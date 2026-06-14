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
