---
# browserid-pay-er8j
title: Chain-enforced contribution floor + fee caps (policy restriction on payload fields)
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T21:35:15Z
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

## Summary of Changes

Done end to end and verified on the live chain (2026-09-16).

**sbo (`sbo-pu34`, commits 1eb6d21 / fb19728, pushed).** The policy language gained what this needed: an optional `id` pattern on grants *and* restrictions (absent ⇒ any id, so existing policies are unchanged), and JSON-pointer `fields` conditions (`eq`/`min`/`max`) that fail closed and read numeric strings as numbers. Also closed a hole the id field would have opened in `grant_covered_by_template`. Spec documented; 194 tests.

**Chain.** Root policy republished as a governed update — no regenesis — finalized at block **3890678**:

    {"on": "/agreements/*/", "id": "proposal", "require": {"fields": [
      {"pointer": "/contribution/fn", "eq": "max"},
      {"pointer": "/contribution/amount", "min": 0.01},
      {"pointer": "/contribution/percentage", "min": 0.01}]}}

`/sys/contribution` republished at block **3890726**: suggestion 0.5%, with the enforced minimum alongside it.

**Verified both directions live** with `dsp-genesis policy-probe`:

- zero contribution → `400: Policy: Payload value at '/contribution/amount' is 0, below the minimum 0.01`
- compliant proposal → still writes

**Custodian.** `contribution_minor` does the arithmetic the chain deliberately cannot (it needs the price, sealed in commitment mode). Fixed three bugs there: an absolute amount *overrode* a percentage instead of combining; anything malformed fell through to a silent zero; and it rounded where the SDK ceils, so the two disagreed by a cent.

**SDK.** Contribution is a structured formula `{recipient, fn, amount, percentage}` — structure, not an expression string, so the policy needs no parser. Fees moved onto the clear proposal so caps on them are expressible. `normalizeContribution` always emits all three fields at or above the floor.

**Deploy.** sbo pushed → mingo daemon bumped and released (e16c5c4) → browserid-pay released → policy published. Ordering matters: a daemon without the new features silently ignores both fields.

## Follow-ups

- Three junk `_policyprobe_*` proposals are permanently on chain, written while the floor was published but not yet in force. Harmless (no parties, no escrow) but permanent.
- A daemon replaying after a redeploy validates against stale policy while `/v1/object` serves the new one from the pending overlay — reads and validation disagree with no way for a client to tell. Cost three misdiagnoses here. Worth a bean in sbo.
- `normalizeContribution` raising a below-floor caller to the floor means a caller asking for zero silently pays a cent. Deliberate (the chain would refuse otherwise) but worth revisiting if it surprises anyone.
