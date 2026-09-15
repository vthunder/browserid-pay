---
# browserid-pay-vpe8
title: Chain genesis + regenesis runbook (Avail turing app 530)
status: todo
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T14:53:16Z
parent: browserid-pay-h1f5
---

Own SBO database. Genesis: /sys/domains/dsp.fyi, sys + names, /sys/trust/brokers = browserid.me, root policy (agreements/**, providers/**, attestations, /sys/contribution suggestion), /sys/dnssec at genesis. A `genesis` binary + Makefile targets + deploy/GENESIS.md modelled on mingo (regenesis-able). Key backups to ~/secure-backup/dsp-*.key. Writers submit via TurboDA directly (key is app-scoped); the existing da.sandmill.org daemon indexes 530 alongside 506.

- [x] genesis builder + wire
- [x] runbook + Makefile
- [x] submit + verify on turing (B=3885219; daemon-side verification pending the mingo deploy)
- [ ] da.sandmill.org follows app 530 (mingo commit 864229a, not pushed)
- [ ] dnssec refresh automation (make dnssec-refresh exists; cron not scheduled)
- [ ] _sbo.dsp.fyi record (Dan)

## Status 2026-09-15

Genesis v1 live: avail:turing:530:3885219:sha256:6e36ddae51fcf93557d6be930cda3849ededbdf226241af08f6f7f07e7e86d51 (Mode A, broker browserid.me). Sys key at ~/secure-backup/dsp-sys.key. See deploy/GENESIS.md for the _sbo.dsp.fyi TXT value and the regenesis procedure.
