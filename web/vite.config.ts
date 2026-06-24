import { defineConfig } from "vite";

// Story 3.2, Phase B — the dev/test harness. Per engine-before-UI, the *page* is throwaway but this
// build/serve infrastructure carries into Epic 4's real UI.
//
// The worklet (public/processor.js) and wasm (public/wasm_bindings_bg.wasm) are static assets built
// by `npm run wasm` (build-wasm.sh) and served from the web root, so Vite does NOT process the
// classic-script worklet — which must stay a self-contained no-modules script (AudioWorkletGlobalScope
// has no import/importScripts). main.ts is the only bundled entry.
//
// COOP/COEP are intentionally NOT set yet: they're only needed for the SharedArrayBuffer event ring
// in Story 3.4, and are added then via `server.headers` + `preview.headers`.
export default defineConfig({
  server: { port: 5173 },
});
