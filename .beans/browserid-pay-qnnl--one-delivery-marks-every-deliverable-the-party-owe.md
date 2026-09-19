---
# browserid-pay-qnnl
title: One delivery marks every deliverable the party owes as met
status: completed
type: bug
priority: high
created_at: 2026-09-19T22:10:31Z
updated_at: 2026-09-19T22:10:31Z
---

deriveObligations matched deliveries by `by` only, ignoring `obligation` — which it honours for handoffs/receipts. `deliver()` had no `obligation` parameter at all, so a deliverer could not tag one even if they wanted to; carl said 'Obligation 1 of 2' in the note, which the ledger does not read.

Live effect (alice/carl, $30, two milestones): one delivery marked BOTH deliverables delivered, including the final mark. settle is agreement-level, so the payer's only options were to release the whole price for half the work or post a `not_met` that was untrue.

- [x] deriveObligations: a delivery counts for its tagged obligation; untagged counts for the party's FIRST deliverable only
- [x] deliver() and the MCP tool take `obligation`
- [x] Test reproducing the two-milestone case

## Summary of Changes

Single-deliverable agreements are unaffected. Does NOT add partial release — settle is still agreement-level (browserid-pay-07w7).
