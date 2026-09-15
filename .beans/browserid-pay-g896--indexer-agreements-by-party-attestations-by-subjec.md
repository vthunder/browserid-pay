---
# browserid-pay-g896
title: 'Indexer: agreements by party, attestations by subject'
status: in-progress
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T15:07:40Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-vpe8
---

index.dsp.fyi: reads da.sandmill.org, builds the reverse index by resolved identity, serves the two verifiable queries (Indexer spec). Reputation view is a non-normative example.

- [x] reverse index (SQLite, rebuilt per poll; exact-string identities, domain lowercased)
- [x] two queries (+ providers, contribution, non-normative reputation)
- [x] references + state root per response (cheap version; full SBOQ/subtree proofs noted in crates/indexer/README.md)
- [ ] deploy to index.dsp.fyi (app created + configured; image blocked on browserid-ng main carrying scope params — CI skipped)
