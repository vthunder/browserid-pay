---
# browserid-pay-y48h
title: 'Escrow: lock, release conditions, rulings (custodian ↔ chain)'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:25:18Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-6dlx
    - browserid-pay-vpe8
---

pay:escrow via the contract warrant (one warrant per agent): lock price + fees against a contract, post escrow-lock object, release on acknowledgement / ruling / deadline, pay custodian fee + adjudicator fee + contribution, receipts, completion attestations, sealed-terms copy held at lock.

- [x] lock
- [x] release conditions (acknowledge; deadline sweep still todo)
- [x] ruling intake
- [x] attestations
- [x] tests

## Summary of Changes

Both remaining items done.

- **Attestations.** `crates/custodian/src/publish.rs:32` writes `attestation.v1` objects to `/<custodian>/attestations/<subject>/`; `settled()` (`publish.rs:67`) attests to *both* parties on every settlement, choosing `agreement:disputed` for a ruling-driven outcome and `agreement:settled` otherwise, carrying role and a value band rather than the amount (`value_band`, per the reputation draft). Live on chain: `agreement:settled`, `agreement:disputed` and `agreement:ruled` all resolve through index.dsp.fyi.
- **Tests.** 5 escrow tests in `crates/custodian/src/escrow.rs` — lock records both parties' grants as budgets, cap and counterparties bound the lock, lock→acknowledge pays everyone, dispute→ruling only from the named adjudicator, min_reputation gates the lock. `cargo test -p custodian`: 15 passed, 0 failed (2026-09-16).

Deadline sweep remains deliberately out of scope here; it is `browserid-pay-b1ue`.
