---
# browserid-pay-7bzw
title: 'Negotiation: propose/accept is binary, there is no counter-offer'
status: draft
type: feature
priority: normal
created_at: 2026-09-17T15:01:52Z
updated_at: 2026-09-17T15:01:52Z
---

The protocol is `propose → accept`, and `accept(id, terms)` is all-or-nothing: the acceptor signs the proposer's exact terms or does not. There is no counter-offer, no amendment before signature, no way to say "yes but $75".

This is why the agents never negotiate, despite having a relay that can carry arbitrary messages between them. It is not agent incuriosity and it is not the instructions — the API gives them one move. `next_step` reinforces it: the only outcomes it knows for a proposal are `accept` and `none`, and its `then` hints say "tell them you accepted".

Observed 2026-09-17: two agents exchanged messages only to hand over an id and terms JSON, never to discuss them.

**What is missing is a non-binding move.** Everything on chain today is committal. There is no way to say something that matters but does not yet bind — which is what a counter-offer is.

**Open: how much of this needs to be on chain at all.** Terms only become binding at the proposal, so bargaining could happen entirely over the relay in prose, with the chain recording only the agreed result. Against that: if the chain records nothing about why terms are what they are, an adjudicator ruling on a dispute cannot see what was represented during bargaining. A middle option is that a proposal references the quote or message thread it came from. See `browserid-pay-0igh` for the structure question.

**Touchpoints:**
- *Structure* (`0igh`) — a counter-offer in prose vs a typed quote object.
- *Discovery* (`h4mu`) — quotes against a broadcast need are also price discovery: what a job actually costs becomes observable. Alice picked $50 out of the air.
- *Amendments* (`b8og`) — the same problem after signature rather than before; likely related machinery.
- *Reputation* — bargaining behaviour is not currently attestable, and probably should not be; only settlement is.

- [ ] can a counter-offer stay off chain entirely, or must the proposal reference what preceded it?
- [ ] does the adjudicator need the bargaining history to rule fairly?
- [ ] `next_step` needs a third answer for a proposal: accept / counter / decline
- [ ] declining is also missing — a proposal can only expire, never be refused
