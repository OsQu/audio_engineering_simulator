#!/usr/bin/env bash
# Run the repo's pinned rust-code-analysis-cli (per-function complexity metrics).
#
# The binary lives in a version-specific directory shared by every checkout, so all worktrees
# measure with the same version and nothing global is changed. Install it once with:
#
#   scripts/rust-code-analysis.sh --install
#
# Any other arguments go to rust-code-analysis-cli, run from the repo root.
set -euo pipefail

version=0.0.25
root="${RCA_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/rust-code-analysis-cli/$version}"
bin="$root/bin/rust-code-analysis-cli"

cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--install" ]]; then
  # shellcheck source=/dev/null
  [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
  exec cargo install --locked --version "$version" --root "$root" rust-code-analysis-cli
fi

if [[ ! -x "$bin" ]]; then
  echo "rust-code-analysis-cli $version is not installed; run: scripts/rust-code-analysis.sh --install" >&2
  exit 1
fi
exec "$bin" "$@"
