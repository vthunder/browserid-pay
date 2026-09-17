---
# browserid-pay-ov7g
title: 'Abandonment backstop: split an escrow nobody has claimed'
status: draft
type: feature
priority: normal
created_at: 2026-09-17T20:20:27Z
updated_at: 2026-09-17T20:20:27Z
---

When BOTH parties abandon an agreement the escrow stays locked for ever. Nobody is wronged — each had an exit and did not take it — but the custodian carries the funds indefinitely, and held funds are a real liability (Dan, 2026-09-17): they can be stolen, and they sit on the books with no prospect of resolution.

## Why a 50/50 split rather than a refund

A refund to the payer looks like the "fair" default — it matches the adjudicator rule that an unopened commitment is undelivered. It is the wrong choice here: it would mean *take the work, go quiet, get your money back*, which is precisely the stalling payoff this project keeps having to remove.

50/50 is deliberately arbitrary. It denies BOTH parties a full win for disappearing, which is the property that matters when the mechanism fires without anyone watching.

**The incentive problem is bounded by requiring both sides to be silent.** For the backstop to fire, the party in the right must ALSO have abandoned — and they have a faster, better remedy available the whole time (dispute, and win outright). Stalling to collect half is only attractive to someone who expects to lose a ruling, and it only works if their counterparty gives up too.

## Shape to work out

- [ ] what the clock runs from: last on-chain activity by either party, not the lock, so any action resets it. A truly abandoned agreement is one with no activity at all.
- [ ] how long: Dan suggested ~2x the agreement length. `latest_deadline` already reaches the custodian at lock, so the duration is known. Needs a FLOOR regardless, or a malicious proposer sets a short one and games it.
- [ ] configurable-but-required, or fixed by the ledger? A clear proposal term with a minimum enforced by the root policy would match how the contribution floor works.
- [ ] the non-price components: custodian fee earned (it did hold the funds), adjudicator fee refunded (no adjudication happened — consistent with `acknowledge`), contribution paid (it is a term of the agreement), and only the PRICE split.
- [ ] this is the one case that genuinely needs a sweeper: by construction nobody is there to trigger it. A slow periodic job at the custodian, which is where the liability sits.
- [ ] say it in the consent card, since it is a way a human can lose money without anyone deciding anything.
