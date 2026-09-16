---
# browserid-pay-f38b
title: 'x402 rail: @browserid-ng/pay + external settlement (deferred)'
status: todo
type: feature
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:18:22Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-6dlx
---

Per the payments draft: pay:x402 scope, cap, custodian signs EIP-3009 from the account, returns X-PAYMENT; real USDC deposits. After agreements.

Not a gap in the payments design — a deliberate sequencing choice: the
agreements build settles in **test money only**, so v1 ships the
`pay:transfer` rail (an internal move between accounts at the same
custodian). x402 is the rail for paying systems that do not speak
browserid-pay; further rails (cards, fiat) follow behind the same audience
and the same cap, per the `pay:<rail>` namespace.

Scope:

- [ ] `pay:x402` rail at the custodian: EIP-3009 transfer signed from the account, `X-PAYMENT` returned
- [ ] real USDC deposits and balances (replacing self-serve test credit)
- [ ] `@browserid-ng/pay`: the separate agent-side package taking a 402 challenge and returning the payment header (today `pay` is only a custodian endpoint)
- [ ] later rails behind the same audience: `pay:card`, fiat

Design: `docs/plans/2026-09-11-agent-payments-design-draft.md` (bean `browserid-pay-gtrp`).
