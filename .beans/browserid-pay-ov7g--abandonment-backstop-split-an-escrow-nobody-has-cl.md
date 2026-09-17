---
# browserid-pay-ov7g
title: 'Abandonment backstop: split an escrow nobody has claimed'
status: completed
type: feature
priority: normal
created_at: 2026-09-17T20:20:27Z
updated_at: 2026-09-17T22:48:50Z
---

When BOTH parties abandon an agreement the escrow stays locked for ever. Nobody is wronged — each had an exit and did not take it — but the custodian carries the funds indefinitely, and held funds are a real liability (Dan, 2026-09-17): they can be stolen, and they sit on the books with no prospect of resolution.

## Why a 50/50 split rather than a refund

A refund to the payer looks like the "fair" default — it matches the adjudicator rule that an unopened commitment is undelivered. It is the wrong choice here: it would mean *take the work, go quiet, get your money back*, which is precisely the stalling payoff this project keeps having to remove.

50/50 is deliberately arbitrary. It denies BOTH parties a full win for disappearing, which is the property that matters when the mechanism fires without anyone watching.

**The incentive problem is bounded by requiring both sides to be silent.** For the backstop to fire, the party in the right must ALSO have abandoned — and they have a faster, better remedy available the whole time (dispute, and win outright). Stalling to collect half is only attractive to someone who expects to lose a ruling, and it only works if their counterparty gives up too.

## Shape to work out

- [x] what the clock runs from: last on-chain activity by either party, not the lock, so any action resets it. A truly abandoned agreement is one with no activity at all.
- [x] how long: Dan suggested ~2x the agreement length. `latest_deadline` already reaches the custodian at lock, so the duration is known. Needs a FLOOR regardless, or a malicious proposer sets a short one and games it.
- [x] configurable-but-required, or fixed by the ledger? A clear proposal term with a minimum enforced by the root policy would match how the contribution floor works.
- [x] the non-price components: custodian fee earned (it did hold the funds), adjudicator fee refunded (no adjudication happened — consistent with `acknowledge`), contribution paid (it is a term of the agreement), and only the PRICE split.
- [x] this is the one case that genuinely needs a sweeper: by construction nobody is there to trigger it. A slow periodic job at the custodian, which is where the liability sits.
- [x] say it in the consent card, since it is a way a human can lose money without anyone deciding anything.

## The backstop must be SUSPENDED by a dispute, not merely reset (Dan, 2026-09-17)

**The attack:** a party who expects to lose a ruling attacks the adjudicator so it cannot rule. The escrow sits in `disputed`, the abandonment clock runs out, and the 50/50 split hands them half of what they were going to lose entirely.

"Last activity resets the clock" is not enough: filing the dispute resets it once, and then the clock simply runs again from there while the judge stays down. The attack works, one window later.

**The rule instead:** while an escrow is `disputed`, the backstop is *suspended*, and it resumes only after a full window with no activity from EITHER party. A disputed escrow is by definition not abandoned — it is waiting on a third party — and the honest counterparty pushing for a replacement adjudicator (`browserid-pay-6ai6`) is itself activity, so an attacker cannot run the clock out by going quiet.

**What this buys, stated precisely:** it does not guarantee resolution. An attacker who downs the judge and refuses to agree a replacement can still freeze the escrow indefinitely. But they gain NOTHING by it — no 50%, no refund — and a stalemate in which the attacker profits is a very different thing from one in which nobody does. Removing the payoff is the defensible property; guaranteeing liveness against a determined attacker is not something this mechanism can promise.

- [x] suspend while `disputed`; resume only on a full window of silence from both
- [x] make sure "activity" counts a replacement-adjudicator attempt, or the defence has a hole in exactly the case it is for

## Summary of Changes

Built, with the clock simplified by the timing rework.

**When it fires.** `max(2 x the agreement's delivery time, 24h)`, capped at 30 days, measured from the agreement's own latest deadline. That deadline is the LAST moment the delivery clock could have expired — an early delivery pulls the clock in — so measuring from it always errs late, which is the right direction for the one mechanism here that moves money with nobody watching. It also means the custodian needs no chain read and no copy of the obligation logic.

Note: with a 30-day ceiling the cap binds for any agreement longer than 15 days, so the scaling is really only visible below that.

**What it pays.** The price splits 50/50; odd pennies and the unused adjudicator fee go back to the payer, whose money it was; the custodian fee is earned; the contribution is paid as a term of the agreement. The same settlement shape as everywhere else.

**A dispute suspends it** — `backstop_due` returns `None` for anything not `locked`. Without that, attacking the adjudicator would be a strategy: stall the ruling, let this expire, collect half of what you were going to lose outright.

**The sweeper** runs hourly in the custodian and logs loudly. Hourly is ample for a clock measured in days, and this is the only unattended money movement in the system.

Not needed in the end: a `last_activity` column, and the configurable-vs-fixed question — timings are fixed by the protocol for now (`browserid-pay-b3gq`).

Tests: the bounds (floor holds a 6h agreement at 24h, ceiling caps a 60-day one at 30 days, scaling visible in between, no deadline falls to the floor); a dispute returning `None`; the sweep touching only what is actually due and never a disputed escrow; and that sweeping is not repeatable.
