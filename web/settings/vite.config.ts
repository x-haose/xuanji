import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// 产物用相对路径，适配 xuanji://localhost/settings/dist/ 下的加载。
export default defineConfig({
  base: './',
  plugins: [svelte()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
});
