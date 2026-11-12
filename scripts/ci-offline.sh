#!/usr/bin/env bash
# Run a command with no network but loopback (Linux CI only): a fresh network
# namespace, `lo` up, then back to the calling user with the same PATH, HOME
# and environment. Used by release-candidate.yml so that a test reaching for
# the public internet (DNS, a live chain RPC, a third-party API) fails every
# time instead of passing or flaking with the weather.
#
#   scripts/ci-offline.sh cargo test --workspace --locked --offline
set -euo pipefail
[ "$(uname -s)" = Linux ] || { echo "ci-offline.sh needs Linux network namespaces" >&2; exit 2; }
export CI_OFFLINE_USER="$(id -un)" CI_OFFLINE_PATH="$PATH" CI_OFFLINE_HOME="$HOME"
exec sudo --preserve-env unshare --net -- bash -c '
  ip link set lo up
  exec sudo --preserve-env -u "$CI_OFFLINE_USER" env "PATH=$CI_OFFLINE_PATH" "HOME=$CI_OFFLINE_HOME" "$@"
' ci-offline "$@"
