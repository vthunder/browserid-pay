---
# browserid-pay-vpe8
title: Chain genesis + regenesis runbook (Avail turing app 530)
status: completed
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-16T16:25:18Z
parent: browserid-pay-h1f5
---

Own SBO database. Genesis: /sys/domains/dsp.fyi, sys + names, /sys/trust/brokers = browserid.me, root policy (agreements/**, providers/**, attestations, /sys/contribution suggestion), /sys/dnssec at genesis. A `genesis` binary + Makefile targets + deploy/GENESIS.md modelled on mingo (regenesis-able). Key backups to ~/secure-backup/dsp-*.key. Writers submit via TurboDA directly (key is app-scoped); the existing da.sandmill.org daemon indexes 530 alongside 506.

- [x] genesis builder + wire
- [x] runbook + Makefile
- [x] submit + verify on turing (B=3885219; daemon-side verification pending the mingo deploy)
- [x] da.sandmill.org follows app 530 (mingo 1d8c562, deployed 2026-09-15) (mingo commit 864229a, not pushed)
- [x] dnssec refresh — no cron: the daemon refreshes on demand at write time (y1yd scrapped)
- [x] _sbo.dsp.fyi record (Dan; verified live 2026-09-16)

## Status 2026-09-15

Genesis v1 live: avail:turing:530:3885219:sha256:6e36ddae51fcf93557d6be930cda3849ededbdf226241af08f6f7f07e7e86d51 (Mode A, broker browserid.me). Sys key at ~/secure-backup/dsp-sys.key. See deploy/GENESIS.md for the _sbo.dsp.fyi TXT value and the regenesis procedure.

## Summary of Changes

Genesis v1 is live and both remaining items are resolved.

- **`_sbo.dsp.fyi` is set.** Verified 2026-09-16: `dig +short TXT _sbo.dsp.fyi` returns the value in `deploy/GENESIS.md` byte for byte, anchor and genesis hash included.
- **No refresh cron is needed.** The DNSSEC proof is refreshed *on demand by the daemon*: when a write needs fresher evidence, the daemon submits the refreshed proof as part of accepting the write and reports it back on `/v1/submit` (the SDK reads `evidence_refreshed`, `sdk/agreements/src/chain.mjs:50`). Already built and deployed, so the scheduled-refresh option in `browserid-pay-y1yd` is moot; that bean is scrapped. `make dnssec-refresh` stays as a manual escape hatch.

Live state 2026-09-16: 530 proof fresh (2026-09-15 → 2026-09-24, `needs_refresh: false`); da.sandmill.org indexing 530 to block 3889017.

Note for mingo, not us: the daemon still reports mingo's own `/sys/dnssec/browserid.me` on app 506 as expired since 2026-09-03 (`needs_refresh: true`).
