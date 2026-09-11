---
# browserid-pay-gtrp
title: 'Agent payments: custodian-enforced budgets over x402 (design draft)'
status: in-progress
type: feature
priority: normal
created_at: 2026-09-11T16:46:36Z
updated_at: 2026-09-12T00:00:00Z
---

Design draft for agent payments: a stateful custodian audience enforces spend caps carried as warrant scope parameters; agents settle x402 challenges through it; receipts private, commitments/attestations to SBO later. Doc: docs/plans/2026-09-11-agent-payments-design-draft.md

- [x] Write flows-first draft (~150 lines)
- [ ] Dan review

Review 2026-09-12: audience = custodian (payees are a parameter); vocabulary kept to cap only; spend UI at custodian; pay in a separate package; rolling windows.

Moved from browserid-ng (bean browserid-ng-gtrp) on 2026-09-12.
