---
# browserid-pay-b3gq
title: Negotiable timings, with floors the ledger enforces
status: draft
type: feature
priority: normal
created_at: 2026-09-17T20:31:12Z
updated_at: 2026-09-18T12:27:37Z
---

Every timing in an agreement is fixed by the protocol today: the delivery response window (P2D), the absolute resolution window (3x that), and the windows the two liveness beans will need — the adjudicator's ruling window (`browserid-pay-6ai6`) and the abandonment backstop (`browserid-pay-ov7g`).

Fixed on purpose. `response_window` and `resolution_window` WERE negotiable terms on the proposal for about a day, and that is a griefing vector with no floor: a proposer sets `PT1S`, the counterparty cannot answer in time, the obligation goes `unanswered` and they are dragged to an adjudicator — losing the escrowed fee for nothing. Removed 2026-09-17 rather than shipped with a hole (Dan: hardcode for now, split the configurable version out).

## What making them negotiable needs

- [ ] a FLOOR per window, enforced by the chain rather than by client code, so the terms cannot name something the counterparty cannot meet. The machinery exists: a root-policy restriction with `fields` conditions on the proposal, exactly as the contribution floor works (`sbo-pu34`, `browserid-pay-er8j`)
- [ ] decide them as a coherent SET, not one at a time: response < resolution < ruling < backstop is presumably a required ordering, and a policy that floors each one separately will not catch an inconsistent combination
- [ ] what the acceptor sees before agreeing — timings belong on the consent card if they can cost the human money
- [ ] both sides must derive the same view. Today the windows are a LOCAL SDK option (`new Agreements({ windows })`), used by tests and deployments; if the two agents disagree they disagree about whose turn it is. Publishing them as terms fixes that, which is an argument FOR negotiability once the floors exist
- [ ] the custodian consumes none of them at present (its only consumer, `lapsed()`, is gone), so this is currently an SDK-only concern. Publishing them again would change that

Worth doing when someone has a real agreement the defaults do not suit — a same-day job needs hours, not two days — rather than before.

## Superseded by the timing rework (2026-09-18)

The windows this bean described no longer exist in that form. `resolution_window` is **gone**: the retry loop is bounded by the agreement's own deadline, and the response window became a `min` against it rather than a clock of its own. What a proposer could negotiate today is a different, smaller set.

The current set, all protocol constants:

| window | value | what it does |
|---|---|---|
| response | 1h | pulls the delivery clock IN when the work arrives early, so an early deliverer is paid early. Never pushes it out. |
| proposal expiry | 6h | how long a proposal stands |
| obligation deadline | 7d default | the bound on everything else |
| judge response | 1h | the disputed party's chance to reveal |
| ruling | 6h | before the adjudicator may be declared unresponsive |
| backstop | max(2x delivery, 24h), cap 30d | abandonment split |

Two orderings any negotiable version must enforce, which is the real argument for chain-enforced floors rather than client checks:

- `judge response < ruling` — otherwise a judge can be declared unresponsive at the moment it first becomes able to rule
- the backstop must not be able to fire while a legitimate window is open

The griefing vector that caused this bean is unchanged: with no floor, a proposer sets a window the counterparty cannot meet. Still worth doing when a real agreement needs timings the defaults do not suit — a same-day job wanting a 10-minute response window — rather than before.

## `W` sets a floor under every window (2026-09-18)

Investigating notary timing for the forum epic turned up a constraint that lands directly here: **the backdating bound `W` is the slack in every deadline.** If an authoring timestamp can be back-dated by up to `W`, a window shorter than `W` is not enforceable — the counterparty can miss it and assert they did not.

So any floor this bean installs must be **well above `W`**, and the chain-enforced floor it calls for now has a principled basis rather than a guessed number.

**How low can `W` go?** The bound is `T_b − W ≤ physical ≤ T_b + ε` where **`T_b` is the block's inclusion time** (`sbo-daemon/src/validate.rs`, `check_hlc_bound`) — not the daemon's receipt clock. So `W` is consumed by everything between authoring and inclusion:

- the collection's `batch_interval_s`, if batched (mingo spaces use 5s; agreements post no collection config, so they are not batched)
- submission and network latency
- mempool wait
- block time (~20s on Avail turing) plus 1–2 block finality

That is realistically 30–90s of the budget. The default `W` is **5 minutes** (`DEFAULT_MAX_AUTHORING_LAG_MS`), leaving 3–4 minutes of headroom; forward skew tolerance `ε` is another 5 minutes, so a timestamp sits in a ~10-minute band. **Tightening `W` much below ~2 minutes would start rejecting honest writes** whenever the network is slow or a block is late. 5 minutes is about right, and it is not a number worth fighting for.

Consequence: **the shortest enforceable agreement window is realistically tens of minutes**, and that should be the floor's order of magnitude.

**And the adjudicator must treat timestamps as ±`W`, not as exact.** If the judge reads a delivery timestamp as a precise fact, it will rule confidently on differences smaller than the bound the timestamp actually carries. Whether that matters depends on `browserid-pay-x9an` — agreements carry no HLC today, so it is not yet clear what the judge is timing against at all. **Settle that first**; this floor cannot be specified until it is known.

**Batching interacts with this**, and is worth stating for whoever configures a collection later: raising a collection's `batch_interval_s` spends the same budget the deadline needs. A forum can afford it (mingo runs `W` at 24h); anything with a deadline cannot.
