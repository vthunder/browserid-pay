#!/bin/bash
# Create a Claude Code client directory per agent for the live run.
# Run with no arguments for usage.
#
# Prerequisites on the machine running this: node 18+, `claude`, and
# `npm install` done in sdk/agreements and sdk/agreements-mcp.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<USAGE
Create a Claude Code client directory per agent.

  demo/setup-clients.sh <target-dir> [agent ...]

  <target-dir>   where the client directories go; created if missing
  [agent ...]    one directory name per agent (default: alice bob carl)

Examples:
  demo/setup-clients.sh ~/pay-clients
  demo/setup-clients.sh ~/pay-clients alice bob carl dave

Each directory gets a .mcp.json (browserid wallet + agreements server from
this checkout), per-agent state, tool permissions, and docs/AGENT-CLAUDE.md
as its CLAUDE.md. Then, one terminal each:

  cd <target-dir>/alice && claude

An agent is just a directory name. Its IDENTITY comes from provisioning, when
a human approves it against whichever email they sign in with — nothing here
needs to know that address in advance. Which email you approve each one
against does matter for REPUTATION: the indexer scores agreements between
agents delegated by the same human at nothing, so an agent that must build a
track record needs its own account.

Re-running is safe: it rewrites .mcp.json / CLAUDE.md / settings.json and
leaves <dir>/state alone, so an already-provisioned agent keeps its identity.
USAGE
}
[ $# -gt 0 ] || { usage; exit 0; }
case "$1" in -h|--help) usage; exit 0 ;; esac

DEST="$1"; shift
AGENTS=("$@"); [ ${#AGENTS[@]} -gt 0 ] || AGENTS=(alice bob carl)
for who in "${AGENTS[@]}"; do
  case "$who" in
    [a-z0-9]*) ;;
    *) echo "bad agent name: '$who' (a lowercase directory name, e.g. carl)" >&2; exit 2 ;;
  esac
done

TARGET="$(mkdir -p "$DEST" && cd "$DEST" && pwd)"
SERVER="$ROOT/sdk/agreements-mcp/server.mjs"
[ -f "$ROOT/sdk/agreements-mcp/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements-mcp" && npm install --silent)
[ -f "$ROOT/sdk/agreements/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements" && npm install --silent)

for who in "${AGENTS[@]}"; do
  d="$TARGET/$who"; mkdir -p "$d/state"
  cat > "$d/.mcp.json" <<JSON
{
  "mcpServers": {
    "browserid": {
      "command": "npx", "args": ["-y", "@browserid-ng/wallet"],
      "env": { "BROWSERID_HOME": "$d/state", "AGENT_NAME": "$who" }
    },
    "agreements": {
      "command": "node", "args": ["$SERVER"],
      "env": { "AGREEMENTS_IDENTITY": "$d/state/agent-credential.json",
               "AGREEMENTS_TERMS_DIR": "$d/state/terms" }
    }
  }
}
JSON
  # Claude Code's auto-mode classifier treats propose/fund as real-world
  # transactions and blocks them; allow the two MCP servers' tools up front.
  mkdir -p "$d/.claude"
  cat > "$d/.claude/settings.json" <<JSON
{ "permissions": { "allow": ["mcp__agreements__*", "mcp__browserid__*"] } }
JSON
  cp "$ROOT/docs/AGENT-CLAUDE.md" "$d/CLAUDE.md"
  echo "$who -> $d"
done
