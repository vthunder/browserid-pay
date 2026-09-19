---
# browserid-pay-pz8m
title: Agreements MCP server misses warrants the wallet grants
status: completed
type: bug
priority: high
created_at: 2026-09-19T21:59:37Z
updated_at: 2026-09-19T21:59:37Z
---

The wallet and agreements MCP servers are separate processes sharing state/agent-credential.json. The agreements server built its grant map once at startup and never re-read it, so a warrant approved through the wallet's `authorize` was invisible: `warrantedAudiences()` lacked the per-agreement audience, so `#contractAssertion` fell back to the broader collection warrant, which the custodian then refused because its counterparties did not cover the counterparty.

Worse, `persistGrants` overwrote `grants` wholesale, so this process could clobber a warrant the wallet had just added.

Seen live: alice funding an agreement with carl@gmail.com. The per-agreement warrant was granted and visible in the wallet, but fund kept presenting the `*@sandmill.org` collection warrant and being rejected.

- [x] refreshGrants() re-reads the file before every tool call
- [x] persistGrants merges instead of overwriting

## Summary of Changes

Both directions of the shared file are now handled: read before each call, merge on write (ours wins per audience, other processes' audiences preserved).
