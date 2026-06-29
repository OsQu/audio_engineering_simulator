import { defineConfig } from "vite";

// The dev/test harness: the *page* is throwaway, but this build/serve infrastructure carries into
// the real UI.
//
// The worklet (public/processor.js) and wasm (public/wasm_bindings_bg.wasm) are static assets built
// by `npm run wasm` (build-wasm.sh) and served from the web root, so Vite does NOT process the
// classic-script worklet — which must stay a self-contained no-modules script (AudioWorkletGlobalScope
// has no import/importScripts). main.ts is the only bundled entry.
//
// COOP/COEP are intentionally NOT set: they're only needed for a SharedArrayBuffer-based event ring,
// which isn't built; if it ever lands, add them via `server.headers` + `preview.headers`.
export default defineConfig({
  server: { port: 5173 },
});
