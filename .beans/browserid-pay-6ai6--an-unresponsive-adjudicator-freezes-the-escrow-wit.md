---
# browserid-pay-6ai6
title: An unresponsive adjudicator freezes the escrow with no way out
status: draft
type: bug
priority: high
created_at: 2026-09-17T20:20:27Z
updated_at: 2026-09-17T20:20:27Z
---

A disputed escrow can only leave that state through a ruling (`escrow.rs`: `dispute` requires `locked`, and only `ruling` moves it on). If the named adjudicator never rules — dead, unreachable, or simply ignoring it — the funds are frozen permanently and NEITHER party can do anything about it.

Worse than the abandoned-agreement case (`browserid-pay-ov7g`), because it happens while both parties are present, waiting, and doing everything right.

## Direction (Dan, 2026-09-17)

**Both parties agree on a replacement adjudicator.** Agreement by both is what makes it safe: neither side can unilaterally shop for a friendlier judge, which is the obvious way this could be abused.

Shape to work out:

- [ ] a `replace_adjudicator` object under the agreement, effective only when BOTH parties have signed it (two objects, or one countersigned)
- [ ] the custodian accepts a ruling from the replacement only after seeing both signatures — it already reads the chain, so this is a read plus a check
- [ ] does the original adjudicator forfeit the escrowed fee? (It has not ruled. The fee should presumably follow whoever does the work.)
- [ ] is there a waiting period before replacement is allowed, so a slow-but-alive adjudicator is not swapped out mid-deliberation? The judge already has a response window for the counterparty; something similar for its own ruling.
- [ ] what if the parties cannot agree on a replacement — does it fall back to the abandonment backstop, or stay frozen?

Related: the adjudicator's own liveness is currently unattested. A provider listing says nothing about whether the adjudicator still answers, which is also a discovery problem (`browserid-pay-h4mu`) and arguably something reputation should carry (`browserid-pay-ngr0`).
