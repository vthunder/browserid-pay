---
# browserid-pay-wjik
title: Reputation scorecard (indexer) + attestation flags (custodian value band, judge late/partial/share)
status: in-progress
type: feature
priority: normal
created_at: 2026-09-16T13:10:03Z
updated_at: 2026-09-16T13:18:30Z
---

Per docs/plans/2026-09-16-reputation-design-draft.md. Custodian settled/disputed attestations carry value_band + role; judge ruled attestations carry share/late/partial; indexer computes the v1 scorecard (completion, dispute_record, responsiveness, promptness, tenure; distinct counterparty grantors; same-grantor agreements count zero), signed, aggregated to the grantor.

- [x] custodian attestation flags
- [x] judge attestation flags + ruler output
- [x] indexer scorecard + signature + key route
- [x] tests + demo-local
- [x] docs
- [ ] deploy + verify
