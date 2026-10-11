import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// Tauri drives this dev server; the port must match tauri.conf.json devUrl.
const DEV_PORT = 1420;

export default defineConfig(({ command }) => ({
  plugins: [
    svelte({
      // On a cold dev start, vite-plugin-svelte intermittently fails to serve a
      // component's virtual CSS module ("failed to load virtual css module"),
      // and that component renders unstyled. Injecting the CSS through the JS
      // module removes the separate request and the race with it. Production
      // still emits a real stylesheet.
      emitCss: command === 'build',
    }),
  ],
  resolve: {
    alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) },
    // Under vitest, Svelte must resolve to its client build: the server build
    // has no `mount`, so rendering a component in jsdom throws.
    ...(process.env.VITEST ? { conditions: ['browser'] } : {}),
  },
  // Tauri expects a fixed port and should fail loudly rather than pick another.
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/target/**'] },
  },
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
}));
