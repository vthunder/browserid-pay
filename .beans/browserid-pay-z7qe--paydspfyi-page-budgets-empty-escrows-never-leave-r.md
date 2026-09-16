---
# browserid-pay-z7qe
title: 'pay.dsp.fyi page: budgets empty, escrows never leave, receipts are not a ledger'
status: in-progress
type: bug
priority: normal
created_at: 2026-09-16T12:29:45Z
updated_at: 2026-09-16T12:35:33Z
---

Dan after the live runs: Budgets always empty (only /api/pay upserts a warrant row; escrow lock never records the contract:agreement grant); Escrows keeps settled agreements forever (should be Agreements with escrow as state); Receipts shows only grantor-side receipts (locks/settles) while the payee credits and refunds are ledger rows shown nowhere, so it reads as -168 against a +100 deposit.

- [x] record agreement grants as budgets at lock
- [x] /api/me: ledger with running balance, agents+grants, agreements with money
- [x] page: Account / Agents & budgets / Agreements
- [x] tests + demo-local (custodian 12, demo-local green)
- [ ] deploy
