Show a ⚠ placeholder when a config widget's key is missing

## What & why
`ConfigSwitch`, `ConfigButton` and `ConfigSelect` rendered **nothing** when their `key` didn't resolve against the
device's config descriptors (`{#if desc}`). A typo'd key or a stale WASM artifact then showed up as an invisible
zero-width element. That's how the 8i6's 48V button once "disappeared" until the wasm was rebuilt.

Now:
- `resolveConfigWidget(handle, key, kind?)` (`web/src/device-handle.ts`) returns either the descriptor or a `problem`
  message that names the device and key, and hints at a stale WASM build. `ConfigSelect` passes `kind: "select"`, so
  a key that points at a toggle is reported as a mismatch instead of silently rendering nothing.
- A new `MissingConfig.svelte` renders a small dashed red `⚠ <key>` chip in the widget's place, with the full message
  as its `title`, and logs it once with `console.warn`.

## Look closely at
- **Not dev-gated.** The web tsconfig has `types: []`, so `import.meta.env.DEV` doesn't typecheck. I chose not to add
  `vite/client` for this. A missing key is a bug in any build, and a visible chip is better than an invisible element.
  If you'd rather hide it in production, adding `"vite/client"` to `types` and wrapping it in
  `{:else if import.meta.env.DEV}` is a one-line follow-up.
- Vitest runs in node and can't mount Svelte, so the tests (`web/test/config-widget.test.ts`) cover the pure resolver:
  hit, missing key, device with no configs, and kind mismatch.

## Testing
`pnpm run format && pnpm run check && pnpm run typecheck && pnpm run test`: all green.

There was no live browser check. In a headless session the `/devices/<typeId>` bench drew the Synth Voice face for
every device type, with or without this change, so I couldn't get the 8i6 faceplate on screen. That might be worth a
quick look on its own.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
