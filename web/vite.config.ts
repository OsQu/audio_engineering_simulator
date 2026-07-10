import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig, type Plugin } from "vite";

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

// The Story-6.4 hot loop: `pnpm wasm:watch` rebuilds the wasm/worklet artifacts into public/ on any Rust
// save. Those live outside the module graph, so an HMR update never fires for them — this plugin watches
// them and forces a **full page reload** when either lands, so the bench restarts on the fresh engine
// (then restores itself from the URL, pins included, and needs one click to sound — the autoplay resume).
function reloadOnWasmArtifact(): Plugin {
  const isArtifact = (file: string): boolean =>
    file.endsWith("wasm_bindings_bg.wasm") || file.endsWith("processor.js");
  return {
    name: "reload-on-wasm-artifact",
    apply: "serve",
    configureServer(server) {
      if (server.config.publicDir) server.watcher.add(server.config.publicDir);
      const reload = (file: string): void => {
        if (!isArtifact(file)) return;
        server.config.logger.info("wasm artifact rebuilt — full reload", { timestamp: true });
        server.ws.send({ type: "full-reload" });
      };
      server.watcher.on("add", reload);
      server.watcher.on("change", reload);
    },
  };
}

export default defineConfig({
  plugins: [svelte(), reloadOnWasmArtifact()],
  server: { port: 5173 },
});
