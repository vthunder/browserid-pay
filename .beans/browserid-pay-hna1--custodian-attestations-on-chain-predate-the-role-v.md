---
# browserid-pay-hna1
title: Custodian attestations on chain predate the role / value_band fields
status: todo
type: task
priority: normal
created_at: 2026-09-16T16:25:30Z
updated_at: 2026-09-16T16:31:32Z
---

The reputation work (82b9e44) added `role`, `value_band` and `currency` to the custodian's settlement attestations (`crates/custodian/src/publish.rs:67-78`), and the indexer's scorecard consumes `value_band`. No attestation carrying those fields exists on the live chain yet: every one indexed at index.dsp.fyi is the older two-field shape.

Verified 2026-09-16 via `/v1/attestations?subject=danmills+bob@sandmill.org`:

    agreement:settled  block 3889017  {"contract": "agr_BE7KDd8YVDPIrJQb", "outcome": "acknowledged"}
    agreement:disputed block 3886486  {"contract": "agr_eHpuhyFqtpuabaAj", "outcome": "ruled:payer"}
    agreement:ruled    block 3886486  {"contract": "agr_eHpuhyFqtpuabaAj", "outcome": "payer"}

Timing explains it rather than a bug — the newest attestation was issued 2026-09-16 11:54 UTC, 82b9e44 landed 15:18 CEST the same day — but it means the new shape is unexerised end to end, and it is worth confirming whether the deployed pay.dsp.fyi is actually running a build that includes it.

- [x] confirm the deployed custodian image includes 82b9e44 (yes; released 15:49 in run 35117887462)
- [ ] settle one agreement and check the attestation carries role / value_band / currency
- [ ] check the indexer scorecard reads the band off a real attestation (not just its tests)

## Narrowed 2026-09-16

The deployment question is answered: the deployed pay.dsp.fyi **does** include the value-band commit. `deploy-pay` released 72152f5 at 15:49 (run 35117887462, success), and 82b9e44 is an ancestor of it. So this is purely a timing artifact — the newest on-chain attestation was issued 11:54 UTC, before the commit landed — and not drift.

What remains is only to exercise it once.
