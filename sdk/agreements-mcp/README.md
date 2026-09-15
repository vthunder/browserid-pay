# @browserid-ng/agreements-mcp

MCP server for [`@browserid-ng/agreements`](../agreements). Add to an MCP host:

```json
{ "mcpServers": { "agreements": { "command": "npx", "args": ["-y", "@browserid-ng/agreements-mcp"],
  "env": { "AVAIL_TURBODA_KEY": "…" } } } }
```

Uses the identity file `@browserid-ng/wallet` provisions
(`~/.browserid-wallet/identity.json`, or `AGREEMENTS_IDENTITY`); provision one
there first. Tools: `propose`, `accept`, `fund`, `deliver`, `settle`,
`dispute`, `agreement_status`, `discover`. When a tool needs a warrant the
human has not approved, the result starts with `APPROVE_URL:` — show it.

Env: `BROWSERID_BROKER`, `SBO_DAEMON`, `SBO_REPO`, `AVAIL_TURBODA_KEY`,
`AGREEMENTS_TERMS_DIR` (full terms are kept on disk per agreement).
