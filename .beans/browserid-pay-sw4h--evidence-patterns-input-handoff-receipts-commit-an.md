---
# browserid-pay-sw4h
title: 'Evidence patterns: input handoff receipts, commit-and-reveal deliverables, milestone deadlines'
status: completed
type: feature
priority: normal
created_at: 2026-09-15T22:03:05Z
updated_at: 2026-09-15T22:19:30Z
parent: browserid-pay-h1f5
---

Live run gap: the agreement depended on alice sending papers after acceptance; if never sent there is no recourse. Standardize patterns parties adapt: (1) input handoff — sender posts a commitment over the inputs, receiver posts a receipt acknowledgement (countersigned handoff on chain, dated); (2) commit-and-reveal deliverable (deliver = hash, respond/settle = reveal); (3) milestone deadlines with dispute rights tied to the unmet party (pairs with deadline release b1ue); (4) public deliverables. Ship as a patterns doc + SDK helpers (handoff, acknowledge_receipt) + typed obligations in agreement.v1 terms (input vs deliverable, who bears the deadline).

## Summary of Changes

docs/specs/evidence-patterns.md (input handoff + receipt, commit-and-reveal, typed obligations with depends_on and sliding deadlines, public deliverables, adjudication reading); agreement-v1.md gains typed obligations and the handoff/receipt evidence kinds (+ writer-claimed `at` on every evidence object). SDK: handoff(), acknowledgeReceipt(), obligations(), deriveObligations(), contentCommitment(); MCP tools handoff / acknowledge_receipt / obligations, propose schema takes kind + depends_on. Demo contract 1 does handoff + receipt before delivery; client CLAUDE.md teaches the patterns. Tests: SDK 10/10.
