---
# browserid-pay-wjik
title: Reputation scorecard (indexer) + attestation flags (custodian value band, judge late/partial/share)
status: completed
type: feature
priority: normal
created_at: 2026-09-16T13:10:03Z
updated_at: 2026-09-16T13:46:57Z
---

Per docs/plans/2026-09-16-reputation-design-draft.md. Custodian settled/disputed attestations carry value_band + role; judge ruled attestations carry share/late/partial; indexer computes the v1 scorecard (completion, dispute_record, responsiveness, promptness, tenure; distinct counterparty grantors; same-grantor agreements count zero), signed, aggregated to the grantor.

- [x] custodian attestation flags
- [x] judge attestation flags + ruler output
- [x] indexer scorecard + signature + key route
- [x] tests + demo-local
- [x] docs
- [x] deploy + verify

Live: custodian/judge deployed 82b9e44; indexer 4e423d5 (dropped/rebuilt derived tables at open; acceptance matched by the SDK's canonical-payload hash — the daemon's object_hash is a different hash, which had left every live acceptor grantor unknown).

## Summary of Changes

Custodian settled/disputed attestations carry role + value_band; judge ruled attestations carry share/late/partial/responded/reveals_matched (ruler outputs late_party + partial); indexer serves the signed browserid-pay-reputation-v1 scorecard (/v1/reputation, /v1/reputation/key), pooled per controlling account; demo-local asserts and verifies bob's card. Live: all three deployed; the live agreements are all controlled by one account and so count zero, by design.
