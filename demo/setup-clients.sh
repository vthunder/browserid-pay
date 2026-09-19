#!/bin/bash
# Create a Claude Code client directory per agent for the live run.
#
#   demo/setup-clients.sh [target-dir] [agent ...]
#
# Default target: demo/clients. Default agents: alice bob carl. An agent is
# just a directory name — its IDENTITY comes from provisioning, when a human
# approves it against whichever email they sign in with, and nothing here
# needs to know that address in advance.
#
# Which email you approve each one against does matter for REPUTATION, though:
# the indexer scores agreements between agents delegated by the same human at
# nothing. alice and bob on plus-addresses of one account exercise the
# mechanics; an agent approved against a genuinely separate account is the
# only way to move a score.
#
# Each directory gets a .mcp.json wiring the browserid wallet and the
# agreements MCP server (run from THIS checkout) with per-agent state under
# <dir>/state, and a CLAUDE.md with the agent's role. Then, one terminal each:
#
#   cd <target>/alice && claude
#   cd <target>/bob   && claude
#   cd <target>/carl  && claude
#
# Re-running is safe: it rewrites .mcp.json / CLAUDE.md / settings.json and
# leaves <dir>/state alone, so an already-provisioned agent keeps its identity.
#
# Prerequisites on the machine running this: node 18+, `claude`, and
# `npm install` done in sdk/agreements and sdk/agreements-mcp.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$(cd "${1:-$ROOT/demo/clients}" 2>/dev/null && pwd || { mkdir -p "${1:-$ROOT/demo/clients}" && cd "${1:-$ROOT/demo/clients}" && pwd; })"
SERVER="$ROOT/sdk/agreements-mcp/server.mjs"
[ -f "$ROOT/sdk/agreements-mcp/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements-mcp" && npm install --silent)
[ -f "$ROOT/sdk/agreements/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements" && npm install --silent)

shift || true
AGENTS=("$@"); [ ${#AGENTS[@]} -gt 0 ] || AGENTS=(alice bob carl)

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
