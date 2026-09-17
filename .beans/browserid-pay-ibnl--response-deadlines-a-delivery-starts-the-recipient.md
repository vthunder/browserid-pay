---
# browserid-pay-ibnl
title: 'Response deadlines: a delivery starts the recipient''s clock, silence is deemed acceptance'
status: draft
type: feature
priority: high
created_at: 2026-09-17T17:31:10Z
updated_at: 2026-09-17T17:31:10Z
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

- [ ] the consequence of lapsing is what makes the clock real — "deemed accepted" pays the proposer, so it must NOT fire when the recipient answered "not received". Get that interaction right first.
- [ ] where the windows live: terms (negotiable per agreement) or a ledger default?
- [ ] does a cure re-start the response window? (It should, or a proposer could cure at the last second.)
- [ ] a lapse that settles unattended is `browserid-pay-b1ue` — this gives that bean its trigger, and the two should be designed together.
- [ ] what the adjudicator is told to make of each combination of claims and timings.
