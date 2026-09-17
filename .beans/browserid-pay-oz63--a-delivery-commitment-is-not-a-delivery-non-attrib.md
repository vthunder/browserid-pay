---
# browserid-pay-oz63
title: 'A delivery commitment is not a delivery: non-attributable withholding'
status: completed
type: bug
priority: high
created_at: 2026-09-17T16:43:19Z
updated_at: 2026-09-17T20:38:09Z
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

- [x] decide whether delivery-met requires availability, a receipt, or both — a RECEIPT (an accepting acknowledgement), built in browserid-pay-ibnl
- [x] agents publish an encryption key — split into browserid-pay-ifz9
- [x] judge: what a commitment-only delivery is worth — split into browserid-pay-lwo3
- [x] say in the terms which regime applies — split into browserid-pay-ifz9

## Summary of Changes

Closed 2026-09-17: the mechanism chosen here is built, and what is left has moved to beans of its own.

**What shipped** (`browserid-pay-ibnl`), and it is not the option this bean recommended. The suggestion was (4) — nudge the judge — plus (1) as the real fix. What we actually built was closer to (2), the receipt mirror, which this bean had dismissed as "moves the non-attributability rather than removing it". That objection was right about a receipt ALONE and wrong once a clock and a default are attached:

- A delivery commitment starts the recipient's response window. Met now requires an accepting acknowledgement — the handoff/receipt asymmetry this bean identified is closed.
- Silence settles nothing. It leaves the obligation `unanswered` and the escrow LOCKED. This is the part that most damages the withholding gamble: a withholder can no longer be paid by outlasting an inattentive recipient, and must file, reveal, and face the rule already in the ruling prompt that an unopened commitment is undelivered.
- Repeated "I never got it" cannot stall: an absolute resolution window runs from the FIRST delivery and never restarts, after which the arrival argument is `stalled` and only an adjudicator can end it.

**What remains, and where it went:**

- Encryption key agreement, and saying which delivery regime an agreement is under → `browserid-pay-ifz9`. That is still the structural fix: a cure is no more verifiable than the original delivery, so the ledger looks identical whether the deliverer is withholding or the recipient is lying. What we built denies both sides a payoff for stalling; it does not decide who is honest.
- What the judge should make of a commitment-only delivery when receipt is denied → `browserid-pay-lwo3`, which also covers the discovery that the ruling prompt has never been told the response-window model exists at all.
- Option (3), the relay attestation, was rejected: weaker than sealed delivery, trusts the relay, and bypassable by claiming out-of-band delivery.
