import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// The dev/test harness: the *page* is throwaway, but this build/serve infrastructure carries into
// the real UI. From Story 4.2 the UI is **Svelte 5** (runes), mounted into #app from main.ts.
//
// The worklet (public/processor.js) and wasm (public/wasm_bindings_bg.wasm) are static assets built
// by `npm run wasm` (build-wasm.sh) and served from the web root, so Vite does NOT process the
// classic-script worklet — which must stay a self-contained no-modules script (AudioWorkletGlobalScope
// has no import/importScripts). main.ts is the only bundled entry.
//
// COOP/COEP are intentionally NOT set: they're only needed for a SharedArrayBuffer-based event ring,
// which isn't built; if it ever lands, add them via `server.headers` + `preview.headers`.
export default defineConfig({
  plugins: [svelte()],
  server: { port: 5173 },
});
