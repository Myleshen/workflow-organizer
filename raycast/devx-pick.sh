#!/bin/zsh

# @raycast.schemaVersion 1
# @raycast.title Devx Pick
# @raycast.mode silent
# @raycast.packageName Devx
# @raycast.description Open the cached devx project picker in the configured terminal.

set -euo pipefail

devx_path="$HOME/scripts/devx"
if [[ ! -x "$devx_path" ]]; then
  print -u2 "devx is not installed at $devx_path"
  exit 1
fi

exec "$devx_path" raycast pick
