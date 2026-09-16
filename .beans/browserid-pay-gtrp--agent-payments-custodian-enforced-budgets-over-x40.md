---
# browserid-pay-gtrp
title: 'Agent payments: custodian-enforced budgets over x402 (design draft)'
status: in-progress
type: feature
priority: normal
created_at: 2026-09-11T16:46:36Z
updated_at: 2026-09-16T16:18:22Z
---

Design draft for agent payments: a stateful custodian audience enforces spend caps carried as warrant scope parameters; agents settle x402 challenges through it; receipts private, commitments/attestations to SBO later. Doc: docs/plans/2026-09-11-agent-payments-design-draft.md

- [x] Write flows-first draft (~150 lines)
- [x] Dan review (2026-09-12; design validated by the build)

Review 2026-09-12: audience = custodian (payees are a parameter); vocabulary kept to cap only; spend UI at custodian; pay in a separate package; rolling windows.

Moved from browserid-ng (bean browserid-ng-gtrp) on 2026-09-12.

## Status 2026-09-16

Reviewed and built, minus the rail. The design held up through the
agreements implementation: the custodian is the audience, `pay:<rail>`
carries the `cap` parameter, the counter is per warrant (keyed by the
status ref), the window is rolling from the warrant's `iat`, an uncapped
or malformed cap fails closed, and the refusal codes are as drafted
(`no_grant`, `cap_exceeded` with `remaining`, `revoked`,
`unsupported_currency`). See `crates/custodian/src/pay.rs`.

The rail is the open part. The agreements build settles in **test money
only**, so v1 ships `pay:transfer` — an internal move between accounts at
the same custodian — rather than x402. This is deliberate, not a gap in
the design: x402 is still the plan for paying systems that do not speak
browserid-pay, with further rails (cards, fiat) behind the same audience
and the same cap afterwards. That work is `browserid-pay-f38b`, which also
carries the separate agent-side `@browserid-ng/pay` package (today `pay` is
an endpoint on the custodian, not a package).

Doc status line updated to match.
