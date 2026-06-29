import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// Svelte 5. `vitePreprocess` lets <script lang="ts"> blocks use TypeScript (transpiled by Vite's
// esbuild); no runtime config beyond that — the harness is a single-page mount (see main.ts).
export default {
  preprocess: vitePreprocess(),
};
