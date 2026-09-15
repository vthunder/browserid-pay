---
# browserid-pay-6dlx
title: 'Custodian core: accounts, bundle verification, per-warrant caps, ledger'
status: in-progress
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T14:32:30Z
parent: browserid-pay-h1f5
---

Rust (axum + SQLite), depends on browserid-ng crates by git. Accounts keyed by grantor identity; admin-issued test credit (v1); POST /pay and escrow endpoints verify the four-object bundle via browserid-rp Verifier with fail-closed status; per-warrant counters keyed by status idx; rolling cap windows; ledger UI at pay.dsp.fyi.

- [ ] crate skeleton + config + SQLite schema
- [ ] identity: custodian signs as escrow@dsp.fyi (agent device cert via browserid.me)
- [ ] bundle verification + cap enforcement (pay: and contract: scopes)
- [ ] accounts, admin credit, ledger + receipts
- [ ] ledger UI (login with browserid)
- [ ] tests
