/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// vitest reads the `test` field below — typed via the vitest triple-slash
// reference above. defineConfig is imported from vite (not vitest/config)
// because vitest's re-exported defineConfig is pinned to an older vite
// major and clashes with our vite 8 plugin types.
export default defineConfig({
  plugins: [react()],
  // @ts-expect-error vitest augments via /// reference; vite 8 types omit it
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
  },
})
