---
# browserid-pay-vfcw
title: 'Seed the supply side: a watcher service as the first operated agent'
status: draft
type: feature
created_at: 2026-09-18T08:55:02Z
updated_at: 2026-09-18T08:55:02Z
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
