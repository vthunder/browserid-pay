#!/bin/bash
# Create two Claude Code client directories (alice, bob) for the live run.
#
#   demo/setup-clients.sh [target-dir]      (default: demo/clients)
#
# Each directory gets a .mcp.json wiring the browserid wallet and the
# agreements MCP server (run from THIS checkout) with per-agent state under
# <dir>/state, and a CLAUDE.md with the agent's role. Then, in two terminals:
#
#   cd <target>/alice && claude
#   cd <target>/bob   && claude
#
# Prerequisites on the machine running this: node 18+, `claude`, and
# `npm install` done in sdk/agreements and sdk/agreements-mcp.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$(cd "${1:-$ROOT/demo/clients}" 2>/dev/null && pwd || { mkdir -p "${1:-$ROOT/demo/clients}" && cd "${1:-$ROOT/demo/clients}" && pwd; })"
SERVER="$ROOT/sdk/agreements-mcp/server.mjs"
[ -f "$ROOT/sdk/agreements-mcp/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements-mcp" && npm install --silent)
[ -f "$ROOT/sdk/agreements/node_modules/.package-lock.json" ] || (cd "$ROOT/sdk/agreements" && npm install --silent)

for who in alice bob; do
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
  other=$([ "$who" = alice ] && echo bob || echo alice)
  cat > "$d/CLAUDE.md" <<MD
# You are ${who}'s agent

You act for a human through browserid. Two MCP servers are configured:
\`browserid\` (your identity: \`provision\`, \`identity\`, \`authorize\`) and
\`agreements\` (\`discover\`, \`propose\`, \`accept\`, \`fund\`, \`deliver\`,
\`settle\`, \`dispute\`, \`agreement_status\`).

Rules:
- If you have no identity yet, call \`provision\` and show the human the
  approval link, code and fingerprint; wait until it reports approved.
- Whenever a tool returns an APPROVE_URL, show it to the human verbatim and
  wait for them to say it is approved before continuing.
- The other party is ${other}'s agent; the human will paste anything that
  must cross between you (an agreement id, the terms JSON) — ask for it.
- Report the exact tool results (ids, amounts, states); never invent them.
- The ledger page is https://pay.dsp.fyi and the index is https://index.dsp.fyi.
MD
  echo "$who -> $d"
done
