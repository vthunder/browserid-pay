---
# browserid-pay-rlun
title: 'Other proposal models: open-ended, unaddressed, first-N-to-accept'
status: draft
type: feature
priority: normal
created_at: 2026-09-17T22:08:52Z
updated_at: 2026-09-17T22:08:52Z
---

A proposal today names exactly one acceptor and stands until it expires. That is the only shape there is, and it forces the proposer to know who they want before they can express what they want.

Shapes worth exploring:

- **Unaddressed** — a proposal with no named acceptor, open to whoever qualifies. The machinery half exists: a reputation-gated grant already says "any agent rated >= N" instead of naming a counterparty (`browserid-pay-9nq1`), and `covers()` understands `"*"` and `"*@domain"`. What is missing is the proposal itself being addressed to nobody.
- **First to accept wins** — the natural pairing with unaddressed. Needs a race rule; first-valid-write-wins at a `(path, id)` slot may already give it for free, since only one acceptance can occupy the slot.
- **First N to accept** — the same job offered to several agents, all of whom are bound. Harder: N escrows from one proposal, and a cap that has to cover all of them.
- **Open-ended** — no expiry, or a standing offer that stays open until withdrawn. Withdrawal does not exist at all today (a proposal can only expire), which `browserid-pay-7bzw` also notes.

Questions this raises:

- [ ] does an unaddressed proposal need a different object, or just an absent `parties.acceptor`?
- [ ] what stops a hundred agents accepting at once, and what do the losers see?
- [ ] the contract warrant caps ONE agreement's total; first-N means the cap must cover N, and the human should see that on the card
- [ ] withdrawal: an unaddressed offer that cannot be retracted is a standing liability
- [ ] this is the same primitive as broadcasting a need (`browserid-pay-h4mu`) approached from the other end — an unaddressed proposal IS a posting. Decide whether they are one mechanism or two.

Raised 2026-09-18 while settling the agreement timing model.
