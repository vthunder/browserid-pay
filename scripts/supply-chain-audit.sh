#!/usr/bin/env bash
# Dependency + secrets audit, the same gate locally and in CI (rc / audit).
#
#   scripts/supply-chain-audit.sh [rust|node|secrets|all]   (default all)
#
# Policy (one place, so CI and the local check cannot drift):
#   rust    `cargo audit` fails on any RUSTSEC vulnerability not ignored in
#           .cargo/audit.toml (unsound/unmaintained warnings are printed).
#   node    `npm audit --omit=dev` on every package below; HIGH or CRITICAL in
#           production deps fails unless the GHSA id is listed in
#           .github/supply-chain/npm-ignore.txt. A package without a lockfile
#           is audited from a temporary one, with its file: deps (packages in
#           this repo, audited on their own) left out.
#   secrets `gitleaks git` over the full history with .gitleaks.toml.
#
# Tools: cargo-audit, npm, gitleaks. A missing tool fails its section rather
# than silently passing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
MODE="${1:-all}"
FAIL=0

ids_from() { grep -vE '^\s*(#|$)' "$1" 2>/dev/null | awk '{print $1}' || true; }
section() { printf '\n==> %s\n' "$*"; }

run_rust() {
  section "cargo audit"
  command -v cargo-audit >/dev/null || { echo "cargo-audit not installed"; return 1; }
  cargo audit
}

NODE_PACKAGES=(sdk/agreements sdk/agreements-mcp demo)

run_node() {
  command -v npm >/dev/null || { echo "npm not installed"; return 1; }
  local ignore; ignore="$(ids_from .github/supply-chain/npm-ignore.txt | tr '\n' ' ')"
  local pkg dir tmp rc
  for pkg in "${NODE_PACKAGES[@]}"; do
    section "npm audit: $pkg"
    [ -f "$pkg/package.json" ] || { echo "missing $pkg/package.json"; FAIL=1; continue; }
    dir="$ROOT/$pkg"
    if [ ! -f "$dir/package-lock.json" ]; then
      tmp="$(mktemp -d)"
      node -e '
        const fs=require("fs"); const p=JSON.parse(fs.readFileSync(process.argv[1]));
        for (const k of ["dependencies","optionalDependencies"]) for (const [n,v] of Object.entries(p[k]||{})) if (/^(file|link):/.test(v)) delete p[k][n];
        delete p.devDependencies; fs.writeFileSync(process.argv[2]+"/package.json", JSON.stringify(p));' "$dir/package.json" "$tmp"
      (cd "$tmp" && npm install --package-lock-only --ignore-scripts --no-audit --no-fund --silent)
      dir="$tmp"
      echo "(no lockfile: audited from a temporary lock, file: deps left out)"
    fi
    rc=0
    (cd "$dir" && { npm audit --omit=dev --json 2>/dev/null || true; } \
      | NPM_IGNORE="$ignore" node -e '
        let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
          let r; try{ r=JSON.parse(s);}catch{ console.log("npm audit produced no JSON (registry unreachable?)"); process.exit(2);}
          const ignore=new Set((process.env.NPM_IGNORE||"").split(/\s+/).filter(Boolean));
          const vulns=r.vulnerabilities||{}; let bad=0, shown=0;
          for (const [name,v] of Object.entries(vulns)) {
            const direct=(v.via||[]).filter(x=>typeof x==="object");
            if (!direct.length) continue;
            const ids=direct.map(x=>x.url?x.url.split("/").pop():String(x.source));
            shown++;
            const gate=(v.severity==="high"||v.severity==="critical");
            const ignored=ids.length&&ids.every(i=>ignore.has(i));
            console.log(`${gate&&!ignored?"FAIL":"    "} ${v.severity.padEnd(8)} ${name}@${v.range}  ${ids.join(",")}${ignored?"  (ignored)":""}`);
            if (gate&&!ignored) bad++;
          }
          if (!shown) console.log("found 0 vulnerabilities");
          process.exit(bad?1:0);
        });') || rc=$?
    [ "$rc" -eq 0 ] || FAIL=1
  done
}

run_secrets() {
  section "gitleaks (full history)"
  command -v gitleaks >/dev/null || { echo "gitleaks not installed"; return 1; }
  gitleaks git --no-banner --redact=90 --config .gitleaks.toml .
}

case "$MODE" in
  rust)    run_rust || FAIL=1 ;;
  node)    run_node || FAIL=1 ;;
  secrets) run_secrets || FAIL=1 ;;
  all)
    run_rust    || FAIL=1
    run_node    || FAIL=1
    run_secrets || FAIL=1
    ;;
  *) echo "usage: $0 [rust|node|secrets|all]" >&2; exit 2 ;;
esac
exit $FAIL
