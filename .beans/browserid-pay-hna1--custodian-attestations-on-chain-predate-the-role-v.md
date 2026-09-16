---
# browserid-pay-hna1
title: Custodian attestations on chain predate the role / value_band fields
status: completed
type: task
priority: normal
created_at: 2026-09-16T16:25:30Z
updated_at: 2026-09-16T22:55:39Z
---

The reputation work (82b9e44) added `role`, `value_band` and `currency` to the custodian's settlement attestations (`crates/custodian/src/publish.rs:67-78`), and the indexer's scorecard consumes `value_band`. No attestation carrying those fields exists on the live chain yet: every one indexed at index.dsp.fyi is the older two-field shape.

Verified 2026-09-16 via `/v1/attestations?subject=danmills+bob@sandmill.org`:

    agreement:settled  block 3889017  {"contract": "agr_BE7KDd8YVDPIrJQb", "outcome": "acknowledged"}
    agreement:disputed block 3886486  {"contract": "agr_eHpuhyFqtpuabaAj", "outcome": "ruled:payer"}
    agreement:ruled    block 3886486  {"contract": "agr_eHpuhyFqtpuabaAj", "outcome": "payer"}

Timing explains it rather than a bug — the newest attestation was issued 2026-09-16 11:54 UTC, 82b9e44 landed 15:18 CEST the same day — but it means the new shape is unexerised end to end, and it is worth confirming whether the deployed pay.dsp.fyi is actually running a build that includes it.

- [x] confirm the deployed custodian image includes 82b9e44 (yes; released 15:49 in run 35117887462)
- [x] settle one agreement and check the attestation carries role / value_band / currency
- [x] check the indexer scorecard reads the band off a real attestation — it does not; the read side was never built (see below)

## Narrowed 2026-09-16

The deployment question is answered: the deployed pay.dsp.fyi **does** include the value-band commit. `deploy-pay` released 72152f5 at 15:49 (run 35117887462, success), and 82b9e44 is an ancestor of it. So this is purely a timing artifact — the newest on-chain attestation was issued 11:54 UTC, before the commit landed — and not drift.

What remains is only to exercise it once.

## Summary of Changes

Verified on the live chain 2026-09-17, after a full prod walkthrough. No code changed here — the gap was that the new attestation shape had never been exercised.

**The fields are live.** `agreement:settled` at block **3890986** carries all three, for both parties:

    alice  {"role": "payer", "value_band": "10-100", "currency": "USD", "contract": "agr_oygUXMwVfS6WXkA6", "outcome": "acknowledged"}
    bob    {"role": "payee", "value_band": "10-100", "currency": "USD", ...}

Confirms the timing explanation in the bean: not deployment drift, just no settlement since 82b9e44 shipped.

**The walkthrough also proved the contribution floor on the real SDK path** — the proposal was accepted under the root policy published the day before, which the synthetic `policy-probe` (sys-signed) could not confirm.

## Correction

This bean claimed "the indexer scorecard consumes `value_band`". **It does not.** `value_band` appears only in `crates/custodian/src/publish.rs` (computed, emitted, unit-tested); there is no reference anywhere in `crates/indexer/`. Bob’s live scorecard confirms it — `role` appears, `value_band` and `currency` do not, and the score is built from completion / dispute_record / promptness / responsiveness / tenure only.

Not a regression. The reputation draft (`docs/plans/2026-09-16-reputation-design-draft.md:103`) decides only that the attestation *carries* a band and that it "lets volume mean something"; it never specifies how a score should weight it. The emit side was built, the read side never was.

**Open question, deliberately deferred (Dan, 2026-09-17): should volume affect the reputation score, and how?** A scoring-policy decision, not a bug — ten `gte10k` settlements arguably should not rank identically to ten `lt10` ones, but weighting by value also makes a score easier to buy. Revisit with real volume, which is what the draft says about decay rules too.
