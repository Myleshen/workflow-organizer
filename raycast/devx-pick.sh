#!/bin/zsh

# @raycast.schemaVersion 1
# @raycast.title Devx Pick
# @raycast.mode silent
# @raycast.packageName Devx
# @raycast.description Open the cached devx project picker in the configured terminal.

set -euo pipefail

if (( $+commands[devx] )); then
  devx_path="$(command -v devx)"
elif [[ -x "$HOME/.cargo/bin/devx" ]]; then
  devx_path="$HOME/.cargo/bin/devx"
else
  print -u2 "devx is not installed. Run: cargo install --path /path/to/devx"
  exit 1
fi

exec "$devx_path" raycast pick
