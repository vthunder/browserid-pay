#!/usr/bin/env bash
# The fast local equivalent of .github/workflows/release-candidate.yml: what
# must be green before a commit is pushed to the staging mirror and submitted
# for approval. The mirror's run of that workflow is what qualifies a
# submission (sandmill-infra/ip-approval/PLAN.md); this is the quick loop
# before it. Prints one line per step and exits non-zero on any failure.
#
#   make release-check                  # through ssh localtest (Gatekeeper)
#   RELEASE_CHECK_SKIP="supply-chain" scripts/release-check.sh   # dev only
#
# It tests the committed HEAD, not the working tree: HEAD is exported to a
# scratch directory without .cargo/config.toml (the sibling browserid-ng
# patch, which .dockerignore also keeps out of the images), so cargo builds
# the pinned git source with the committed Cargo.lock under --locked, with
# the rust version the Dockerfiles pin. It therefore refuses a dirty tree,
# before and after the run. Not run here: the image builds and the offline
# network namespace, which need Linux; CI runs both.
#
# Needs: rustup with the pinned toolchain (installed on demand), node + npm,
# cargo-audit, gitleaks.
set -uo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
LOGDIR="${RELEASE_CHECK_LOGDIR:-${TMPDIR:-/tmp}/release-check-pay}"
SRC="$LOGDIR/src"
SKIP=" ${RELEASE_CHECK_SKIP:-} "
declare -a RESULTS=()
FAILED=0

if [ -n "$(git status --porcelain)" ]; then
  echo "release-check: the tree is dirty; commit or stash first (it tests HEAD):"
  git status --short | sed 's/^/  /'
  exit 1
fi

mkdir -p "$LOGDIR"
rm -rf "$SRC" && mkdir -p "$SRC"
git archive HEAD | tar -x -C "$SRC"
rm -f "$SRC/.cargo/config.toml"

step() {
  local name="$1"; shift
  if [[ "$SKIP" == *" $name "* ]]; then RESULTS+=("skip  $name"); return 0; fi
  local log="$LOGDIR/$name.log"
  printf '==> %s\n' "$name"
  if "$@" >"$log" 2>&1; then
    RESULTS+=("ok    $name")
  else
    RESULTS+=("FAIL  $name  (log: $log)")
    FAILED=1
    tail -40 "$log" | sed 's/\x1b\[[0-9;]*m//g'
    return 1
  fi
}

# The rust version every Dockerfile pins (by digest), and the one CI pins.
TC=""
toolchain_pin() {
  local vs n
  vs="$(sed -nE 's/^FROM rust:([0-9.]+)-[a-z]+@sha256:[0-9a-f]{64} .*/\1/p' crates/*/Dockerfile | sort -u)"
  n="$(cat crates/*/Dockerfile | grep -c '^FROM rust:')"
  [ "$(cat crates/*/Dockerfile | grep -cE '^FROM rust:[0-9.]+-[a-z]+@sha256:[0-9a-f]{64} ')" = "$n" ] || { echo "every FROM rust: must be a version pinned by digest"; return 1; }
  [ "$(echo "$vs" | wc -l | tr -d ' ')" = 1 ] || { echo "Dockerfiles disagree on the rust version: $vs"; return 1; }
  grep -q "rust-toolchain@[0-9a-f]\{40\} # $vs\." .github/workflows/release-candidate.yml || { echo "release-candidate.yml pins a different toolchain than the Dockerfiles ($vs)"; return 1; }
  rustup toolchain list | grep -qE "^$vs(\.[0-9]+)?-" || rustup toolchain install "$vs" --profile minimal
  TC="$(rustup toolchain list | grep -oE "^$vs(\.[0-9]+)?-" | head -1 | sed 's/-$//')"
  [ -n "$TC" ] || { echo "toolchain $vs not installed"; return 1; }
  echo "pinned toolchain: $TC ($(cargo "+$TC" --version))"
}
# Run in a subshell so that the main shell keeps TC.
run_cargo() { (cd "$SRC" && cargo "+$TC" "$@"); }
rust_workspace() { run_cargo test --workspace --locked; }
sdk_agreements() { (cd "$SRC/sdk/agreements" && npm ci --ignore-scripts --no-audit --no-fund && node --test); }
agreements_mcp() { node --check "$SRC/sdk/agreements-mcp/server.mjs"; }
e2e() {
  run_cargo build --locked -p custodian -p judge -p demo-idp -p indexer || return 1
  local target
  target="$(cd "$SRC" && cargo "+$TC" metadata --format-version 1 --no-deps | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
  (cd "$SRC/demo" && npm ci --ignore-scripts --no-audit --no-fund) || return 1
  (cd "$SRC/demo" && CARGO_TARGET_DIR="$target" node run-local.mjs)
}
supply_chain() { scripts/supply-chain-audit.sh all; }

if step toolchain-pin toolchain_pin; then
  step rust-workspace  rust_workspace
  step e2e             e2e
else
  RESULTS+=("FAIL  rust-workspace  (no pinned toolchain)" "FAIL  e2e  (no pinned toolchain)")
fi
step sdk-agreements  sdk_agreements
step agreements-mcp  agreements_mcp
step supply-chain    supply_chain

echo
echo "release-check summary ($(git rev-parse --short HEAD)):"
printf '  %s\n' "${RESULTS[@]}"
if [ -z "$(git status --porcelain)" ]; then
  echo "  tree clean after the run"
else
  echo "  FAIL  the run left the tree dirty:"; git status --short | sed 's/^/    /'; FAILED=1
fi
exit $FAILED
