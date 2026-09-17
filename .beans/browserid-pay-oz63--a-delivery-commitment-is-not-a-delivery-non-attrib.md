---
# browserid-pay-oz63
title: 'A delivery commitment is not a delivery: non-attributable withholding'
status: draft
type: bug
priority: high
created_at: 2026-09-17T16:43:19Z
updated_at: 2026-09-17T16:43:19Z
---

Seen in the haiku run 2026-09-17. Bob posted a delivery commitment on chain but did not send the content. Alice saw the commitment, could not see the haiku, and correctly refused to settle. She messaged him, he sent it, she settled. Everything behaved as built — and that is the problem.

Had bob simply not sent it, he could gamble on adjudication: at the hearing he reveals the haiku, it matches his commitment, and he looks like someone who delivered. **Alice cannot prove a negative.** Withholding is a non-attributable fault. It is the data-availability problem: a commitment proves the content EXISTED, never that it was AVAILABLE.

## Our own model has the asymmetry already

`deriveObligations` (`sdk/agreements/src/agreements.mjs`):

- an **input** is met only when the receiver posts a `receipt` matching the handoff. A handoff alone is `handed_off`, not met.
- a **deliverable** is met as soon as ANY `delivery` evidence exists — the commitment alone.

So the handoff/receipt pattern was built for exactly this and is not applied to deliveries. Committing is treated as delivering.

## Options

1. **Encrypted delivery on chain.** Bob posts the content sealed to alice's public key as the delivery evidence. Availability is then guaranteed by the chain, confidentiality survives, and no new trusted party is introduced. The judge sees exactly what alice could see, so the "I sent it / she is lying" asymmetry disappears; bob encrypting rubbish or to the wrong key becomes attributable. The `seal()` / X25519 machinery already exists for sealed terms to the adjudicator. Costs: the ciphertext is permanent and public (a future key compromise exposes it), and `/agreements/**` is capped at 64 KiB — fine for text, not for a video. Needs agents to publish an encryption key, which the agent directory would carry anyway (`browserid-pay-h4mu`).

2. **Require a receipt for deliverables**, mirroring inputs. Cheap and consistent. But it moves the non-attributability rather than removing it: alice can stonewall a receipt for something she did receive, and her silence is as unprovable as his.

3. **Relay attests delivery on chain** (Dan's sketch). Strictly weaker than (1): it trusts the relay, bob can claim he delivered out of band, and it evidences reaching a mailbox rather than the content matching. More moving parts for less.

4. **Change the judge's disposition** — treat "commitment only, recipient claims non-receipt, no availability evidence" as favouring the recipient. A policy answer, not a mechanism, but bob is gambling ON the judge, so it makes the gamble bad rather than impossible. Cheapest thing that helps, and composes with the others.

5. **Public mode**, which already exists, has none of this problem and none of the confidentiality.

## Suggested shape

(4) now as a mitigation, (1) as the real fix for text-sized deliverables, keeping commitment-only delivery for things too large to publish — and being explicit in the terms about which regime an agreement is under, because the two have genuinely different guarantees.

- [ ] decide whether delivery-met requires availability, a receipt, or both
- [ ] agents publish an encryption key (overlaps the directory bean)
- [ ] judge: what a commitment-only delivery is worth when the recipient denies receipt
- [ ] say in the terms which regime applies, so neither party is surprised
