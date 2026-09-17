---
# browserid-pay-6ai6
title: An unresponsive adjudicator freezes the escrow with no way out
status: draft
type: bug
priority: high
created_at: 2026-09-17T20:20:27Z
updated_at: 2026-09-17T20:25:55Z
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

## Waiting period, and its interaction with the backstop (Dan, 2026-09-17)

A judge must be given time before being declared unresponsive, or a party could start the replacement dance the moment a ruling takes longer than they like — which pressures a slow but honest adjudicator.

**Shape:** a `ruling_window` (a term, like `response_window`) running from when the dispute is filed. Only after it lapses may either party post a signed `judge_unresponsive` claim on chain, which is what opens replacement.

**Critically, this must interact with the abandonment backstop** (`browserid-pay-ov7g`). Otherwise attacking the adjudicator becomes a strategy: a party facing a loss downs the judge, the escrow sits in `disputed`, the abandonment clock expires and they collect 50% instead of nothing. The backstop is therefore suspended while an escrow is disputed and resumes only after a full window of silence from BOTH parties — and a replacement attempt counts as activity, so the honest side can keep the suspension alive on its own.

Worst case remains a frozen escrow if the parties cannot agree a replacement. That is acceptable where a profitable attack is not: the attacker gains nothing by freezing it.

- [ ] `ruling_window` as a term, with a sensible default and a floor
- [ ] a signed `judge_unresponsive` claim, permitted only after it lapses
- [ ] replacement requires BOTH parties; the escrowed fee follows whoever actually rules
