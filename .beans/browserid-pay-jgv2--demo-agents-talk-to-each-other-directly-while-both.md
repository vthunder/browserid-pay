---
# browserid-pay-jgv2
title: 'Demo: agents talk to each other directly while both transcripts stay visible'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T22:03:05Z
updated_at: 2026-09-15T22:25:18Z
parent: browserid-pay-h1f5
---

Dan: cooler if the agents negotiated with each other instead of the human pasting ids/terms. Candidate: a tiny relay (mailbox) MCP tool — send/receive messages addressed by identity, authenticated by browserid presentations, hosted next to the custodian or as demo/relay — so each Claude session shows its own tool calls and the human watches two terminals. Off-chain (chat is not evidence); anything that must be evidence goes through the evidence patterns.

## Summary of Changes

crates/relay (relay.dsp.fyi, live with TLS 2026-09-16): POST /api/send { presentation, to, body } / POST /api/receive { presentation, since? }; sender = the presentation's acting identity under a message-scoped warrant for the relay origin; SQLite, 16 KB bodies, 3-day TTL, 500 per mailbox; DNSSEC verification in prod, pinned issuer in dev. MCP tools send_message / check_messages in sdk/agreements-mcp (AGREEMENTS_RELAY env). demo/setup-clients.sh CLAUDE.md tells each agent to message the other and poll when waiting; demo/README.md contract-1 flow now runs agent-to-agent. Deploy: deploy-relay.yml (self-deploy), sandmill-infra apps/hobby/relay.conf + secrets/relay.env.age. Commit 3626c71.
