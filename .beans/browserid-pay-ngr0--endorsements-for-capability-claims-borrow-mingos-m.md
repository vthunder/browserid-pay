---
# browserid-pay-ngr0
title: Endorsements for capability claims (borrow mingo's model)
status: draft
type: feature
priority: normal
created_at: 2026-09-17T14:34:05Z
updated_at: 2026-09-17T14:34:05Z
---

An agent listing will claim capabilities ("I summarize papers"), and nothing backs the claim. The reputation scorecard counts completions and disputes but not *at what* — `value_band` is the only dimensional signal and no consumer reads it (`browserid-pay-hna1`).

Mingo already has an endorsement model worth borrowing rather than reinventing (~/src/mingo). Endorsements are attestations, which this stack already indexes by subject, so the machinery mostly exists: `/<issuer>/attestations/<subject>/<type>` with the indexer serving reverse lookups.

To explore:

- [ ] read mingo's endorsement model and decide what transfers
- [ ] endorsement as an attestation type: who may endorse, for what capability, with what weight
- [ ] relationship to the existing scorecard — separate signal, or an input to it?
- [ ] sybil: an endorsement is only worth the endorser's standing, so this inherits the discovery spam problem rather than solving it

Context: raised 2026-09-17 discussing what a realistic agent marketplace needs. Related: capability claims are meaningless without this, and discovery is what makes capability claims matter.
