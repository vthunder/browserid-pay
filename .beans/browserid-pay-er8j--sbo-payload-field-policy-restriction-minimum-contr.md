---
# browserid-pay-er8j
title: Chain-enforced contribution floor + fee caps (policy restriction on payload fields)
status: todo
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T20:24:58Z
---

Policy spec + daemon: restriction condition on a payload field (contribution.percentage ≥ N) on /agreements/**. Needed only to enforce the chain minimum.


## Rewritten 2026-09-16 (design settled with Dan)

Blocked on the sbo policy-language work: `sbo-pu34` (match on object id; value
conditions on payload fields). Nothing here can be expressed until that lands —
`/agreements/<id>/` holds `proposal`, `acceptance`, `lock`, `ruling` and evidence
in one container, and patterns match path only, so no rule can target proposals.

No regenesis required either way: the sys key holds `govern` on `/**`
(`crates/genesis/src/main.rs:277`), so a replacement `/sys/policies/root` can just
be posted. (Dan is fine with a regenesis if one is ever wanted — updating the
`_sbo.dsp.fyi` TXT is a small hassle, not a blocker.)

### Numbers

- minimum contribution **0.01%** (1 bp), enforced by the root policy
- suggested contribution **0.5%**, published in `/sys/contribution` (a genesis
  parameter today: `--contribution-percentage`)

### Contribution becomes a formula, not a scalar

Today the clear proposal carries `contribution: { recipient, percentage }`
(`sdk/agreements/src/agreements.mjs:223`) — percentage only, so an absolute floor
cannot even be stated. It should be able to express `max(amount, percentage)` or
`min(amount, percentage)`, e.g. "1 bp or at least 1 cent".

Structured, not a string — `{ recipient, fn: "max", amount, percentage }`:

- a string (`"max(0.01, 0.01%)"`) would force the daemon to parse an expression
  language to check it; structured JSON is checkable with plain pointer
  conditions and no parsing
- keeps JCS canonicalization (and therefore the commitment) stable

The policy pins `fn` and floors both operands — three scalar conditions, and the
policy never evaluates the formula. Pinning `fn` is not optional: without it a
proposer picks `min` and dodges the floor.

### Two layers of enforcement

- **Chain**: a coarse floor on the *declared* field. Cheap, public, works in
  commitment mode (contribution is clear in both modes).
- **Custodian**: the actual `max()` / `min()` arithmetic against the real price at
  lock. Same shape as `min_reputation` (custodian at lock, SDK at accept, `9nq1`).

The chain cannot enforce the combination — a declared `amount: 0.01` on a large
deal clears a chain floor while still violating `max(0.01, 0.01%)` — which is why
the custodian layer is required, not optional.

### Fees move into the clear

Decided: pull `fees: { custodian, adjudicator }` out of the sealed `full` terms
(`agreements.mjs:211`) and onto the clear proposal, so policy can enforce
min/max on them too. Part of keeping the ecosystem healthy is being able to cap
adjudication and escrow fees, not only floor the contribution.

Trade-off accepted: this publishes fee pricing on chain. It does not publish the
deal value, and attestations already publish a `value_band`. A cap on a `max()`
is the exact case for AND-ed componentwise conditions, so fee caps need no
approximation.

### Custodian arithmetic also changes

`crates/custodian/src/escrow.rs:341-345` treats the two forms as **either/or with
`amount` winning**, not `max()`, and falls through to `_ => 0` on anything
malformed — a silent zero contribution. Both need fixing alongside.

- [ ] (blocked) sbo policy language — `sbo-pu34`
- [ ] contribution becomes `{ recipient, fn, amount, percentage }` in the SDK, clear on the proposal and inside the committed terms
- [ ] fees pulled into the clear proposal
- [ ] custodian: `max()` / `min()` arithmetic; refuse a malformed contribution instead of charging 0
- [ ] root policy: floor the contribution at 0.01%, pin `fn`; optional fee caps
- [ ] `/sys/contribution` suggested percentage → 0.5%
- [ ] post the updated root policy (no regenesis) and confirm a below-floor proposal is rejected on chain
