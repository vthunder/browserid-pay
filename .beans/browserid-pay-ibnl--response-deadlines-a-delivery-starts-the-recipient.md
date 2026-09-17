---
# browserid-pay-ibnl
title: 'Response deadlines: a delivery starts the recipient''s clock, silence is deemed acceptance'
status: completed
type: feature
priority: high
created_at: 2026-09-17T17:31:10Z
updated_at: 2026-09-17T19:29:00Z
---

The mitigation for non-attributable withholding (`browserid-pay-oz63`) that needs no cryptography (Dan, 2026-09-17).

## Today the recipient has no clock at all

`acknowledgement` is only evidence that flips the stage to `settled`. There is no obligation on the recipient, no deadline, nothing that lapses. After a delivery commitment the deliverable is already `met` (`deriveObligations`), so no clock is running on either side — the agreement just sits. That is why a withholding proposer can stall indefinitely and why the counterparty's non-receipt is only ever silence.

## The shape

A delivery commitment starts a SHORT response window on the recipient, who must do one of:

- **settle** — accept and release;
- **not received** — a signed, timestamped claim that the content never arrived;
- **does not meet requirements** — accept receipt, contest quality.

If the window lapses with no response: **deemed accepted**. If the answer is "not received", the clock moves back to the proposer to cure (re-send) within a second window; failing to cure is then an attributable failure.

## Why this mitigates without encryption

It does not stop a party lying, but it removes silence as a free option on both sides, and it converts "absence of evidence" into a dated record:

- A withholding proposer can no longer stall: the recipient must respond quickly, and their non-receipt claim is on chain, timestamped, immediately after the commitment. At adjudication the judge sees a prompt claim and a proposer who never cured — revealing the content at the hearing then looks like withholding, not delivering. The gamble gets much worse.
- A stalling recipient can no longer sit on delivered work: silence pays the proposer.
- A recipient who falsely claims non-receipt is cheaply refuted — the proposer just re-sends — and a repeated pattern of it is itself evidence.

## The machinery already exists, one side only

Inputs do this: `handoff` → `receipt`, and a receipt carrying `ok: false` marks the obligation `contested`. Deliveries have no equivalent. This is the same pattern applied to the other direction, plus a clock.

## Open

- [x] the consequence of lapsing is what makes the clock real — "deemed accepted" pays the proposer, so it must NOT fire when the recipient answered "not received". Get that interaction right first.
- [x] where the windows live: terms (negotiable per agreement) or a ledger default?
- [x] does a cure re-start the response window? (It should, or a proposer could cure at the last second.)
- [ ] a lapse that settles unattended is `browserid-pay-b1ue` — this gives that bean its trigger, and the two should be designed together.
- [ ] what the adjudicator is told to make of each combination of claims and timings.

## Summary of Changes

Built. A delivery commitment now starts the recipient's clock, and silence past it is acceptance.

**The state machine**, derived from the LATEST delivery and any answer after it: `pending` → `delivered` (clock running, `response_due` exposed) → `met` (accepted, or deemed accepted at the deadline) / `contested` (refused, with `not_received` or `not_met`). A cure is simply a later delivery — it becomes the latest and the clock restarts, so no new verb was needed.

**`response_window`** is a CLEAR field on the proposal (ISO-8601, default P2D). It has to be clear: the custodian decides releases from it and cannot read sealed terms.

**`settle(id, {ok, reason})`** — accepting releases as before; refusing posts evidence and releases NOTHING, and must say which refusal it is, because they are different complaints: `not_received` is the deliverer's to cure, `not_met` belongs to the adjudicator.

**`claim_lapsed`** — the payee asks the custodian to release. The custodian re-derives the whole claim from the ledger: latest delivery attributed to the claimant, no answer after it, window elapsed. It is a request to CHECK, never an assertion to believe.

## Two things the work turned up

**`status()` settled on any acknowledgement**, including a refusal — so a refused delivery looked settled and vanished from both parties' view. Only an accepting acknowledgement settles now.

**Wall-clock ordering was wrong.** `at` is writer-claimed with second resolution, and a cure routinely shares a timestamp with the refusal it answers, which made the stale refusal look like an answer to the cure. Ordering is now by ledger position. The custodian is deliberately stricter than the SDK: it disregards an acknowledgement only when it is older by BOTH measures, because the unsafe direction there releases money to someone who withheld and then waited.

## Guarantees

- A withholder cannot stall: the recipient must answer quickly, and their `not_received` lands on chain, timestamped, right after the commitment. At adjudication the judge sees a prompt claim and a deliverer who never cured.
- A recipient cannot sit on delivered work: silence pays the deliverer.
- A false `not_received` is cheaply refuted — the deliverer just delivers again.

Does not stop either party lying; it removes silence as a free option and turns absence of evidence into a dated record.

Tested: the safety property both sides (a refusal stops deemed acceptance in the obligations model AND at the custodian), cure restarts, legacy acknowledgements with no `ok` still accept, `next_step` driving both sides through refuse → cure → accept, and a refusal releasing nothing.

## Still open

- `browserid-pay-b1ue` (unattended settlement) now has its trigger, but nothing sweeps: a lapse releases only when the payee CLAIMS it. That is deliberate — no background job, and the payee has the incentive — but an escrow whose payee never returns still sits.
- What the adjudicator makes of each combination of claims and timings is unchanged; the judge prompt has not been told about response windows yet.

## Addendum 2026-09-17: the absolute resolution clock

The first cut let a cure restart the recipient's window, which made repeated "I never got it" an unbounded stall (Dan spotted it).

Fixed by separating the two questions a recipient can answer:

- **Did it arrive?** An unresolved arrival runs against a `resolution_window` measured from the FIRST delivery and never restarted by a cure. When it runs out the obligation goes `stalled`. Default: three response windows; settable as a clear proposal term.
- **Is the work any good?** `not_met` acknowledges receipt, so the arrival clock is behind us and it stays an ordinary contest for as long as it takes.

Silence is still acceptance regardless of the absolute clock — this bounds an argument, it does not punish a quiet recipient.

**A stalled arrival pays nobody.** The custodian still refuses `claim_lapsed` while any refusal stands, however stale; the only way out is the adjudicator, whose fee is already escrowed, so an honest recipient is not out of pocket for defending themselves. Releasing automatically here would hand the money to whoever waited longest, which is the failure the whole mechanism exists to prevent.

`next_step` sends the deliverer to `dispute` rather than to a payout.

**The limit of this, stated plainly**: a cure is no more verifiable than the original delivery, so the chain records the same objects whether the deliverer is withholding and curing on paper or the recipient is lying about non-receipt. The clock bounds the argument and denies either side a payoff for stalling; it does not decide who is telling the truth. That needs `browserid-pay-ifz9` (sealed delivery on chain), after which a cure is checkable and the question becomes decidable. Accepted knowingly: the tie-break at adjudication favours the party holding verifiable artifacts.
