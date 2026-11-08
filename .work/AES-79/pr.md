Record the wasm-pack 0.15.1 upgrade (host tool, no code change)

`pnpm run wasm` warned that a newer wasm-pack (0.15.1) was available. The repo doesn't pin wasm-pack:
it's a host Homebrew install locally, and CI's `taiki-e/install-action` already installs the latest
version. The fix was `brew upgrade wasm-pack` on the dev machine. With 0.15.1, `pnpm run wasm` builds
cleanly and prints no warning.

The branch only adds the working notes. Nothing to review in the source.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
