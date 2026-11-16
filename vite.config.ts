import { defineConfig } from 'vite';

// Frontend lezy w ui/, wynik budowy w dist/ (stamtad bierze go Tauri).
export default defineConfig({
  root: 'ui',
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: { outDir: '../dist', emptyOutDir: true, target: 'es2022' },
});
