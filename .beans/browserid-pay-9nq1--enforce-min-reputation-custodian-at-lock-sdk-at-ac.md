---
# browserid-pay-9nq1
title: 'Enforce min_reputation: custodian at lock, SDK at accept; demo grant'
status: completed
type: feature
priority: normal
created_at: 2026-09-16T13:44:25Z
updated_at: 2026-09-16T14:01:05Z
parent: browserid-pay-h1f5
---

Reputation-gated grants (docs/plans/2026-09-16-reputation-design-draft.md, 'Plugging it back in'). Custodian: at lock, each side's contract:agreement min_reputation is checked against the other party's signed scorecard at the indexer the grantor named (signature verified against /v1/reputation/key, cached); reputation_below_minimum (403) / reputation_unavailable (503); the card seen is stored on the escrow. SDK: accept checks the acceptor's own card early; propose takes min_reputation; MCP propose tool exposes it. Demo contract 3 + negative case. browserid-ng pin → f7df6bb.

- [x] pin bump + literals
- [x] custodian check + tests
- [x] SDK + MCP
- [x] demo + README
- [x] deploy verified

## Summary of Changes

Custodian enforces min_reputation at lock (signed card from the grantor-named indexer, stored on the escrow); SDK checks at accept; propose/MCP take min_reputation; demo contract 3 + negative case; browserid-ng pin f7df6bb. Commit 7920e30.
