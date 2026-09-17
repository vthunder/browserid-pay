---
# browserid-pay-07w7
title: 'Milestones: partial delivery without a dispute'
status: draft
type: feature
priority: normal
created_at: 2026-09-17T14:34:05Z
updated_at: 2026-09-17T14:34:05Z
---

An escrow is one lock and one release, all-or-nothing. A ten-paper job that delivers six has no representation except a dispute — the counterparty either acknowledges work that is not done, or escalates to a judge over what should be an ordinary partial settlement.

Largely a custodian feature: the escrow would hold a schedule rather than a single amount, releasing per milestone on acknowledgement, with the remainder still disputable. The obligations model already types obligations and tracks per-obligation state (`deriveObligations`), so the terms side may mostly exist; the money side does not.

- [ ] escrow schedule: lock once, release per milestone
- [ ] tie a release to an obligation index so `obligations` and the ledger agree
- [ ] what a dispute means when some milestones settled — partial rulings
- [ ] consent card wording: the cap is the total, but release is piecewise

Backlog (Dan, 2026-09-17): good feature, not urgent.
