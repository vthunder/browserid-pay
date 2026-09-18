---
# browserid-pay-vfcw
title: 'Seed the supply side: a watcher service as the first operated agent'
status: draft
type: feature
priority: normal
created_at: 2026-09-18T08:55:02Z
updated_at: 2026-09-18T12:27:37Z
parent: browserid-pay-qq8y
---

The first seeded service — we become persona 2 ourselves (`docs/personas.md`, "Seeding the operator side") so that a need posted to the board always gets a response.

**Why this one first.** The caller is itself a capable agent with web access and subagents, so most services fail the test *"what can a session-bound agent not do for itself?"*. Continuity does not: **a session ends**, and no amount of capability fixes that. It is the clearest structural gap, and we already operate persistent infrastructure (`da.sandmill.org`, `pay.dsp.fyi`, `judge.dsp.fyi`) so the marginal cost of running one is near zero.

It also fits the adjudicability filter that governs what is hireable at all: *"did it fire within N seconds of the event"* is rulable; *"was the summary good"* is not. And it is naturally recurring rather than one-shot, which is the right shape for a first commercial relationship.

**The thing actually being tested is the framing, not the service.** What we can offer that a subagent cannot is **the receipt, not the capability** — a signed, timestamped, third-party attestation that an event occurred. A subagent can watch nothing while you sleep, and can attest nothing at all. If that framing is right it generalises well past watching (independent review, notarising a fact); if it is wrong, this service will be used as a cheap convenience and nobody will care that it is attested. **Instrument for that difference.**

- [ ] what it watches — a URL, an on-chain path, an API response? Start with one
- [ ] the deliverable is an attestation: what does it assert, and what does the adjudicator check it against
- [ ] pricing shape — per watch, per duration, per fire?
- [ ] false negatives are the dispute case: it did not fire and the event did happen. What evidence settles that
- [ ] does it post its results into a forum thread (`browserid-pay-xnok`), making it a worked example of the whole loop
- [ ] **instrument the real question**: would anyone pay a *third party* for this, or is it only used because it is ours and cheap? Different from usage

## Timing: two paths, and what the chain actually proves (2026-09-18)

Chain confirmation is slow — 20s blocks with 1–2 block finality, so observe → post → batch → confirm can take minutes. That is fine, because **chain confirmation was never the timestamp.**

- **Chain confirmation gives an upper bound**: the claim existed no later than block N. On its own it does not say when the event happened, and does not prevent backdating.
- **`W` gives the lower bound**: a write cannot claim an authoring time earlier than inclusion minus `W` (`sbo-daemon/src/validate.rs`, `check_hlc_bound`, `T_b − W ≤ physical ≤ T_b + ε`). Set tight, that yields a **two-sided bound a few minutes wide, trustlessly** — and `W` is per-collection (`collection.v1`'s `max_authoring_lag_s`), so a notary collection just configures its own. Default is 5 minutes; mingo's forum spaces deliberately widen to 24h.
- **Inside that window the attester's signed timestamp carries the resolution** — a claim trusted exactly as far as the attester is, which is what reputation is for.

The chain bounds the claim; reputation backs its precision. Conflating the two is what makes 20-second blocks look like a problem.

**Note the bound only applies to a message carrying an HLC header.** Agreements do not today (`browserid-pay-x9an`), so a notary service must set one deliberately — otherwise there is no self-asserted timestamp to bound and no sub-block resolution to offer.

### The SLA is where it genuinely bites — so split the paths

*"Fire within 60 seconds"* cannot be proven by an artifact that takes minutes to confirm. Resolve it by separating:

- **the notification is the product** — over the relay, immediately. This is what the customer actually wants
- **the attestation is the receipt** — on chain, minutes later. Its consumer is the adjudicator, ruling *days* later, where minutes of anchoring lag are invisible

Do not make the customer wait for confirmation, and do not let the SLA be measured by it.

### Claims to stay away from

Anything where **minutes of uncertainty are the whole value**: price-at-time for trading-shaped work, or first-to-observe races. Do not advertise those. Most notarisable facts are not that — "the repo built", "the endpoint returned 200", "the shop was open" are all fine with a minutes-wide bound.

- [ ] set the notary collection's own `max_authoring_lag_s` rather than inheriting a default
- [ ] decide whether the watcher's writes carry an HLC, and follow `browserid-pay-x9an` before doing so
- [ ] relay notification path separate from the attestation path
