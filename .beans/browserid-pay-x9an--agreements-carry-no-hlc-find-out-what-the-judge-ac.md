---
# browserid-pay-x9an
title: 'Agreements carry no HLC: find out what the judge actually times against'
status: completed
type: task
priority: high
created_at: 2026-09-18T12:27:11Z
updated_at: 2026-09-18T16:33:43Z
blocking:
    - browserid-pay-b3gq
blocked_by:
    - browserid-pay-e1ou
---

**Agreement writes carry no HLC header, so no backdating bound is enforced on them at all.**

The daemon's bound is `T_b − W ≤ physical ≤ T_b + ε` (`sbo-daemon/src/validate.rs`, `check_hlc_bound`), and it is evaluated **only when the message carries an HLC**: *"no HLC → base inclusion-order semantics"*. The SDK envelope builder supports the header — `if (m.hlc) h.HLC = m.hlc` (`sdk/agreements/src/envelope.mjs:40`) — but **no caller sets it**, and `crates/chain/src/identity.rs:203` passes `hlc: None`.

So the 5-minute `DEFAULT_MAX_AUTHORING_LAG_MS` discussed elsewhere is moot for agreements: there is no self-asserted timestamp to bound.

**This is not automatically a bug — it depends on where the judge gets its times from.** Two cases, and they are opposite:

1. **Times come from block inclusion order.** Then there is nothing to forge: an object's time *is* when it landed, and deadlines evaluated against it are un-gameable. No HLC is the safe configuration, and adding HLC would introduce the very backdating risk `W` exists to bound.
2. **Times come from payload fields** (a `created_at`, a `delivered_at`). Then they are **completely unvalidated** — the daemon does not check payload timestamps at all — and an agent can assert any time it likes with no bound whatsoever. That is strictly worse than HLC, and the adjudicator would be ruling on forged evidence.

**So the task is to find out which, and write it down.** It has probably never been decided explicitly, which is why this is worth checking rather than assuming.

- [ ] where does the judge actually source times — inclusion time, payload fields, or both?
- [ ] where does `deriveObligations` source times when it computes lateness?
- [ ] if payload fields are load-bearing anywhere, that is the real finding and needs fixing — either bound them or stop trusting them
- [ ] if inclusion order is the source, record that as a deliberate choice, along with the consequence that resolution is one block (~20s) and not better
- [ ] only then decide whether agreements should start carrying an HLC at all

Note also a stale doc comment in `check_hlc_bound`: *"per-collection `W` ... is wired in a later step"* — it already is; `collection_max_lag_ms(msg, state)` is called on the line above.

## Investigated (2026-09-18) — answer: neither, and it is worse

The question was whether the judge times against inclusion order (safe) or payload fields (bad). The answer is **payload fields, and the judge is given no chain time at all** — see `browserid-pay-e1ou`, which this bean now depends on.

`ObjectView` drops everything but `{path, id, creator, value, confirmed}`, so no block number reaches the judge; meanwhile `deriveObligations` and the adjudicator's prompt both consume a self-asserted `at` written by the interested party.

So the HLC question is settled, but as the *last* of four steps rather than the fix:

**Yes, add an HLC to agreement writes** — it is self-asserted but bounded by `W` (5 min default), which is strictly better than an unbounded payload field, and it gives sub-block resolution. There is no reason not to.

**But it is not the fix.** HLC constrains a claim; inclusion time is a fact no party controls. Deadline evaluation should use inclusion time, with HLC for ordering and resolution inside a block. Adding HLC while `deriveObligations` still reads payload `at` would change nothing.

## Resolution (2026-09-18): do NOT add an HLC

The earlier note here said "yes, add an HLC, as the last of four steps". **That is now wrong**, and the reason is worth keeping.

HLC was attractive when the alternative was an unbounded payload timestamp: it is still self-asserted, but at least `W` bounds it. Moving the agreement clock to **block heights** removed the premise. There is no self-asserted time left in an agreement to bound — deadlines, expiry and windows are heights, and evidence is ordered by the block it landed in. Adding an HLC now would reintroduce exactly the thing that had to be removed: a number the writer chooses, sitting next to one the chain assigns, inviting some future reader to trust the wrong one.

So agreements continue to carry no HLC, and that is the right configuration rather than an oversight. `check_hlc_bound` returning `Ok(())` for them is correct.

The original question — *what does the judge actually time against?* — is answered: **the block each evidence object landed in**, sorted and passed to the adjudicator (`browserid-pay-dtjt`), with the prompt stating plainly that any time-looking payload field is a party's claim.

## Summary of Changes

No code. A decision reversed with its reasoning: heights made HLC unnecessary for agreements, and adding one would be a regression rather than an improvement. `sbo-xgyr` (persisting block TIME) is likewise demoted — it is wanted for display estimates and for the notary idea, not for anything money hangs on.
