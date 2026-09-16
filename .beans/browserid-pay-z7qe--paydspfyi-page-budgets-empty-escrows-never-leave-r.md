---
# browserid-pay-z7qe
title: 'pay.dsp.fyi page: budgets empty, escrows never leave, receipts are not a ledger'
status: completed
type: bug
priority: normal
created_at: 2026-09-16T12:29:45Z
updated_at: 2026-09-16T12:46:11Z
---

Dan after the live runs: Budgets always empty (only /api/pay upserts a warrant row; escrow lock never records the contract:agreement grant); Escrows keeps settled agreements forever (should be Agreements with escrow as state); Receipts shows only grantor-side receipts (locks/settles) while the payee credits and refunds are ledger rows shown nowhere, so it reads as -168 against a +100 deposit.

- [x] record agreement grants as budgets at lock
- [x] /api/me: ledger with running balance, agents+grants, agreements with money
- [x] page: Account / Agents & budgets / Agreements
- [x] tests + demo-local (custodian 12, demo-local green)
- [x] deploy (run 35096733518, pay.dsp.fyi healthy, new page served)

## Summary of Changes

Lock records both parties' contract:agreement grants as budgets (cap, counterparties, agreement id) and the payer's lock as a spend; /api/me gained ledger (running balance), agents[] (grants grouped by agent), agreements[] (role, agents, money moved, links); page reorganized into Account / Agents & budgets / Agreements (settled collapsed). Commit 7f6b389, deployed 2026-09-16.
