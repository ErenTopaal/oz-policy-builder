import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // happy-dom is lighter/faster than jsdom; Freighter API guards on `window`,
    // so we need *some* DOM-like global. happy-dom suffices.
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
    // mocked tests only in default config. Integration tests are gated
    // by the INTEGRATION=1 env var (see package.json test:integration).
    testTimeout: 10_000,
    clearMocks: true,
    mockReset: true,
    restoreMocks: true,
  },
});
