---
# browserid-pay-1iv1
title: 'Custodian chain writer: post lock objects and settled attestations as escrow@dsp.fyi'
status: completed
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-15T20:06:04Z
parent: browserid-pay-h1f5
---

escrow.rs has a Publisher trait with a log-only impl. Give the custodian a browserid agent identity (same provisioning story as the judge: crates/judge README) and post /agreements/<id>/lock + agreement:settled / agreement:disputed attestations via TurboDA.

## Summary of Changes

crates/chain (shared: identity provisioning/pickup, device-model envelopes, daemon reads/submits, listings) extracted from the judge; the custodian provisions its own delegated identity (handle escrow), serves GET /api/setup, posts /providers/custodians/<identity>/listing, and escrow::Publisher is real: lock objects (agreement.lock.v1) and agreement:settled / agreement:disputed attestations on both parties, all through the daemon.
