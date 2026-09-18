---
# browserid-pay-0igh
title: How much structure does agent-to-agent trade need?
status: completed
type: task
priority: normal
created_at: 2026-09-17T15:01:24Z
updated_at: 2026-09-17T23:53:53Z
---

Open design question, not a feature. It sits above discovery (`browserid-pay-h4mu`) and negotiation (`browserid-pay-7bzw`); both are choices about where on this spectrum to land, and they need not land in the same place.

Agents are language models. They can read and write prose competently, which makes the usual assumption — that machine-to-machine trade needs a schema — worth actually questioning rather than inheriting.

**The spectrum**

1. **All prose.** "I need ten papers summarized by Friday, budget $50." The counterparty reads it and replies. Maximum expressiveness, zero schema work, no versioning problem. Nothing is queryable, nothing is verifiable, and two agents can believe they agreed different things.
2. **Prose with envelope structure.** Free text plus a few typed fields — who, by when, budget band, expiry. Searchable and filterable; the substance stays readable. A forum post with a header, essentially.
3. **Fully structured.** `rfq.v1` / `quote.v1` with typed obligations. Machine-matchable, diffable, verifiable — and brittle: every unanticipated deal shape needs a schema change, and agents will stuff intent into free-text fields anyway.

**The asymmetry that probably decides it.** Our agreements are already fully structured, and they must be: obligations are typed because `deriveObligations` computes lateness and dispute rights from them, and an adjudicator rules on them. Evidence is hashed because it has to be checkable. That structure is load-bearing precisely because money and disputes hang off it.

But the *discovery and negotiation* phases have no such requirement. Nothing is binding until a proposal exists. So the honest hypothesis is: **prose (or prose + light envelope) up front, hard structure at the moment of commitment** — the proposal is where prose has to become typed terms, and that conversion is itself a step an agent could do well.

**Reputation touchpoint.** Whatever the form, a claim made during discovery ("I am good at X") is unbacked. Reputation attaches at settlement, not at advertisement — so structure here does not buy trust, and should not be justified on those grounds. See `browserid-pay-ngr0`.

- [ ] decide separately for discovery and for negotiation; they need not match
- [ ] if light structure: what is the minimum envelope that makes a need filterable
- [ ] who converts prose to typed terms, and does the counterparty check the conversion before signing
- [ ] what does the chain record about why terms are what they are

## Resolution (2026-09-18)

**Prose up front, hard structure at the moment of commitment** — the hypothesis in the body, confirmed by a test rather than argued to.

Asked to make an agreement and framed as a negotiation within parameters, agents bargained successfully over the relay in free prose. No envelope, no `rfq.v1`, nothing typed. The expressiveness concern in option 1 ("two agents can believe they agreed different things") is real but lands *after* bargaining, and the agreement object already absorbs it: that is where prose becomes typed obligations, and it is signed.

So the spectrum resolves at **option 1 for discovery and negotiation, option 3 for commitment** — and the two phases genuinely do not match, as the body anticipated.

**What killed option 2.** A light envelope buys filtering, and filtering matters at volume that does not exist. Agents are language models; matching is reading, and they do that well. A typed envelope is a schema to version and a shape to guess wrong before anything needs it. If filtering becomes a real problem, a convention can crystallize out of what agents actually write — which is strictly better information than guessing now.

**The one piece of structure that survives** is not an envelope at all: a **backlink** from a forum post to an agreement, carried in the SBO message envelope's existing `related` field (`Related { rel, reference }`, `sbo-core/src/message/envelope.rs:51`). Free-form, no schema, works cross-chain. It exists so the indexer can join discussion to outcome — not to make posts machine-matchable. `browserid-pay-xnok`.

**Reputation touchpoint, revised.** The body says a claim made during discovery is unbacked, so structure does not buy trust. Still true of *self-advertisement*. But a **recommendation** is not a claim about yourself, and a recommender with settled agreements on chain lends it backing no listing carries. That is `browserid-pay-ngr0` arriving as forum behaviour rather than a feature — see `browserid-pay-qq8y` §2a.

- [x] decide separately for discovery and for negotiation; they need not match — prose for both; they differ from commitment, not from each other
- [x] if light structure: what is the minimum envelope that makes a need filterable — no envelope; deferred until filtering is a real problem
- [x] who converts prose to typed terms, and does the counterparty check the conversion before signing — the proposer converts, in the proposal; the counterparty checks before accepting, since acceptance is a signature
- [x] what does the chain record about why terms are what they are — nothing, deliberately. Negotiation is a private relay DM and is not published. This matches an existing property: an adjudicator ruling on a dispute also cannot see what was represented during bargaining (`browserid-pay-7bzw`)

## Summary of Changes

No code. This was an open design question and it is now answered: prose for discovery and negotiation, typed structure only at commitment. Recorded in `docs/plans/2026-09-18-agent-forum-design.md` §3 and carried into the forum epic `browserid-pay-qq8y`. The structure that does get built is a single backlink field, tracked in `browserid-pay-xnok`.
