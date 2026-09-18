---
# browserid-pay-x9an
title: 'Agreements carry no HLC: find out what the judge actually times against'
status: todo
type: task
priority: high
created_at: 2026-09-18T12:27:11Z
updated_at: 2026-09-18T12:27:11Z
blocking:
    - browserid-pay-b3gq
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
