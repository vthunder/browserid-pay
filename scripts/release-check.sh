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
# curl + jq (registry read), cargo-audit, gitleaks.
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

# The toolchain must be exactly the one the Dockerfiles' pinned rust image
# carries: tag == the image's RUST_VERSION (read from the registry for
# linux/amd64, no docker needed) == the version release-candidate.yml pins.
TC=""
image_rust_version() {
  local ref="$1" repo dig tok m c A
  repo="library/${ref%%:*}"; dig="${ref#*@}"
  A="Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json"
  tok="$(curl -fsS "https://auth.docker.io/token?service=registry.docker.io&scope=repository:$repo:pull" | jq -r .token)" || return 1
  m="$(curl -fsS -H "Authorization: Bearer $tok" -H "$A" "https://registry-1.docker.io/v2/$repo/manifests/$dig" \
    | jq -r 'if .manifests then (.manifests[] | select(.platform.os=="linux" and .platform.architecture=="amd64") | .digest) else empty end')" || return 1
  [ -n "$m" ] || m="$dig"
  c="$(curl -fsS -H "Authorization: Bearer $tok" -H "$A" "https://registry-1.docker.io/v2/$repo/manifests/$m" | jq -r .config.digest)" || return 1
  curl -fsSL -H "Authorization: Bearer $tok" "https://registry-1.docker.io/v2/$repo/blobs/$c" | jq -r '.config.Env[]' | sed -n 's/^RUST_VERSION=//p'
}
toolchain_pin() {
  local refs ref tag img ci n
  refs="$(sed -nE 's/^FROM (rust:[^ ]+) AS builder.*/\1/p' crates/*/Dockerfile | sort -u)"
  n="$(cat crates/*/Dockerfile | grep -c '^FROM rust:')"
  [ "$(cat crates/*/Dockerfile | grep -cE '^FROM rust:[0-9]+\.[0-9]+\.[0-9]+-[a-z]+@sha256:[0-9a-f]{64} AS builder')" = "$n" ] || { echo "every FROM rust: must be rust:<x.y.z>-<suite>@sha256:<digest> AS builder"; return 1; }
  [ "$(echo "$refs" | wc -l | tr -d ' ')" = 1 ] || { echo "Dockerfiles disagree on the rust image: $refs"; return 1; }
  ref="$refs"
  tag="$(sed -nE 's/^rust:([0-9.]+)-.*/\1/p' <<<"$ref")"
  img="$(image_rust_version "$ref")" || { echo "could not read RUST_VERSION of $ref from the registry"; return 1; }
  ci="$(grep -hoE 'dtolnay/rust-toolchain@[0-9a-f]{40} # [0-9.]+' .github/workflows/release-candidate.yml | awk '{print $3}' | sort -u)"
  echo "Dockerfile tag $tag, image carries $img, release-candidate.yml pins $ci"
  [ -n "$tag" ] && [ "$tag" = "$img" ] && [ "$img" = "$ci" ] || { echo "toolchain mismatch: tag $tag, image $img, CI $ci"; return 1; }
  rustup toolchain list | grep -q "^$tag-" || rustup toolchain install "$tag" --profile minimal
  TC="$tag"
  [ "$(cargo "+$TC" --version | awk '{print $2}')" = "$tag" ] || { echo "toolchain $tag not usable"; return 1; }
  echo "pinned toolchain: $(cargo "+$TC" --version)"
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
  # The demo links ../sdk/agreements, which resolves its own dependencies.
  local d; for d in sdk/agreements demo; do
    (cd "$SRC/$d" && npm ci --ignore-scripts --no-audit --no-fund) || return 1
  done
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
