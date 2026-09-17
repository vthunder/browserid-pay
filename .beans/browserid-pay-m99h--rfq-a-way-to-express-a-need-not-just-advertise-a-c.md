---
# browserid-pay-m99h
title: 'RFQ: a way to express a need, not just advertise a capability'
status: draft
type: feature
priority: normal
created_at: 2026-09-17T14:51:03Z
updated_at: 2026-09-17T14:51:03Z
---

Every discovery system surveyed — A2A, the marketplaces built on it — is provider-advertises-then-client-queries. Confirmed against the A2A spec 2026-09-17: there is no broadcast-a-need primitive anywhere in it. An agent can say "I summarize papers"; no agent can say "I need ten papers summarized, who wants it, what would you charge?"

That single absence is three of our gaps at once:

**`goal` has nowhere to live.** `next_step` takes a `goal` parameter that is discarded unless nothing is open (`browserid-pay-9v18` era code) — so a loop carrying a goal is silently ignored while any agreement is in flight. Intent is currently re-stated by the model every tick and stored nowhere. An RFQ is a real object for it, local first and publishable when the human wants.

**No negotiation.** The protocol is `propose → accept`, and `accept(id, terms)` is binary: sign the proposer's exact terms or do not. There is no counter-offer, so the agents never bargain — not agent incuriosity, the API gives them one move. A quote responding to an RFQ is a counter-offer that is not yet binding.

**No price discovery.** Alice picked $50 out of the air. Quotes against an RFQ are the market rate, observable.

## Shape to explore

- `rfq.v1`: what is wanted, by when, budget or none, who may answer (open / named / reputation-gated). Local draft → optionally published.
- `quote.v1`: a response — price, terms, expiry — binding only if the requester turns it into a proposal.
- A proposal then references the quote it came from, so the chain records why the terms are what they are.
- Where do RFQs live? Options: an `/rfq/` path on our chain; a mingo community aimed at agents (it already has communities, identities and signed posts — a `request.v1` post would be human-readable AND machine-matchable); or an A2A extension if one fits.
- Spam is the obvious failure mode of any open board, and reputation cannot bootstrap it for new agents. Parked deliberately (Dan, 2026-09-17): defer sybil resistance until it is an actual problem.

## Why it is worth doing even before distribution

It fixes the `goal` flaw and gives the agents something to talk about over the relay they already have but never use for negotiation. Useful between two agents that already know each other, before any discovery surface exists.
