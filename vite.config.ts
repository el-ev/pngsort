import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    outDir: 'dist',
    target: 'es2020',
  },
  server: {
    fs: {
      allow: ['..']
    }
  },
  optimizeDeps: {
    exclude: ['./pkg/pngsort.js']
  },
  assetsInclude: ['**/*.wasm']
});
