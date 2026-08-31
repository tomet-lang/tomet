import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

import path from 'path';
import fs from 'fs';

const localPkg = path.resolve(__dirname, '../../../packages/svelte/src/index.ts');
const vendoredPkg = path.resolve(__dirname, 'packages-svelte/src/index.ts');
const svelteEntry = fs.existsSync(localPkg) ? localPkg : vendoredPkg;

const localWasm = path.resolve(__dirname, '../../../bindings/js/pkg/tomet_js.js');
const vendoredWasm = path.resolve(__dirname, 'bindings-js/tomet_js.js');
const wasmEntry = fs.existsSync(localWasm) ? localWasm : vendoredWasm;

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: {
      '@tomet/svelte': svelteEntry,
      '@tomet/wasm': wasmEntry,
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: 'app.js',
        chunkFileNames: 'app.js',
        assetFileNames: (assetInfo) => {
          if (assetInfo.name && assetInfo.name.endsWith('.css')) {
            return 'style.css';
          }
          return '[name].[ext]';
        },
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8787',
    },
  },
});
