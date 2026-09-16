---
# browserid-pay-g896
title: 'Indexer: agreements by party, attestations by subject'
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:25:18Z
parent: browserid-pay-h1f5
blocked_by:
    - browserid-pay-vpe8
---

index.dsp.fyi: reads da.sandmill.org, builds the reverse index by resolved identity, serves the two verifiable queries (Indexer spec). Reputation view is a non-normative example.

- [x] reverse index (SQLite, rebuilt per poll; exact-string identities, domain lowercased)
- [x] two queries (+ providers, contribution, non-normative reputation)
- [x] references + state root per response (cheap version; full SBOQ/subtree proofs noted in crates/indexer/README.md)
- [x] deploy to index.dsp.fyi (app created + configured; image blocked on browserid-ng main carrying scope params — CI skipped)

## Summary of Changes

Deployed and serving. index.dsp.fyi answers both verifiable queries against the live 530 chain, each response carrying references, a state root and the verify block.

Verified live 2026-09-16 at state root block 3889017:

- `/v1/agreements?party=danmills+bob@sandmill.org` → 3 agreements, with acceptor, acceptor_grantor, adjudicator, custodian and the evidence chain (handoff → receipt).
- `/v1/attestations?subject=danmills+bob@sandmill.org` → 3 attestations (`agreement:settled`, `agreement:disputed`, `agreement:ruled`) with issuer and per-object references.
- `/v1/contribution` resolves `/sys/contribution` from genesis (block 3885219).
