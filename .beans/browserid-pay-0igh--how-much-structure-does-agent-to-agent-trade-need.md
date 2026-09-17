---
# browserid-pay-0igh
title: How much structure does agent-to-agent trade need?
status: draft
type: task
priority: normal
created_at: 2026-09-17T15:01:24Z
updated_at: 2026-09-17T15:01:24Z
---

Open design question, not a feature. It sits above discovery (`browserid-pay-6t2d`) and negotiation (`browserid-pay-wqx4`); both are choices about where on this spectrum to land, and they need not land in the same place.

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
