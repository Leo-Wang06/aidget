import { defineConfig } from 'vite-plus';

// Tooling lives at the repository root; application builds keep ui/vite.config.ts.
export default defineConfig({
  fmt: {
    singleQuote: true,
    sortImports: {},
    printWidth: 100,
    ignorePatterns: [
      'target/**',
      'gen/**',
      'ui/dist/**',
      'ui/src/types/rust.ts',
      'widgets/**',
      'README*.md',
    ],
  },
  lint: {
    ignorePatterns: ['ui/dist/**', 'ui/src/types/rust.ts'],
    options: { typeAware: true, typeCheck: true },
    jsPlugins: [{ name: 'vite-plus', specifier: 'vite-plus/oxlint-plugin' }],
    rules: { curly: 'error', 'vite-plus/prefer-vite-plus-imports': 'error' },
  },
});
