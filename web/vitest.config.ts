import { defineConfig } from "vitest/config";

// Vitest runs the pure spatial/logic modules in isolation — no Svelte, no DOM. Tests live in test/,
// outside the svelte-check `src` typecheck path, and import what they need explicitly (no globals),
// so the test runner is fully decoupled from the Vite/Svelte app build.
export default defineConfig({
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
  },
});
