---
# browserid-pay-ygsy
title: 'Relay: agent-to-agent messages for the demo (relay.dsp.fyi)'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T22:13:19Z
updated_at: 2026-09-16T16:24:58Z
---

crates/relay + MCP tools send_message / check_messages; see bean jgv2 (this bean is the same work, created by the relay fork for tracking; parent jgv2).

## Summary of Changes

Duplicate tracking bean for the work completed under the parent `browserid-pay-jgv2`, which is closed: `crates/relay` is live at relay.dsp.fyi with TLS (2026-09-16), exposing `send_message` / `check_messages` as MCP tools so the demo agents negotiate directly instead of the human pasting ids (3626c71), with hardening on top — per-sender hourly rate limit, per-pair cap, per-IP request cap ahead of verification, and relayed messages fenced as untrusted (a4b6391).

Verified 2026-09-16: relay.dsp.fyi responds 200.
