---
# browserid-pay-6dlx
title: 'Custodian core: accounts, bundle verification, per-warrant caps, ledger'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T16:03:50Z
parent: browserid-pay-h1f5
---

Rust (axum + SQLite), depends on browserid-ng crates by git. Accounts keyed by grantor identity; admin-issued test credit (v1); POST /pay and escrow endpoints verify the four-object bundle via browserid-rp Verifier with fail-closed status; per-warrant counters keyed by status idx; rolling cap windows; ledger UI at pay.dsp.fyi.

- [x] crate skeleton + config + SQLite schema
- [ ] identity: custodian signs as escrow@dsp.fyi (agent device cert via browserid.me)
- [x] bundle verification + cap enforcement (pay: scopes; contract: comes with escrow y48h)
- [x] accounts, admin credit, ledger + receipts
- [x] ledger UI (login with browserid)
- [x] tests (7 unit/integration, pinned-issuer path)
- [x] deployed to pay.dsp.fyi 2026-09-15 (released by image; CI key pending Dan)

## Summary of Changes

crates/custodian: axum + SQLite; /api/pay with per-warrant caps over rolling windows; escrow (y48h) added on top; ledger UI; admin test credit. Live at https://pay.dsp.fyi.
