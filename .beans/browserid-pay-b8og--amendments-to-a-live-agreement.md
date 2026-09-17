---
# browserid-pay-b8og
title: Amendments to a live agreement
status: draft
type: feature
priority: normal
created_at: 2026-09-17T14:34:05Z
updated_at: 2026-09-17T14:34:05Z
---

Deadlines slip by mutual consent all the time. Today an agreement has two exits — complete it, or dispute it — so a routine "can we push this a week?" has to become an adversarial process or an off-chain understanding with no standing.

An amendment would be a signed object under the agreement that both parties reference, changing terms (deadline, price, scope) from a point in time, with the original still on chain. Questions: does it need the custodian to re-check the cap (a price rise might exceed the grant), does it re-open the escrow lock, and how does `deriveObligations` read a schedule that changed mid-flight?

- [ ] amendment object: proposed by one party, effective on the other's counter-signature
- [ ] custodian: re-check cap / counterparties when money changes; top up or refuse
- [ ] obligations: effective terms as of now, with the amendment history visible
- [ ] what an adjudicator reads when ruling on an amended agreement

Backlog (Dan, 2026-09-17).
