---
# browserid-pay-vpe8
title: Chain genesis + regenesis runbook (Avail turing app 530)
status: todo
type: task
priority: normal
created_at: 2026-09-15T14:32:06Z
updated_at: 2026-09-15T14:32:30Z
parent: browserid-pay-h1f5
---

Own SBO database. Genesis: /sys/domains/dsp.fyi, sys + names, /sys/trust/brokers = browserid.me, root policy (agreements/**, providers/**, attestations, /sys/contribution suggestion), /sys/dnssec at genesis. A `genesis` binary + Makefile targets + deploy/GENESIS.md modelled on mingo (regenesis-able). Key backups to ~/secure-backup/dsp-*.key. Writers submit via TurboDA directly (key is app-scoped); the existing da.sandmill.org daemon indexes 530 alongside 506.

- [ ] genesis builder + wire
- [ ] runbook + Makefile
- [ ] submit + verify on turing
- [ ] da.sandmill.org follows app 530
- [ ] dnssec refresh automation
- [ ] _sbo.dsp.fyi record (Dan)
