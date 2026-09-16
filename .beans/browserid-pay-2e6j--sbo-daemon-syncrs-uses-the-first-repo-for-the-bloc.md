---
# browserid-pay-2e6j
title: 'sbo-daemon: sync.rs uses the first repo for the block pre-state root'
status: todo
type: task
priority: normal
created_at: 2026-09-15T15:25:14Z
updated_at: 2026-09-16T16:31:25Z
---

Noted in deploy/GENESIS.md by the genesis fork; with two repos followed on one node, mingo checkpointing may be affected. Needs a look in ~/src/sbo.

Unparented from the closed build epic `browserid-pay-h1f5` on 2026-09-16: this is an sbo/mingo daemon bug that only surfaced here because dsp.fyi (530) became the second repo followed on that node. Fix lives in ~/src/sbo.
