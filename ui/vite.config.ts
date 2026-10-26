import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vite-plus';

const root = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root,
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'safari16',
    rollupOptions: {
      input: {
        widget: resolve(root, 'widget.html'),
        main: resolve(root, 'main.html'),
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
});
